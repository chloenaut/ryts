#![allow(unused_imports)]
#[macro_use]
extern crate lazy_static;
use futures::{
    future::{self, BoxFuture},
    Future, FutureExt, StreamExt,
};
use regex::Regex;
use reqwest::Response;
use select::document::Document;
use select::predicate::Name;
use std::{
    borrow::Cow,
    io::{self, BufRead, Error, ErrorKind, Write},
    pin::Pin,
    process::{exit, Stdio},
};
use structopt::{clap::ArgGroup, StructOpt};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
};
extern crate skim;
use skim::prelude::*;
use unicode_truncate::{Alignment, UnicodeTruncateStr};

fn get_json<'a>(text: &'a str) -> Option<&'a str> {
    lazy_static! {
        static ref RE: Regex = Regex::new(r"(?:var ytInitialData = )(?P<json>.*)(?:;)").unwrap();
    }
    RE.captures(text)
        .and_then(|cap| cap.name("json").as_ref().map(|json| json.as_str()))
}

pub fn sanitize_query<'a, S: Into<Cow<'a, str>>>(input: S) -> Cow<'a, str> {
    let input = input.into();
    fn is_replace(c: char) -> bool {
        c == '+' || c == '#' || c == '&' || c == ' '
    }
    let first = input.find(is_replace);
    if let Some(first) = first {
        let mut output = String::from(&input[0..first]);
        output.reserve(input.len() - first);
        let rest = input[first..].chars();
        for c in rest {
            match c {
                '+' => output.push_str("%2B"),
                '#' => output.push_str("%23"),
                '&' => output.push_str("%26"),
                ' ' => output.push_str("+"),
                _ => output.push(c),
            }
        }
        Cow::Owned(output)
    } else {
        input
    }
}

#[derive(Clone)]
enum ListItem {
    Video(VideoItem),
    Playlist(PlaylistItem),
    Channel(ChannelItem),
}

#[derive(Clone)]
struct Item {
    id: String,
    name: String,
    item_type: String,
}

#[derive(Clone)]
struct VideoItem {
    item_data: Item,
    length: String,
    channel_name: String,
}

#[derive(Clone)]
struct PlaylistItem {
    item_data: Item,
    video_count: i32,
}

#[derive(Clone)]
struct ChannelItem {
    item_data: Item,
}

#[derive(Clone)]
struct ResponseList {
    item_list: Vec<ListItem>,
    item_text_list: Vec<String>,
}

impl ResponseList {
    fn new() -> ResponseList {
        ResponseList {
            item_list: Vec::new(),
            item_text_list: Vec::new(),
        }
    }
    pub fn add_item(&mut self, item: &ListItem) {
        self.item_list.push(item.clone());
        use ListItem::*;
        match item {
            Video(v) => {
                let mut trunc_title = v.item_data.name.clone();
                let trunc_len = 100;
                if trunc_title.len() > trunc_len - 3 {
                    trunc_title = trunc_title
                        .unicode_truncate(trunc_len - 3)
                        .0
                        .trim_end()
                        .to_string();
                    trunc_title = format!("{}...", trunc_title);
                }
                trunc_title = trunc_title
                    .unicode_pad(trunc_len, Alignment::Left, true)
                    .to_string();

                self.item_text_list.push(format!("▶ {}\n", trunc_title,));
            }
            Playlist(p) => {
                let mut trunc_title = p.item_data.name.clone();
                let trunc_len = 100;
                if trunc_title.len() > trunc_len - 3 {
                    trunc_title = trunc_title
                        .unicode_truncate(trunc_len - 3)
                        .0
                        .trim_end()
                        .to_string();
                    trunc_title = format!("{}...", trunc_title);
                }
                trunc_title = trunc_title
                    .unicode_pad(trunc_len, Alignment::Left, true)
                    .to_string();
                self.item_text_list.push(format!("≡ {}\n", trunc_title,));
            }
            Channel(c) => {
                self.item_text_list.push(format!(
                    "@ {}\n",
                    &c.item_data.name.unicode_pad(50, Alignment::Left, true),
                ));
            }
        }
    }
}

#[derive(Clone)]
struct ResponseSkimItem {
    inner: String,
    inner_item: ListItem,
}

