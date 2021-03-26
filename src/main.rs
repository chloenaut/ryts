#![allow(dead_code)]
#[macro_use] extern crate lazy_static;
use regex::Regex;
use select::document::Document;
use select::predicate::Name;
use indicatif::{ProgressBar, ProgressStyle};
// use tokio::{io::{AsyncBufReadExt, BufReader}, process::Command};
use std::{borrow::Cow, io, process::{exit,Command}};
extern crate skim;
use skim::prelude::*;
// extern crate clap;
// use dyn_clone::DynClone;
use structopt::{clap::ArgGroup, StructOpt};

fn get_json<'a>(text:&'a str) -> Option<&'a str> {
   lazy_static! {
      static ref RE: Regex = Regex::new(r"(?:var ytInitialData = )(?P<json>.*)(?:;)").unwrap();
   }
   RE.captures(text).and_then(|cap| {
      cap.name("json").as_ref().map(|json| json.as_str())//.to_string())
   })
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
                _ => output.push(c)
            }
        }
        Cow::Owned(output)
    } else {
        input
    }
}

#[derive(Clone)]
struct Item {
    id: String,
    name: String,
    item_type: String,
    // item_info: Box<dyn YtItem>
}

// #[derive(Clone, Default)]
// struct Playlist {

// }

// impl YtItem for Playlist {
//     fn display_info(&self) -> String {
//         let info = "id: ".to_string() + self.id.as_str();
//         info
//     }
//     fn get_id(&self) -> String {
//         self.id.clone()
//     }
//     fn get_name(&self) -> String {
//         self.title.clone()
//     }
// }

// #[derive(Clone, Default)]
// struct Video {
// }

// impl YtItem for Video {
//     fn display_info(&self) -> String {
//         let info = "id : ".to_string() + self.id.as_str();
//         info
//     }
//     fn get_id(&self) -> String {
//         self.id.clone()
//     }
//     fn get_name(&self) -> String {
//         self.title.clone()
//     }
// }

// trait YtItem: DynClone {
//     fn display_info(&self) -> String;
//     fn get_id(&self) -> String;
//     fn get_name(&self) -> String;
// }

// dyn_clone::clone_trait_object!(YtItem);

#[derive(Clone)]
struct ResponseList{
    item_list: Vec<Item>,
    item_text: String
}

impl ResponseList {
    fn new() -> ResponseList {
        ResponseList { item_list: Vec::new(), item_text: String::new() }
    }
    pub fn add_item(&mut self, item: Item) {
        let item_c = item.clone();
        self.item_list.push(item);
        self.item_text = self.item_text.clone() + item_c.name.as_str() +"\n";
    }
}

async fn get_yt_data( url: String) -> Result<String, reqwest::Error> {
    let resp = reqwest::get(url).await?
      .text_with_charset("utf-8")
      .await.expect("could not fetch yt");
    Ok(resp)
}


async fn search_for_generic(query: &str, search_type: char) -> Result<ResponseList,reqwest::Error> {
    let mut result_list = ResponseList::new();
    let mut search_url = ["https://www.youtube.com/results?search_query=", &query].concat();
    match search_type {
        'c' => search_url = [&search_url, "&sp=EgIQAg%253D%253D"].concat(),
        'p' =>  search_url = [&search_url, "&sp=EgIQAw%253D%253D"].concat(),
        'v' => search_url = [&search_url,"&sp=EgIQAQ%253D%253D" ].concat(),
        _ => ()
    }
    let loading_icon: ProgressBar = ProgressBar::new_spinner();
    loading_icon.set_style(ProgressStyle::default_bar().template("{spinner} {msg}").tick_strings(&[".   ", "..  ", "... ", "...."]));
    loading_icon.set_message("fetching youtube data");
    let resp: String = get_yt_data(search_url).await?;
    loading_icon.finish();
    let doc = Document::from_read(resp.as_bytes()).unwrap();
    for node in doc.find(Name("script")) {
        if node.text().find("var ytInitialData =") != None {
            let node_text = node.text();
            let scr_txt = get_json(&node_text).unwrap();
            let json: serde_json::Value =
                serde_json::from_str(scr_txt).unwrap();
            let search_contents: &Vec<serde_json::Value> = json["contents"]["twoColumnSearchResultsRenderer"]["primaryContents"]["sectionListRenderer"]["contents"][0]["itemSectionRenderer"]["contents"].as_array().unwrap();
            for i in 0..search_contents.len() {
                if search_contents[i].get("videoRenderer") != None {
                    let vid_id = search_contents[i]["videoRenderer"]["videoId"].as_str().unwrap();
                    let vid_title = search_contents[i]["videoRenderer"]["title"]["runs"][0]["text"].as_str().unwrap();
                    result_list.add_item(Item{id: vid_id.to_string(),name: vid_title.to_string(), item_type: "video".to_string()});
                } else if search_contents[i].get("playlistRenderer") != None {
                    let playlist_id = search_contents[i]["playlistRenderer"]["playlistId"].as_str().unwrap();
                    let playlist_title = search_contents[i]["playlistRenderer"]["title"]["simpleText"].as_str().unwrap();
                   result_list.add_item(Item{id: playlist_id.to_string(), name: playlist_title.to_string(), item_type: "playlist".to_string()});
                } else if search_contents[i].get("channelRenderer") != None {
                    let channel_id = search_contents[i]["channelRenderer"]["channelId"].as_str().unwrap();
                    let channel_title = search_contents[i]["channelRenderer"]["title"]["simpleText"].as_str().unwrap();
                    result_list.add_item(Item{id: channel_id.to_string(), name: channel_title.to_string(), item_type: "channel".to_string()});
                }
            }
        }
    }
    Ok(result_list)
}

