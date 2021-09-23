#![allow(dead_code)]
#![allow(unused_imports)]
#[macro_use]
extern crate lazy_static;
use regex::Regex;
use select::document::Document;
use select::predicate::Name;
use std::{
    borrow::Cow,
    io::{self, Write},
    process::{exit, Command, Stdio},
};
use structopt::{clap::ArgGroup, StructOpt};
extern crate skim;
use skim::prelude::*;
// use dyn_clone::DynClone;

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

impl Item {
    fn new() -> Item {
        Item {
            id: String::new(),
            name: String::new(),
            item_type: String::new(),
        }
    }
    fn get_item_text(&mut self) -> String {
        let item_text: String = format!("{:<100} {}", self.name.as_str(), self.id.as_str());
        item_text
    }
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
    item_text: String,
}

impl ResponseList {
    fn new() -> ResponseList {
        ResponseList {
            item_list: Vec::new(),
            item_text: String::new(),
        }
    }
    pub fn add_item(&mut self, item: &ListItem) {
        self.item_list.push(item.clone());
        use ListItem::*;
        match item {
            Video(v) => {
                self.item_text = format!(
                    "{}▶ {:<40} {:<100} {:<10} {}\n",
                    self.item_text.clone(),
                    &v.channel_name,
                    &v.item_data.name,
                    &v.length,
                    &v.item_data.id
                )
            }
            Playlist(p) => {
                self.item_text = format!(
                    "{}≡ {} | ▶ {}\n",
                    self.item_text.clone(),
                    &p.item_data.to_owned().get_item_text(),
                    &p.video_count.to_string()
                )
            }
            Channel(c) => {
                self.item_text = format!(
                    "{}@ {}\n",
                    self.item_text.clone(),
                    &c.item_data.to_owned().get_item_text()
                )
            }
        }
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

async fn search_for_generic(
    query: &str,
    search_type: char,
) -> Result<ResponseList, reqwest::Error> {
    let mut result_list = ResponseList::new();
    let mut search_url = ["https://www.youtube.com/results?search_query=", &query].concat();
    match search_type {
        'c' => search_url = [&search_url, "&sp=EgIQAg%253D%253D"].concat(),
        'p' => search_url = [&search_url, "&sp=EgIQAw%253D%253D"].concat(),
        'v' => search_url = [&search_url, "&sp=EgIQAQ%253D%253D"].concat(),
        _ => (),
    }
    let resp: String = get_yt_data(search_url).await?;
    let doc = Document::from_read(resp.as_bytes()).unwrap();
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
                        length: vid["ownerText"]["runs"][0]["text"]
                            .as_str()
                            .unwrap_or_default()
                            .to_string(),
                        channel_name: vid["lengthText"]["simpleText"]
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
    let resp =
        get_yt_data(["http://www.youtube.com/playlist?list=", &playlist_id].concat()).await?;
    let doc = Document::from_read(resp.as_bytes()).unwrap();
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
    let resp =
        get_yt_data(["https://www.youtube.com/channel/", &channel_id, "/videos"].concat()).await?;
    let doc = Document::from_read(resp.as_bytes()).unwrap();
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

fn display_prompt(prompt: ResponseList) -> Vec<String> {
    let mut abort: bool = false;
    let mut selected_name = String::new();
    let binds: Vec<&str> = vec!["esc:execute(exit 0)+abort"];
    let options = SkimOptionsBuilder::default()
        .height(Some("50%"))
        .bind(binds)
        .build()
        .unwrap();
    let item_reader = SkimItemReader::default();
    let items = item_reader.of_bufread(io::Cursor::new(prompt.item_text));
    let output = Skim::run_with(&options, Some(items)).unwrap();
    for items in output.selected_items.iter() {
        selected_name = items.output().to_string();
    }
    if output.is_abort {
        abort = true;
    }

    let mut selected_type = String::new();
    let mut selected_id = String::new();
    if abort {
        return vec![
            selected_id,
            selected_type,
            selected_name,
            "aborted".to_string(),
        ];
    }
    for i in 0..prompt.item_list.len() {
        let item = prompt.item_list.get(i).unwrap().clone();
        let item_data;
        match item {
            ListItem::Video(v) => item_data = v.item_data,
            ListItem::Playlist(p) => item_data = p.item_data,
            ListItem::Channel(c) => item_data = c.item_data,
        }
        if selected_name.contains(item_data.id.as_str().clone()) {
            selected_id = item_data.id.clone();
            selected_type = item_data.item_type.clone();
        }
    }
    vec![selected_id, selected_type, selected_name]
}

async fn play_video(search_result: ResponseList) {
    loop {
        let prompt_res: Vec<String> = display_prompt(search_result.clone());
        if prompt_res.get(3).is_some() {
            exit(0)
        }
        let selected_id: String = prompt_res
            .get(0)
            .expect("could not get selection id")
            .clone();
        let selected_type: String = prompt_res
            .get(1)
            .expect("could not get selection type")
            .clone();
        match selected_type.clone().as_str() {
            "playlist" => {
                let playlist_videos = get_playlist_videos(selected_id)
                    .await
                    .expect("cannot get playlist videos");
                loop {
                    let prompt = display_prompt(playlist_videos.clone());
                    if prompt.get(3).is_some() {
                        break;
                    }
                    launch_mpv(
                        "https://youtu.be/".to_string() + prompt.get(0).unwrap(),
                        prompt.get(2).unwrap().clone(),
                    );
                }
            } 
            //launch_mpv("https://youtube.com/playlist?list=".to_string()+ selected_id.as_str(), prompt_res.get(2).unwrap().clone()); },
            "video" => {
                launch_mpv(
                    "https://youtu.be/".to_string() + selected_id.as_str(),
                    prompt_res.get(2).unwrap().clone(),
                );
            }
            "channel" => {
                let channel_videos = get_channel_videos(selected_id)
                    .await
                    .expect("cannot get channel videos");
                if !channel_videos.item_list.is_empty() {
                    loop {
                        let prompt = display_prompt(channel_videos.clone());
                        if prompt.get(3).is_some() {
                            break;
                        }
                        launch_mpv(
                            "https://youtu.be/".to_string() + prompt.get(0).unwrap(),
                            prompt.get(2).unwrap().clone(),
                        );
                    }
                } else {
                    eprintln!("channel video list is empty");
                }
            }
            _ => {
                eprintln!("error getting search item type: {}", selected_type);
                exit(1);
            }
        }
    }
}

fn launch_mpv(video_link: String, video_title: String) {
    println!("Playing video {}", video_title);
    let _output = Command::new("mpv")
        .arg(video_link)
        .arg("--hwdec=vaapi")
        .arg("--ytdl-format=bestvideo[ext=mp4][height<=?720]+bestaudio[ext=m4a]")
        .output()
        .expect("failed to launch mpv");
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
            if search_result.item_text.trim().is_empty() {
                eprintln!("results are empty");
            } else {
                play_video(search_result).await;
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
                println!("{}", search_result.item_text);
            }
        }
        Subcommands::Pl(cfg) => {
            let search_result = get_playlist_videos(cfg.id)
                .await
                .expect("could not get playlist videos");
            println!("{}", search_result.item_text);
        }
    }
}

#[tokio::main]
async fn main() {
    let opt = Opts::from_args();
    handle_subcommand(opt).await;
}