impl<'a> SkimItem for ResponseSkimItem {
    fn text(&self) -> Cow<str> {
        Cow::Borrowed(&self.inner)
    }
    fn preview(&self, _context: PreviewContext) -> ItemPreview {
        let preview_text;
        use ListItem::*;
        match &self.inner_item {
            Video(v) => {
                preview_text = format!(
                    "Video\n\nTitle: {}\nUploader: {}\nLength: {}\nID: {}\n",
                    &v.item_data.name, &v.channel_name, &v.length, &v.item_data.id
                )
            }
            Playlist(p) => {
                preview_text = format!(
                    "Playlist\n\nTitle: {}\nID: {}\nVideo Count: {}",
                    &p.item_data.name,
                    &p.item_data.id,
                    &p.video_count.to_string()
                )
            }
            Channel(c) => {
                preview_text = format!(
                    "Channel\n\nName: {}\nID: {}",
                    &c.item_data.name, &c.item_data.id
                )
            }
        }
        ItemPreview::Text(format!("{}", preview_text))
    }
}

async fn get_yt_data(url: String) -> Result<String, reqwest::Error> {
    let resp = reqwest::get(url)
        .await?
        .text_with_charset("utf-8")
        .await
        .expect("could not fetch yt");
    Ok(resp)
}

//TODO find some way to combine video search functions
async fn search_for_generic(
    query: &str,
    search_type: char,
) -> Result<ResponseList, reqwest::Error> {
    let mut result_list = ResponseList::new();
    let mut search_url = ["https://www.youtube.com/results?search_query=", &query].concat();
    //search for specific type
    match search_type {
        'c' => search_url = [&search_url, "&sp=EgIQAg%253D%253D"].concat(),
        'p' => search_url = [&search_url, "&sp=EgIQAw%253D%253D"].concat(),
        'v' => search_url = [&search_url, "&sp=EgIQAQ%253D%253D"].concat(),
        _ => (),
    }
    let resp_str = get_yt_data(search_url).await.ok().unwrap_or_default();
    if resp_str.is_empty() {
        return Ok(result_list);
    }
    let doc = Document::from_read(resp_str.as_bytes()).unwrap();
    for node in doc.find(Name("script")) {
        let node_text = node.text();
        let scr_txt = get_json(&node_text);
        if scr_txt != None {
            let json: serde_json::Value =
                serde_json::from_str(scr_txt.unwrap_or_default()).unwrap();
            let empty_ret: &Vec<serde_json::Value> = &Vec::<serde_json::Value>::new();
            let search_contents: &Vec<serde_json::Value> = json["contents"]
                ["twoColumnSearchResultsRenderer"]["primaryContents"]["sectionListRenderer"]
                ["contents"][0]["itemSectionRenderer"]["contents"]
                .as_array()
                .unwrap_or(empty_ret);
            for i in 0..search_contents.len() {
                if search_contents[i].get("videoRenderer") != None {
                    let vid = search_contents[i]["videoRenderer"].clone();
                    result_list.add_item(&ListItem::Video(VideoItem {
                        item_data: Item {
                            id: vid["videoId"].as_str().unwrap_or_default().to_string(),
                            name: vid["title"]["runs"][0]["text"]
                                .as_str()
                                .unwrap_or_default()
                                .to_string(),
                            item_type: "video".to_string(),
                        },
                        length: vid["lengthText"]["simpleText"]
                            .as_str()
                            .unwrap_or_default()
                            .to_string(),
                        channel_name: vid["ownerText"]["runs"][0]["text"]
                            .as_str()
                            .unwrap_or_default()
                            .to_string(),
                    }));
                } else if search_contents[i].get("playlistRenderer") != None {
                    let playlist = search_contents[i]["playlistRenderer"].clone();
                    let playlist_vid_count: i32 = playlist["videoCount"]
                        .as_str()
                        .unwrap_or("0")
                        .parse()
                        .unwrap_or_default();
                    if playlist_vid_count > 1 {
                        result_list.add_item(&ListItem::Playlist(PlaylistItem {
                            item_data: Item {
                                id: playlist["playlistId"].as_str().unwrap().to_string(),
                                name: playlist["title"]["simpleText"]
                                    .as_str()
                                    .unwrap()
                                    .to_string(),
                                item_type: "playlist".to_string(),
                            },
                            video_count: playlist_vid_count,
                        }));
                    }
                } else if search_contents[i].get("channelRenderer") != None {
                    let channel = search_contents[i]["channelRenderer"].clone();
                    result_list.add_item(&ListItem::Channel(ChannelItem {
                        item_data: Item {
                            id: channel["channelId"]
                                .as_str()
                                .unwrap_or_default()
                                .to_string(),
                            name: channel["title"]["simpleText"]
                                .as_str()
                                .unwrap_or_default()
                                .to_string(),
                            item_type: "channel".to_string(),
                        },
                    }));
                }
            }
        }
    }
    Ok(result_list)
}