async fn get_channel_videos(channel_id: String) -> Result<ResponseList, reqwest::Error> {
    let mut result_list = ResponseList::new();
    let loading_icon: ProgressBar = ProgressBar::new_spinner();
    loading_icon.set_style(ProgressStyle::default_bar().template("{spinner} {msg}").tick_strings(&[".   ", "..  ", "... ", "...."]));
    loading_icon.set_message("fetching youtube data");
    let resp = get_yt_data(["https://www.youtube.com/channel/", &channel_id, "/videos"].concat()).await?;
    loading_icon.finish();
    let doc = Document::from_read(resp.as_bytes()).unwrap();
    for node in doc.find(Name("script")) {
        if node.text().find("var ytInitialData =") != None {
            let node_text = node.text();
            let scr_txt = get_json(&node_text).unwrap();
            let json: serde_json::Value =
                serde_json::from_str(scr_txt).unwrap();
            let search_contents: &Vec<serde_json::Value> = json["contents"]["twoColumnBrowseResultsRenderer"]["tabs"][1]["tabRenderer"]["content"]["sectionListRenderer"]["contents"][0]["itemSectionRenderer"]["contents"][0]["gridRenderer"]["items"].as_array().unwrap();
            for i in 0..search_contents.len() {
                if search_contents[i].get("gridVideoRenderer") != None {
                    let vid_id = search_contents[i]["gridVideoRenderer"]["videoId"].as_str().unwrap();
                    let vid_title = search_contents[i]["gridVideoRenderer"]["title"]["runs"][0]["text"].as_str().unwrap();
                    result_list.add_item(Item{id: vid_id.to_string(),name: vid_title.to_string(), item_type: "video".to_string()});
                }
            }
        }
    }
   Ok(result_list)
}

// if escape pressed video is still selected //:w
fn skim_prompt(prompt: ResponseList) -> Vec<String> {
    let binds: Vec<&str> = vec!["esc:execute(exit 0)+abort"];
    let options = SkimOptionsBuilder::default()
        .height(Some("50%")).bind(binds)
        .build()
        .unwrap();
    let item_reader = SkimItemReader::default();
    let items = item_reader.of_bufread(io::Cursor::new(prompt.item_text));
    let output = Skim::run_with(&options, Some(items)).unwrap();
    let mut selected_item = String::new();
    let mut selected_type = String::new();
    let mut selected_id = String::new();
    for items in output.selected_items.iter() { selected_item = items.output().to_string(); }
        for i in 0..prompt.item_list.len() {
            let item = prompt.item_list.get(i).unwrap();
            if item.name == selected_item {
                selected_id = item.id.clone();
                selected_type = item.item_type.clone();
            }
        }
    let mut out = vec![selected_id, selected_type, selected_item];
    if output.is_abort { out.push("aborted".to_string()); }
    out
}

fn launch_mpv(video_link: String, video_title: String) {
    println!("Playing video {}", video_title);
    let _output = Command::new("mpv")
            .arg(video_link)
            .arg("--hwdec=vaapi")
            .arg("--ytdl-format=bestvideo[ext=mp4][height<=?720]+bestaudio[ext=m4a]")
            .output().expect("failed to launch mpv");
}

#[derive(StructOpt, Debug)]
#[structopt(name="ryts",group = ArgGroup::with_name("search").conflicts_with("subscriptions"))]
struct Opt {
    #[structopt(name="channel", short="c", help="search for channel", group="search")]
    channel: bool,
    #[structopt(name="playlist", short="p", help="search for playlist", group="search")]
    playlist: bool,
    #[structopt(name="video", short="v", help="search for video", group="search")]
    video: bool,
    #[structopt(name="subscriptions", short="s", help="search for video")]
    subscription: bool,
    #[structopt(name="query",required(true))]
    query: String
}

#[tokio::main]
async fn main() {
    let opt = Opt::from_args();
    // Get Args
    let mut search_mod = 's';
    if opt.channel { search_mod = 'c' }
    if opt.video { search_mod = 'v' }
    if opt.playlist { search_mod = 'p' }
    // Sanitize and search
    let query = &sanitize_query(opt.query).to_string();
    let search_result = search_for_generic(query, search_mod).await.expect("cannot fetch yt");
    // Display prompt and get selection
    loop {
        let prompt_res = skim_prompt(search_result.clone());
        if prompt_res.get(3).is_some() { exit(0) }
        let selected_id: String = prompt_res.get(0).expect("could not get selection id").clone();
        let selected_type: String = prompt_res.get(1).expect("could not get selection type").clone();
        match selected_type.clone().as_str() {
            "playlist" => { launch_mpv("https://youtube.com/playlist?list=".to_string()+ selected_id.as_str(), prompt_res.get(2).unwrap().clone()); },
            "video" => { launch_mpv("https://youtu.be/".to_string() + selected_id.as_str(), prompt_res.get(2).unwrap().clone()); },
            "channel" => {
                let channel_videos = get_channel_videos(selected_id).await.expect("cannot get channel videos");
                loop {
                    let prompt = skim_prompt(channel_videos.clone());
                    if prompt.get(3).is_some() { break }
                    launch_mpv("https://youtu.be/".to_string() + prompt.get(0).unwrap(), prompt.get(2).unwrap().clone());
                }
            },
            _ => {
                println!("error getting search item type: {}", selected_type);
                exit(1);
            }
        }
    }
}
