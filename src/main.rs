#[macro_use]
extern crate lazy_static;
use env_logger::Env;
use crate::search_item::{ListItem, ListEnum, SearchResult, ResponseList};
use crate::yt_json::{ parse_generic, parse_playlist, parse_channel, get_yt_json, parse_suggestions };
use std::{borrow::Cow, io::Write, process::{exit, Stdio}, env};
use structopt::{clap::ArgGroup, StructOpt};
use tokio::process::Command;
extern crate skim;
use async_recursion::async_recursion;
use skim::prelude::*;
mod yt_json;
mod search_item;
mod search;

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


// fn disp_icat(id: String) {
//     let status = std::process::Command::new("kitty")
//         .arg("+kitten")
//         .arg("icat")
//         .arg(format!("https://i.ytimg.com/vi/{}/mqdefault.jpg", id))
//         .status().expect("failed to get icat");
//     log::info!("Exit status: {}", status);
// }

//TODO optimize thumbnail shennanigans
fn _get_ansi_thumb(id: String) -> String {
    let cmd = std::process::Command::new("pixterm")
        .arg("-tc").arg("60")
        .arg("-tr").arg("20")
        .arg(format!("https://i.ytimg.com/vi/{}/mqdefault.jpg", id))
        .output().expect("could not get thumb");
    String::from_utf8_lossy(&cmd.stdout).to_string()
}

fn get_search_mod(search_mod: char) -> String {
    match search_mod {
        'c' => "&sp=EgIQAg%253D%253D",
        'p' => "&sp=EgIQAw%253D%253D",
        'v' => "&sp=EgIQAQ%253D%253D",
        _ => (""),
    }.to_string()
}

//TODO find some way to combine video search functions
async fn yt_search(
    query: String,
    search_type: char,
    search_mod: Option<char>,
) -> Result<ResponseList, reqwest::Error> {
        let mut result_list = ResponseList::new();
        let search_url = match search_type {
        'g' => { format!("https://www.youtube.com/results?search_query={}{}", &query, get_search_mod(search_mod.unwrap_or_default()))},
        'p' => { format!("https://www.youtube.com/playlist?list={}", &query) },
        'c' => { format!("https://www.youtube.com/channel/{}/videos",&query) },
	    's' => { format!("https://www.youtube.com/watch?v={}", &query)},
        _ => {format!("https://www.youtube.com/results?search_query={}", &query)}
    };
    //search for specific type

    let scr_txt = get_yt_json(search_url).await;
    if scr_txt.is_empty() { return Ok(result_list) }
   
    return Ok(match search_type{
            'g' => parse_generic(&mut result_list, scr_txt),
            'p' => parse_playlist(&mut result_list, scr_txt),
            'c' => parse_channel(&mut result_list, scr_txt),
            's' => parse_suggestions(&mut result_list, scr_txt),
            _ =>  &result_list
        }.clone())
}