async fn get_playlist_videos(playlist_id: String) -> Result<ResponseList, reqwest::Error> {
    let mut result_list = ResponseList::new();
    let resp_str: String =
        get_yt_data(["http://www.youtube.com/playlist?list=", &playlist_id].concat())
            .await
            .ok()
            .unwrap_or_default();
    if resp_str.is_empty() {
        return Ok(result_list);
    }
    let doc = Document::from_read(resp_str.as_bytes()).unwrap();
    for node in doc.find(Name("script")) {
        let node_text = node.text();
        let scr_txt = get_json(&node_text);
        if scr_txt != None {
            let json: serde_json::Value = serde_json::from_str(scr_txt.unwrap()).unwrap();
            let search_contents = json["contents"]["twoColumnBrowseResultsRenderer"]["tabs"][0]
                ["tabRenderer"]["content"]["sectionListRenderer"]["contents"][0]
                ["itemSectionRenderer"]["contents"][0]["playlistVideoListRenderer"]["contents"]
                .as_array()
                .expect("could not get playlist json");
            for i in 0..search_contents.len() {
                if search_contents[i].get("playlistVideoRenderer") != None {
                    let vid = search_contents[i]["playlistVideoRenderer"].clone();
                    result_list.add_item(&ListItem::Video(VideoItem {
                        item_data: Item {
                            id: vid["videoId"].as_str().unwrap_or_default().to_string(),
                            name: vid["title"]["runs"][0]["text"]
                                .as_str()
                                .unwrap_or_default()
                                .to_string(),
                            item_type: "video".to_string(),
                        },
                        length: vid["lengthText"]["simpleText"]
                            .as_str()
                            .unwrap_or_default()
                            .to_string(),
                        channel_name: vid["shortBylineText"]["runs"][0]["text"]
                            .as_str()
                            .unwrap_or_default()
                            .to_string(),
                    }));
                }
            }
        }
    }
    Ok(result_list)
}

async fn get_channel_videos(channel_id: String) -> Result<ResponseList, reqwest::Error> {
    let mut result_list = ResponseList::new();
    let resp_str =
        get_yt_data(["https://www.youtube.com/channel/", &channel_id, "/videos"].concat())
            .await
            .expect("couldn't fetch yt");
    if resp_str.is_empty() {
        return Ok(result_list);
    }
    let doc = Document::from_read(resp_str.as_bytes()).unwrap();
    for node in doc.find(Name("script")) {
        let node_text = node.text();
        let scr_txt = get_json(&node_text);
        if scr_txt != None {
            let json: serde_json::Value = serde_json::from_str(scr_txt.unwrap()).unwrap();
            let empty_ret = &Vec::<serde_json::Value>::new();
            let search_contents: &Vec<serde_json::Value> = json["contents"]
                ["twoColumnBrowseResultsRenderer"]["tabs"][1]["tabRenderer"]["content"]
                ["sectionListRenderer"]["contents"][0]["itemSectionRenderer"]["contents"][0]
                ["gridRenderer"]["items"]
                .as_array()
                .unwrap_or(empty_ret);
            for i in 0..search_contents.len() {
                if search_contents[i].get("gridVideoRenderer") != None {
                    let vid = search_contents[i]["gridVideoRenderer"].clone();
                    result_list.add_item(&ListItem::Video(VideoItem {
                        item_data: Item {
                            id: vid["videoId"].as_str().unwrap_or_default().to_string(),
                            name: vid["title"]["runs"][0]["text"]
                                .as_str()
                                .unwrap_or_default()
                                .to_string(),
                            item_type: "video".to_string(),
                        },
                        length: vid["thumbnailOverlays"][0]["thumbnailOverlayTimeStatusRenderer"]
                            ["text"]["simpleText"]
                            .as_str()
                            .unwrap_or_default()
                            .to_string(),
                        channel_name: json["metadata"]["channelMetadataRenderer"]["title"]
                            .as_str()
                            .unwrap_or_default()
                            .to_string(),
                    }));
                }
            }
        }
    }
    Ok(result_list)
}

