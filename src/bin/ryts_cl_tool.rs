#![allow(dead_code)]
#![allow(unused_imports)]
use structopt::{clap::ArgGroup, StructOpt};
use ryts::*;

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
			println!("{}", search_result.item_text);
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
						let search_result = get_channel_videos(id).await.expect("could not fetch channel videos");
						println!("{}", search_result.item_text);
						
					},
					'v' => {
						let search_result = get_video_suggestions(id).await.expect("could not get video suggestions");
						println!("{}", search_result.item_text);	
					},
					'p' => {
						let search_result = get_playlist_videos(id).await.expect("could not get playlist videos");
						println!("{}", search_result.item_text);
					},
					_ => {
						let search_result = get_video_suggestions(id).await.expect("could not get video suggestions");
						println!("{}", search_result.item_text);	
						// link = ["https://www.youtu.be/", &id].concat().to_string();
						// if cfg.thumbnail { link = "https://i.ytimg.com/vi/".to_string() + &id + "/hqdefault.jpg"}
						// println!("{}", link);
						// exit(0);
					}
				}
			},
		Subcommands::Ch(cfg) => {
			//println!("handle Ch: {:?}", cfg);
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