fn display_prompt(result_list: &ResponseList) -> SkimOutput {
    let header_text = "Search Results\nCtrl-P : toggle preview\nCtrl-T: show thumbnail";
    let binds: Vec<&str> = vec![
        "esc:execute(exit 0)+abort",
        "ctrl-p:toggle-preview",
        "ctrl-t:accept",
        "ctrl-space:accept"
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
    for item in &result_list.search_list {
        let _ = tx_item.send(Arc::new(item.clone()));
    }
    drop(tx_item);
    let output = Skim::run_with(&options, Some(rx_item)).unwrap();
    output
}

fn get_output_search_list(output: &SkimOutput) -> Vec<ListItem> {
    output.selected_items
        .iter()
        .map(|selected_item| {
            (**selected_item)
                .as_any()
                .downcast_ref::<SearchResult>()
                .unwrap()
                .search_data
                .to_owned()
        })
        .collect::<Vec<ListItem>>()
}

fn launch_feh(id: String) {
    let _cmd = Command::new("feh")
        .arg("-B").arg("Black")
        .arg("--no-fehbg")
        .arg("-Z")
        .arg(format!("https://i.ytimg.com/vi/{}/mqdefault.jpg", id))
        .stderr(Stdio::null())
        .spawn()
        .expect("feh command failed to start");
}

fn launch_mpv(video_link: String, video_title: String) {
    let hwdec = env::var("HWDEC_OPT").unwrap_or("--hwdec=vaapi".to_string());
    let mpv_command = env::var("MPV_DIR").unwrap_or("mpv".to_string());
    log::info!("Playing video {}", video_title);
    let mut cmd = std::process::Command::new(mpv_command);
    cmd.arg(video_link)
        .arg(hwdec)
        .arg("--ytdl-format=bestvideo[ext=mp4][height<=?720]+bestaudio[ext=m4a]");
    if !log::log_enabled!(log::Level::Info) { cmd.stdout(Stdio::null()); }
    let mut mpv = cmd.spawn().expect("cannot start mpv");
    let status = mpv.wait().expect("could not get exit status of mpv");//.await.expect("Exit mpv failed");
    log::info!("the command exited with {}", status);
    if !log::log_enabled!(log::Level::Info) {std::io::stdout().flush().expect("could not flush")}
}

#[async_recursion]
async fn play_video(id: String, name: String, key: Key) {
    match key {
        Key::Enter => { launch_mpv("https://youtu.be/".to_string() + id.as_str(), name.clone()); },
        Key::Ctrl('t') => launch_feh(id.clone()),
        Key::Ctrl(' ') => {
            let results = yt_search( id.to_owned(), 's', None)
                    .await
                    .expect("could not get playlist videos");
                loop {
                    let output = display_prompt(&results);
                    if output.is_abort { break }
                    let out_item = get_output_search_list(&output).get(0).unwrap().clone();
                    match get_output_search_list(&output).get(0).unwrap().clone().ex {
                        ListEnum::Video(_) => { play_video( out_item.id.clone(), out_item.name.clone(), output.final_key).await }
                        _ => (),
                    };
                }
        },
        _ => (),
    }
}

async fn prompt_loop(query: String, search_type: char, search_mod: Option<char>) {
    let results = match yt_search(query, search_type, search_mod).await {
        Ok(k) => { k },
        Err(e) => { eprintln!("{}",e); ResponseList::new() },
    };
    loop {
        let mut output = display_prompt(&results);
        if output.is_abort { break }
        let out_item_o = get_output_search_list(&output);
        let out_item = out_item_o.get(0).unwrap().clone();
        use ListEnum::*;
        match out_item.ex { 
            Video(_) => { play_video( out_item.id, out_item.name, output.final_key).await; },
            Playlist(_) | Channel(_) => {
                let results = yt_search(out_item.id.to_owned(), out_item.clone().get_type_char(), None)
                    .await
                    .expect("could not get playlist videos");
                loop {
                    output = display_prompt(&results);
                    if output.is_abort { break }
                    match get_output_search_list(&output).get(0).unwrap().clone().ex {
                        Video(_) => { play_video(out_item.id.clone(), out_item.name.clone(), output.final_key).await}
                        _ => (),
                    };
                }

            },
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
            let search_mod= match cfg {
                SeOpts{ video: true, .. } => { 'v' },
                SeOpts{ playlist: true, .. } => { 'p' },
                SeOpts{ channel: true, .. } => { 'c' },
                _ => { 'n' }
            };
            // log::info!("Searching for {}...", cfg.query.clone().unwrap_or_default());
            let query = sanitize_query(cfg.query.unwrap()).to_string();
            prompt_loop(query, 'g' , Some(search_mod)).await;
        }
        Subcommands::Id(cfg) => {
            let id = cfg.id.trim().to_string();
            let search_mod =  match cfg {
                IdOpts{ video: true, .. } => { 'v' },
                IdOpts{ playlist: true, .. } => { 'p' },
                IdOpts{ channel: true, .. } => { 'c' },
                _ => { 'n' }
            };
            let mut link;
            match search_mod {
                'c' => { link = format!("https://www.youtube.com/channel/{}/videos", &id) },
                'p' => { link = format!("https://youtube.com/playlist?list={}", &id) },
                _ => {
                    link = ["https://www.youtu.be/", &id].concat().to_string();
                    if cfg.thumbnail { link = format!("https://i.ytimg.com/vi/{}/hqdefault.jpg", &id) }
                },
            }
            println!("{}", link);
            exit(0);
        }
        Subcommands::Ch(cfg) => {
            if cfg.video {
                let search_result = yt_search(cfg.id, 'c', None)
                    .await
                    .expect("could not get channel videos");
                for item in search_result.search_list { println!("{}", item.search_text); }
            }
        }
        Subcommands::Pl(cfg) => {
            let search_result = yt_search(cfg.id, 'p', None)
                .await
                .expect("could not get playlist videos");
            for item in search_result.search_list { println!("{}", item.search_text); }
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
    // use select::{document::Document, predicate::Name};
    // use crate::yt_json::{strip_html_json,parse_generic};
    // use crate::search_item::{ListEnum,ResponseList};
    use crate::yt_search;

    #[tokio::test]
    async fn test_playlist_empty_query() {
        let playlist_videos = yt_search("".to_string(), 'p', None).await.expect("could not fetch");
        assert_eq!(playlist_videos.search_list.is_empty(), true);
    }

    // #[test]
    // fn test_parse_generic() {
    //     let contents = std::fs::read_to_string("./testNoFormat.html").expect("Something went wrong reading the file");
    //     let mut search_result = ResponseList::new();
    //     let mut scr_txt = String::new();
    //     let doc = Document::from_read(contents.as_bytes()).unwrap();
    //     for node in doc.find(Name("script")) {
    //         let node_text: String;
    //         node_text = node.text();
    //         if let Some(sc) = strip_html_json(&node_text) {
    //             scr_txt = sc.to_string();
    //         }
    //     }
    //     search_result = parse_generic(&mut search_result, scr_txt).clone();
    //     let test_list = vec!["2zOqMK9fXIw","GO2F-e_D-bo","r0XoAoXo4tM", "FcUvf1R-fVY", "Vj5ZMcIHOy4", "WPX-yemalxA", "w8N4e7cfn-M","kPUAQd0NEv4", "iqdZIs7jGX8", "evXO9V0UQX4", "c0EufiNQH0c", "QVo_QIdOwQU", "OUbRIeGjeqU", "my1lUZ1M1b0", "Ig9Es7ri-Pc",  "jn_kFQIxNH8"];
    //
    //     for item in search_result.search_list {
    //         match item.search_data.ex {
    //             ListEnum::Video(_) => { assert!(test_list.contains(&item.search_data.id.as_str())) }
    //             _ => {},
    //         }
    //     }
    // }
}