fn display_prompt(prompt: &ResponseList) -> SkimOutput {
    if prompt.item_list.len() == 0 {
        exit(0)
    }
    let header_text = "Search Results\nCtrl-P : toggle preview\nCtrl-T: show thumbnail";
    let binds: Vec<&str> = vec![
        "esc:execute(exit 0)+abort",
        "ctrl-p:toggle-preview",
        "ctrl-t:accept",
    ];
    let options = SkimOptionsBuilder::default()
        .height(Some("100%"))
        .bind(binds)
        .preview(Some(""))
        .preview_window(Some("wrap"))
        .header(Some(&header_text))
        .reverse(true)
        .build()
        .unwrap();
    let (tx_item, rx_item): (SkimItemSender, SkimItemReceiver) = unbounded();
    for i in 0..prompt.item_list.len() {
        let _ = tx_item.send(Arc::new(ResponseSkimItem {
            inner: prompt.item_text_list[i].to_string(),
            inner_item: prompt.item_list[i].clone(),
        }));
    }
    drop(tx_item);
    let output = Skim::run_with(&options, Some(rx_item)).unwrap();
    output
}

fn get_output_item_list(output: &SkimOutput) -> Vec<ListItem> {
    output
        .selected_items
        .iter()
        .map(|selected_item| {
            (**selected_item)
                .as_any()
                .downcast_ref::<ResponseSkimItem>()
                .unwrap()
                .inner_item
                .to_owned()
        })
        .collect::<Vec<ListItem>>()
}

fn launch_feh(id: String) {
    let _cmd = Command::new("feh")
        .arg("-B")
        .arg("Black")
        .arg("--no-fehbg")
        .arg("-Z")
        .arg(format!("https://i.ytimg.com/vi/{}/mqdefault.jpg", id))
        .spawn()
        .expect("feh command failed to start");
}

async fn play_video(id: String, name: String, key: Key) {
    match key {
        Key::Enter => launch_mpv("https://youtu.be/".to_string() + id.as_str(), name.clone()).await,
        Key::Ctrl('t') => launch_feh(id.clone()),
        _ => (),
    }
}

async fn prompt_result(prompt: ResponseList) {
    loop {
        let mut output = display_prompt(&prompt);
        if output.is_abort {
            break;
        }
        match get_output_item_list(&output).get(0).unwrap() {
            ListItem::Video(v) => {
                play_video(
                    v.item_data.id.clone(),
                    v.item_data.name.clone(),
                    output.final_key,
                )
                .await
            }
            ListItem::Playlist(p) => {
                let playlist_videos = get_playlist_videos(p.item_data.id.to_owned())
                    .await
                    .expect("could not get playlist videos");
                loop {
                    output = display_prompt(&playlist_videos);
                    if output.is_abort {
                        break;
                    }
                    match get_output_item_list(&output).get(0).unwrap().clone() {
                        ListItem::Video(v) => {
                            play_video(v.item_data.id, v.item_data.name, output.final_key).await
                        }
                        _ => (),
                    };
                }
            }
            ListItem::Channel(c) => {
                let channel_videos = get_channel_videos(c.item_data.id.to_owned())
                    .await
                    .expect("could not get channel_videos");
                loop {
                    output = display_prompt(&channel_videos);
                    if output.is_abort {
                        break;
                    }
                    match get_output_item_list(&output).get(0).unwrap().clone() {
                        ListItem::Video(v) => {
                            play_video(v.item_data.id, v.item_data.name, output.final_key).await
                        }
                        _ => (),
                    };
                }
            }
        }
    }
}

async fn launch_mpv(video_link: String, video_title: String) {
    println!("Playing video {}", video_title);
    let mut mpv = Command::new("mpv")
        .arg(video_link)
        .arg("--hwdec=vaapi")
        .arg("--ytdl-format=bestvideo[ext=mp4][height<=?720]+bestaudio[ext=m4a]")
        .stdout(Stdio::piped())
        .spawn()
        .expect("cannot start mpv");
    let stdout = mpv.stdout.take().expect("no stdout");

    let mut lines = BufReader::new(stdout).lines();
    while let Ok(line) = lines.next_line().await {
        if Some(mpv.try_wait().is_ok()).is_some() {
            break;
        }
        match line {
            Some(k) => println!("{}", k),
            None => {}
        }
    }
    let _ = mpv.wait().await.expect("Exiting mpv failed");
}

#[derive(StructOpt, Debug)]
#[structopt(name = "ryts", no_version)]
struct Opts {
    #[structopt(subcommand)]
    commands: Subcommands,
}

#[derive(StructOpt, Debug)]
#[structopt(name = "subcommands", about = "subcommands list")]
enum Subcommands {
    #[structopt(name = "se", group = ArgGroup::with_name("search").conflicts_with_all(&["subscriptions"]))]
    Se(SeOpts),
    #[structopt(name = "id", group = ArgGroup::with_name("search"))]
    Id(IdOpts),
    #[structopt(name = "ch", group = ArgGroup::with_name("search"))]
    Ch(ChOpts),
    #[structopt(name = "pl")]
    Pl(PlOpts),
}

