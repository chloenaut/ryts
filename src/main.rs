#![allow(dead_code)]
#![allow(unused_imports)]
#[macro_use] extern crate lazy_static;
use regex::Regex;
use select::document::Document;
use select::predicate::Name;
use std::{borrow::Cow, io::{self, Write}, process::{exit,Command,Stdio}};
use structopt::{clap::ArgGroup, StructOpt};
extern crate skim;
use skim::prelude::*;
// use dyn_clone::DynClone;

fn get_json<'a>(text:&'a str) -> Option<&'a str> {
   lazy_static! {
	  static ref RE: Regex = Regex::new(r"(?:var ytInitialData = )(?P<json>.*)(?:;)").unwrap();
   }
   RE.captures(text).and_then(|cap| {
	  cap.name("json").as_ref().map(|json| json.as_str())
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
		self.item_text = self.item_text.clone() + "(" + item_c.item_type.as_str() + ") " + item_c.name.as_str() + "\t" + item_c.id.as_str() + "\n";
	}
}

async fn get_yt_data( url: String) -> Result<String, reqwest::Error> {
	let resp = reqwest::get(url).await?.text_with_charset("utf-8").await.expect("could not fetch yt");
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
	let resp: String = get_yt_data(search_url).await?;
	let doc = Document::from_read(resp.as_bytes()).unwrap();
	for node in doc.find(Name("script")) {
		let node_text = node.text();
		let scr_txt = get_json(&node_text);
		if scr_txt != None {
			let json: serde_json::Value = serde_json::from_str(scr_txt.unwrap()).unwrap();
			let search_contents: &Vec<serde_json::Value> = json["contents"]
				["twoColumnSearchResultsRenderer"]["primaryContents"]["sectionListRenderer"]["contents"][0]
				["itemSectionRenderer"]["contents"].as_array().unwrap();
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

async fn get_playlist_videos(playlist_id: String) -> Result<ResponseList, reqwest::Error> {
	let mut result_list = ResponseList::new();
	let resp = get_yt_data(["http://www.youtube.com/playlist?list=", &playlist_id].concat()).await?;
	let doc = Document::from_read(resp.as_bytes()).unwrap();
	for node in doc.find(Name("script")) {
		let node_text = node.text();
		let scr_txt = get_json(&node_text);
		if scr_txt != None {
			let json: serde_json::Value = serde_json::from_str(scr_txt.unwrap()).unwrap();
			let search_contents = json["contents"]
				["twoColumnBrowseResultsRenderer"]["tabs"][0]
				["tabRenderer"]["content"]["sectionListRenderer"]["contents"][0]
				["itemSectionRenderer"]["contents"][0]
				["playlistVideoListRenderer"]["contents"].as_array().expect("could not get playlist json");
			for i in 0..search_contents.len() {
				if search_contents[i].get("playlistVideoRenderer") != None {
					let vid_id = search_contents[i]["playlistVideoRenderer"]["videoId"].as_str().unwrap();
					let vid_title = search_contents[i]["playlistVideoRenderer"]["title"]["runs"][0]["text"].as_str().unwrap();
					result_list.add_item(Item{id: vid_id.to_string(),name: vid_title.to_string(), item_type: "video".to_string()});
				}
			}
		}
	}
	Ok(result_list)
}

async fn get_channel_videos(channel_id: String) -> Result<ResponseList, reqwest::Error> {
	let mut result_list = ResponseList::new();
	let resp = get_yt_data(["https://www.youtube.com/channel/", &channel_id, "/videos"].concat()).await?;
	let doc = Document::from_read(resp.as_bytes()).unwrap();
	for node in doc.find(Name("script")) {
		let node_text = node.text();
		let scr_txt = get_json(&node_text);
		if scr_txt != None {		
			let json: serde_json::Value =
				serde_json::from_str(scr_txt.unwrap()).unwrap();
			let search_contents: &Vec<serde_json::Value> = json["contents"]
				["twoColumnBrowseResultsRenderer"]["tabs"][1]
				["tabRenderer"]["content"]["sectionListRenderer"]["contents"][0]
				["itemSectionRenderer"]["contents"][0]
				["gridRenderer"]["items"].as_array().unwrap();
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

fn display_prompt(prompt: ResponseList) -> Vec<String> {
    let mut abort: bool = false;
    let mut selected_name= String::new();
	let binds: Vec<&str> = vec!["esc:execute(exit 0)+abort"];
	let options = SkimOptionsBuilder::default()
		.height(Some("50%")).bind(binds)
		.build()
		.unwrap();
	let item_reader = SkimItemReader::default();
	let items = item_reader.of_bufread(io::Cursor::new(prompt.item_text));
	let output = Skim::run_with(&options, Some(items)).unwrap();
	for items in output.selected_items.iter() { selected_name = items.output().to_string(); }
	if output.is_abort { abort = true; }

    let mut selected_type = String::new();
    let mut selected_id = String::new();
    if abort { return vec![selected_id, selected_type, selected_name, "aborted".to_string()] }
    for i in 0..prompt.item_list.len() {
        let item = prompt.item_list.get(i).unwrap();
        if selected_name.contains(item.id.as_str().clone()) {
            selected_id = item.id.clone();
            selected_type = item.item_type.clone();
        }
    }
    vec![selected_id, selected_type, selected_name]
}

async fn play_video(search_result: ResponseList) {
	loop {
        let prompt_res: Vec<String> = display_prompt(search_result.clone());
		println!("{}", prompt_res.get(0).expect("could not get selection id"));
		println!("{}", prompt_res.get(1).expect("could not get selection type"));
        if prompt_res.get(3).is_some() { exit(0) }
        let selected_id: String = prompt_res.get(0).expect("could not get selection id").clone();
        let selected_type: String = prompt_res.get(1).expect("could not get selection type").clone();
        match selected_type.clone().as_str() {
			"playlist" =>{
				let playlist_videos = get_playlist_videos(selected_id).await.expect("cannot get playlist videos");
				loop {
                    let prompt = display_prompt(playlist_videos.clone());
					if prompt.get(3).is_some(){ break }
					launch_mpv("https://youtu.be/".to_string() + prompt.get(0).unwrap(), prompt.get(2).unwrap().clone());
				}
			}, //launch_mpv("https://youtube.com/playlist?list=".to_string()+ selected_id.as_str(), prompt_res.get(2).unwrap().clone()); },
            "video" => { launch_mpv("https://youtu.be/".to_string() + selected_id.as_str(), prompt_res.get(2).unwrap().clone()); },
            "channel" => {
                let channel_videos = get_channel_videos(selected_id).await.expect("cannot get channel videos");
                loop {
                    let prompt = display_prompt(channel_videos.clone());
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

fn launch_mpv(video_link: String, video_title: String) {
    println!("Playing video {}", video_title);
    let _output = Command::new("mpv")
            .arg(video_link)
            .arg("--hwdec=vaapi")
            .arg("--ytdl-format=bestvideo[ext=mp4][height<=?720]+bestaudio[ext=m4a]")
            .output().expect("failed to launch mpv");
}

#[derive(StructOpt, Debug)]
#[structopt(name="ryts", no_version)]
struct Opts {
	#[structopt(subcommand)]
	commands: Subcommands
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
	#[structopt(name="channel", short="c", help="search for channel", group="search")]
    channel: bool,
    #[structopt(name="playlist", short="p", help="search for playlist", group="search")]
    playlist: bool,
    #[structopt(name="video", short="v", help="search for video", group="search")]
    video: bool,
	#[structopt(name="subscriptions", short="s", help="get subscription list")]
	subscription: bool,
	#[structopt(name="query", required_unless("subscriptions"))]
	query: Option<String>
}

#[derive(StructOpt, Debug)]
struct IdOpts	{
	#[structopt(name="channel", short="c", help="get channel link", group="search")]
    channel: bool,
    #[structopt(name="playlist", short="p", help="get playlist link", group="search")]
    playlist: bool,
    #[structopt(name="video", short="v", help="get video link", group="search")]
    video: bool,
	#[structopt(name="thumbnails", short="t", help="get thumbnail by id")]
	thumbnail:bool,
	#[structopt(name="id", required=true)]
	id: String,
}

#[derive(StructOpt, Debug)]
struct ChOpts {
	#[structopt(name="playlist", short="p", help="get channel playlist", group="search")]
    playlist: bool,
    #[structopt(name="video", short="v", help="get channel video", group="search")]
    video: bool,
	#[structopt(name="id", required=true)]
	id: String,
}

#[derive(StructOpt, Debug)]
struct PlOpts {
	#[structopt(name="id", required=true)]
	id: String
}

async fn handle_subcommand(opt: Opts){
    match opt.commands {
		Subcommands::Se(cfg) => {
			let mut search_mod = 's';
			if cfg.video { search_mod = 'v' }
			if cfg.playlist { search_mod = 'p' }
			if cfg.channel { search_mod = 'c' }
			let query = &sanitize_query(cfg.query.unwrap()).to_string();
	 		let search_result = search_for_generic(query, search_mod).await.expect("could not fetch youtube");
			// println!("{}", search_result.item_text);
			play_video(search_result).await;
		},
		Subcommands::Id(cfg) => {
			let id = cfg.id.trim().to_string();
			let mut search_mod = 's';
			if cfg.video { search_mod = 'v' } 
			if cfg.playlist { search_mod = 'p' }
			if cfg.channel { search_mod = 'c' } 
			let mut link = String::new();
				match search_mod {
					'c' => { 
						link = ["https://www.youtube.com/channel/", &id, "/videos"].concat().to_string();
						println!("{}", link);
						exit(0);            
					},
					'v' => {
						link = ["https://www.youtu.be/", &id].concat().to_string();
						if cfg.thumbnail { link = "https://i.ytimg.com/vi/".to_string() + &id + "/hqdefault.jpg"}
						println!("{}", link);
						exit(0);
					},
					'p' => {
						link = ["https://youtube.com/playlist?list=", &id].concat().to_string();
						println!("{}", link);
						exit(0);
					},
					_ => {
						link = ["https://www.youtu.be/", &id].concat().to_string();
						if cfg.thumbnail { link = "https://i.ytimg.com/vi/".to_string() + &id + "/hqdefault.jpg"}
						println!("{}", link);
						exit(0);
					}
				}
			},
		Subcommands::Ch(cfg) => {
			if cfg.video { 
				let search_result = get_channel_videos(cfg.id).await.expect("could not get channel videos");
				println!("{}", search_result.item_text);
			}
		},
		Subcommands::Pl(cfg) => {
			let search_result = get_playlist_videos(cfg.id).await.expect("could not get playlist videos");
			println!("{}", search_result.item_text);
		}
    }
}

#[tokio::main]
async fn main() {
	let opt = Opts::from_args();
	handle_subcommand(opt).await;
}
