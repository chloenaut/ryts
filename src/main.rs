#[macro_use]
extern crate lazy_static;
use env_logger::Env;
use regex::Regex;
use select::document::Document;
use select::predicate::Name;
use std::{borrow::Cow, process::{exit, Stdio}, io::Write};
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

#[derive(Clone, Debug)]
enum ListItem {
    Video(VideoItem),
    Playlist(PlaylistItem),
    Channel(ChannelItem),
}

#[derive(Clone, Debug)]
struct Item {
    id: String,
    name: String,
    item_type: String,
}

#[derive(Clone, Debug)]
struct VideoItem {
    item_data: Item,
    length: String,
    channel_name: String,
}

#[derive(Clone, Debug)]
struct PlaylistItem {
    item_data: Item,
    video_count: i32,
}

#[derive(Clone, Debug)]
struct ChannelItem {
    item_data: Item,
}

#[derive(Clone, Debug)]
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

async fn get_search_data(search_url: String) -> String {
    let mut scr_txt: String = String::new();
    let resp_str = get_yt_data(search_url).await.ok().unwrap_or_default();
    if resp_str.is_empty() { return scr_txt }
    let doc = Document::from_read(resp_str.as_bytes()).unwrap();
    let mut node_text: String;
    for node in doc.find(Name("script")) {
        node_text = node.text();
        if let Some(sc) = get_json(&node_text) {
            scr_txt = sc.to_string();
        }
    }
    scr_txt
}

//TODO find some way to combine video search functions
async fn search_for_generic(
    query: &str,
    search_type: char,
) -> Result<ResponseList, reqwest::Error> {
    let mut result_list = ResponseList::new();
    if query.is_empty() { return Ok(result_list) }
    let mut search_url = format!("https://www.youtube.com/results?search_query={}", &query);
    //search for specific type
    match search_type {
        'c' => search_url = [&search_url, "&sp=EgIQAg%253D%253D"].concat(),
        'p' => search_url = [&search_url, "&sp=EgIQAw%253D%253D"].concat(),
        'v' => search_url = [&search_url, "&sp=EgIQAQ%253D%253D"].concat(),
        _ => (),
    }
    let scr_txt = get_search_data(search_url).await;
    if scr_txt.is_empty() { return Ok(result_list) }
    Ok(parse_generic(&mut result_list, scr_txt).clone())
}

fn parse_generic(result_list: &mut ResponseList, scr_txt: String) -> &ResponseList {
    let json: serde_json::Value =
        serde_json::from_str(&scr_txt).unwrap_or_default();
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
    result_list
}

async fn get_playlist_videos(playlist_id: String) -> Result<ResponseList, reqwest::Error> {
    let mut result_list = ResponseList::new();
    if playlist_id.is_empty() { return Ok(result_list) }
    let scr_txt = get_search_data(["http://www.youtube.com/playlist?list=", &playlist_id].concat()).await;
    if scr_txt.is_empty() { return Ok(result_list) }
    let json: serde_json::Value = serde_json::from_str(&scr_txt).unwrap_or_default();
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

    Ok(result_list)
}

async fn get_channel_videos(channel_id: String) -> Result<ResponseList, reqwest::Error> {
    let mut result_list = ResponseList::new();
    if channel_id.is_empty() { return Ok(result_list) }
    let scr_txt = get_search_data(["https://www.youtube.com/channel/", &channel_id, "/videos"].concat()).await;
    if scr_txt.is_empty() { return Ok(result_list) }
    let json: serde_json::Value = serde_json::from_str(&scr_txt).unwrap_or_default();
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
        .stderr(Stdio::null())
        .spawn()
        .expect("feh command failed to start");
}

async fn launch_mpv(video_link: String, video_title: String) {
    log::info!("Playing video {}", video_title);
    let mut cmd = Command::new("mpv");
    cmd.arg(video_link)
        .arg("--hwdec=vaapi")
        .arg("--ytdl-format=bestvideo[ext=mp4][height<=?720]+bestaudio[ext=m4a]");
    if !log::log_enabled!(log::Level::Info) { cmd.stdout(Stdio::null()); }
    let mut mpv = cmd.spawn().expect("cannot start mpv");
    let status = mpv.wait().await.expect("Exit mpv failed");
    log::info!("the command exited with {}", status);
    if !log::log_enabled!(log::Level::Info) {std::io::stdout().flush().expect("could not flush")}
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
            let mut search_mod= 's';
            if cfg.video {
                search_mod = 'v'
            }
            if cfg.playlist {
                search_mod = 'p'
            }
            if cfg.channel {
                search_mod = 'c'
            }
            log::info!("Searching for {}...", cfg.query.clone().unwrap_or_default());
            let query = &sanitize_query(cfg.query.unwrap()).to_string();
            let search_result = search_for_generic(query, search_mod)
                .await
                .expect("could not fetch youtube");
            if search_result.item_text_list.is_empty() {
                log::error!("results are empty");
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
                    },
                'v' => {
                    link = ["https://www.youtu.be/", &id].concat().to_string();
                    if cfg.thumbnail {
                        link = "https://i.ytimg.com/vi/".to_string() + &id + "/hqdefault.jpg"
                    }
                },
                'p' => {
                    link = ["https://youtube.com/playlist?list=", &id]
                        .concat()
                        .to_string();
                },
                _ => {
                    link = ["https://www.youtu.be/", &id].concat().to_string();
                    if cfg.thumbnail {
                        link = "https://i.ytimg.com/vi/".to_string() + &id + "/hqdefault.jpg"
                    }
                }
            }
            println!("{}", link);
            exit(0);
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
    let env = Env::default()
        .filter_or("MY_LOG_LEVEL", "Info")
        .write_style_or("MY_LOG_STYLE", "always");
    env_logger::init_from_env(env);
    let opt = Opts::from_args();
    handle_subcommand(opt).await;
}

#[cfg(test)]
mod tests {
    use select::{document::Document, predicate::Name};

    use crate::{ListItem, ResponseList, get_json, get_playlist_videos, parse_generic};

    #[tokio::test]
    async fn test_playlist_empty_query() {
        let playlist_videos = get_playlist_videos("".to_string()).await.expect("could not fetch");
        assert_eq!(playlist_videos.item_list.is_empty(), true);
    }

    #[test]
    fn test_parse_generic() {
        let contents = std::fs::read_to_string("./testNoFormat.html").expect("Something went wrong reading the file");
        let mut search_result = ResponseList::new();
        let mut scr_txt = String::new();
        let doc = Document::from_read(contents.as_bytes()).unwrap();
        for node in doc.find(Name("script")) {
            let node_text: String;
            node_text = node.text();
            if let Some(sc) = get_json(&node_text) {
                scr_txt = sc.to_string();
            }
        }
        search_result = parse_generic(&mut search_result, scr_txt).clone();
        let test_list = vec!["2zOqMK9fXIw","GO2F-e_D-bo","r0XoAoXo4tM", "FcUvf1R-fVY", "Vj5ZMcIHOy4", "WPX-yemalxA", "w8N4e7cfn-M","kPUAQd0NEv4", "iqdZIs7jGX8", "evXO9V0UQX4", "c0EufiNQH0c", "QVo_QIdOwQU", "OUbRIeGjeqU", "my1lUZ1M1b0", "Ig9Es7ri-Pc",  "jn_kFQIxNH8"];

        for item in search_result.item_list {
            match item {
                ListItem::Video(v) => {
                    assert!(test_list.contains(&v.item_data.id.as_str()));
                }
                _ => {},
            }
        }
    }
}