#[derive(StructOpt, Debug)]
struct SeOpts {
    #[structopt(
        name = "channel",
        short = "c",
        help = "search for channel",
        group = "search"
    )]
    channel: bool,
    #[structopt(
        name = "playlist",
        short = "p",
        help = "search for playlist",
        group = "search"
    )]
    playlist: bool,
    #[structopt(
        name = "video",
        short = "v",
        help = "search for video",
        group = "search"
    )]
    video: bool,
    #[structopt(name = "subscriptions", short = "s", help = "get subscription list")]
    subscription: bool,
    #[structopt(name = "query", required_unless("subscriptions"))]
    query: Option<String>,
}

#[derive(StructOpt, Debug)]
struct IdOpts {
    #[structopt(
        name = "channel",
        short = "c",
        help = "get channel link",
        group = "search"
    )]
    channel: bool,
    #[structopt(
        name = "playlist",
        short = "p",
        help = "get playlist link",
        group = "search"
    )]
    playlist: bool,
    #[structopt(name = "video", short = "v", help = "get video link", group = "search")]
    video: bool,
    #[structopt(name = "thumbnails", short = "t", help = "get thumbnail by id")]
    thumbnail: bool,
    #[structopt(name = "id", required = true)]
    id: String,
}

#[derive(StructOpt, Debug)]
struct ChOpts {
    #[structopt(
        name = "playlist",
        short = "p",
        help = "get channel playlist",
        group = "search"
    )]
    playlist: bool,
    #[structopt(
        name = "video",
        short = "v",
        help = "get channel video",
        group = "search"
    )]
    video: bool,
    #[structopt(name = "id", required = true)]
    id: String,
}

#[derive(StructOpt, Debug)]
struct PlOpts {
    #[structopt(name = "id", required = true)]
    id: String,
}

async fn handle_subcommand(opt: Opts) {
    match opt.commands {
        Subcommands::Se(cfg) => {
            let mut search_mod = 's';
            if cfg.video {
                search_mod = 'v'
            }
            if cfg.playlist {
                search_mod = 'p'
            }
            if cfg.channel {
                search_mod = 'c'
            }
            println!("Searching for {}...", cfg.query.clone().unwrap_or_default());
            let query = &sanitize_query(cfg.query.unwrap()).to_string();
            let search_result = search_for_generic(query, search_mod)
                .await
                .expect("could not fetch youtube");
            if search_result.item_text_list.is_empty() {
                eprintln!("results are empty");
            } else {
                let resp_list = search_result.clone();
                prompt_result(resp_list).await;
            }
        }
        Subcommands::Id(cfg) => {
            let id = cfg.id.trim().to_string();
            let mut search_mod = 's';
            if cfg.video {
                search_mod = 'v'
            }
            if cfg.playlist {
                search_mod = 'p'
            }
            if cfg.channel {
                search_mod = 'c'
            }
            let mut link;
            match search_mod {
                'c' => {
                    link = ["https://www.youtube.com/channel/", &id, "/videos"]
                        .concat()
                        .to_string();
                    println!("{}", link);
                    exit(0);
                }
                'v' => {
                    link = ["https://www.youtu.be/", &id].concat().to_string();
                    if cfg.thumbnail {
                        link = "https://i.ytimg.com/vi/".to_string() + &id + "/hqdefault.jpg"
                    }
                    println!("{}", link);
                    exit(0);
                }
                'p' => {
                    link = ["https://youtube.com/playlist?list=", &id]
                        .concat()
                        .to_string();
                    println!("{}", link);
                    exit(0);
                }
                _ => {
                    link = ["https://www.youtu.be/", &id].concat().to_string();
                    if cfg.thumbnail {
                        link = "https://i.ytimg.com/vi/".to_string() + &id + "/hqdefault.jpg"
                    }
                    println!("{}", link);
                    exit(0);
                }
            }
        }
        Subcommands::Ch(cfg) => {
            if cfg.video {
                let search_result = get_channel_videos(cfg.id)
                    .await
                    .expect("could not get channel videos");
                for item in search_result.item_text_list {
                    println!("{}", item);
                }
            }
        }
        Subcommands::Pl(cfg) => {
            let search_result = get_playlist_videos(cfg.id)
                .await
                .expect("could not get playlist videos");
            for item in search_result.item_text_list {
                println!("{}", item);
            }
        }
    }
}

#[tokio::main]
async fn main() {
    let opt = Opts::from_args();
    handle_subcommand(opt).await;
}
