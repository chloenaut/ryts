#[macro_use]
extern crate lazy_static;
use env_logger::Env;
use crate::search_item::{ListItem, ListEnum, SearchResult, ResponseList};
use crate::yt_json::{ parse_generic, parse_playlist, parse_channel, get_yt_json, parse_suggestions };
// use std::sync::mpsc::{Receiver, Sender};
// use std::thread::JoinHandle;
use std::{borrow::Cow, io::Write, process::{exit, Stdio}, env};
use structopt::{clap::ArgGroup, StructOpt};
// use rayon::prelude::*;
extern crate skim;
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

fn get_search_mod(search_mod: char) -> String {
    match search_mod {
        'c' => "&sp=EgIQAg%253D%253D",
        'p' => "&sp=EgIQAw%253D%253D",
        'v' => "&sp=EgIQAQ%253D%253D",
        _ => "",
    }.to_string()
}

fn yt_search(
    query: String,
    search_type: char,
    search_mod: Option<char>,
) -> Result<ResponseList, reqwest::Error> {
    let mut result_list = ResponseList::new();
    let search_url = match search_type {
        'g' => { format!("https://www.youtube.com/results?search_query={}{}", &query, get_search_mod(search_mod.unwrap_or_default())) },
        'p' => { format!("https://www.youtube.com/playlist?list={}", &query) },
        'c' => { format!("https://www.youtube.com/channel/{}/videos",&query) },
        's' => { format!("https://www.youtube.com/watch?v={}", &query) },
        _ => { format!("https://www.youtube.com/results?search_query={}", &query) }
    };

    let scr_txt = get_yt_json(search_url);
    if scr_txt.is_empty() { return Ok(result_list) }
    return Ok(
        match search_type {
            'g' => parse_generic(&mut result_list, scr_txt),
            'p' => parse_playlist(&mut result_list, scr_txt),
            'c' => parse_channel(&mut result_list, scr_txt),
            's' => parse_suggestions(&mut result_list, scr_txt),
            _ =>  &result_list
        }.clone()
    )
}

fn display_prompt<'a>(mut _result_list: &'a ResponseList) -> SkimOutput {
    let header_text = "Search Results\nCtrl-P : toggle preview\nCtrl-T: show thumbnail";
    let binds: Vec<&str> = vec![
        "esc:execute(exit 0)+abort",
        "ctrl-p:toggle-preview",
        "ctrl-t:accept",
        "ctrl-s:toggle-sort",
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
    // let result_list_c = result_list.clone();
    // let r_clone = rx_item.clone();
    // let t_clone = tx_item.clone();

    //TODO has_thumbs
    // let has_thumbs: bool = result_list_c.search_list.iter().find(|x| { x.check_thumbnail() } ).is_some();
    // let (sender,receiver): (Sender<&ResponseList>, Receiver<&ResponseList>) = std::sync::mpsc::channel();
    // if !has_thumbs {
    //     std::thread::spawn(move || {
    //         log::debug!("collector start");
    //         match result_list_c.search_list.par_iter().try_for_each_with(t_clone,|s, item|{
    //             if item.clone().check_thumbnail() { return Ok(())}
    //             if r_clone.is_empty() && !s.is_empty() { return  Ok(())};
    //                     s.send(Arc::new(item.clone().set_thumbnail()))
    //                 }) {
    //             Ok(_) => { log::debug!("got thumbnail") },
    //             Err(e) => { eprintln!("{}", e) },
    //         };
    //         drop(tx_item);
    //         let n_list: &'a ResponseList = &result_list_c;
    //         sender.send(n_list).unwrap();
    //         log::debug!("collector stopped");
    //     }).join().unwrap();

    // } else {
    _result_list.search_list.iter()
        .for_each(|item|{ 
            tx_item.send(Arc::new(item.clone())).unwrap(); 
        });
    drop(tx_item);
    // }
    let output = Skim::run_with(&options, Some(rx_item)).unwrap();
    // result_list = &receiver.recv().unwrap().clone();
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
    let _cmd = std::process::Command::new("feh")
        .arg("-B").arg("Black")
        .arg("--no-fehbg")
        .arg("-Z")
        .arg(format!("https://i.ytimg.com/vi/{}/mqdefault.jpg", id))
        .stderr(Stdio::null())
        .spawn()
        .expect("feh command failed to start");
}

fn launch_mpv(video_link: String, video_title: String) {
    let mpv_command = env::var("MPV_DIR")
        .unwrap_or("mpv".to_string());
    // let hwdec = env::var("HWDEC_OPT")
    // .unwrap_or("--hwdec=vaapi".to_string());
    log::info!("Playing video {}", video_title);
    log::info!("Video Link {}", video_link);

    let mut cmd = std::process::Command::new(mpv_command);
    cmd.arg(video_link)
        // .arg(hwdec)
        .arg("--ytdl-format=bestvideo[ext=mp4][height<=?720]+bestaudio[ext=m4a]");
    if !log::log_enabled!(log::Level::Info) {
        cmd.stdout(Stdio::null());
    }
    let mut mpv = cmd.spawn()
        .expect("cannot start mpv");
    let status = mpv.wait()
        .expect("could not get exit status of mpv");
    log::info!("the command exited with {}", status);
    if !log::log_enabled!(log::Level::Info) {
        std::io::stdout()
            .flush()
            .expect("could not flush")
    }
}

fn play_video<'a>(id: String, name: String, key: Key) {
    match key {
        Key::Enter => { launch_mpv("https://youtu.be/".to_string() + id.as_str(), name.clone()); },
        Key::Ctrl('t') => launch_feh(id.clone()),
        Key::Ctrl(' ') => {
            let results = yt_search( id.to_owned(), 's', None)
                .expect("could not get playlist videos");
            loop {
                let output = display_prompt(&results);
                if output.is_abort { break }
                let out_item = get_output_search_list(&output).get(0).unwrap().clone();
                match get_output_search_list(&output).get(0).unwrap().clone().ex {
                    ListEnum::Video(_) => { play_video( out_item.id.clone(), out_item.name.clone(), output.final_key) }
                    _ => (),
                };
            }
        },
        _ => (),
    }
}

fn prompt_loop(query: String, search_type: char, search_mod: Option<char>) {
    // let loading_icon: indicatif::ProgressBar = indicatif::ProgressBar::new_spinner();
    // loading_icon.set_style(indicatif::ProgressStyle::default_bar().template("{spinner} {msg}").tick_strings(&[".   ", "..  ", "... ", "...."]));
    // loading_icon.set_message("fetching youtube data");
    // loading_icon.finish_and_clear();
    let results = match yt_search(query, search_type, search_mod) {
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
            Video(_) => { play_video( out_item.id, out_item.name, output.final_key) },
            Playlist(_) | Channel(_) => {
                let results = yt_search(out_item.id.to_owned(), out_item.clone().get_type_char(), None)
                    .expect("could not get playlist videos");
                loop {
                    output = display_prompt(&results);
                    if output.is_abort { break }
                    let out_item_e = get_output_search_list(&output).get(0).unwrap().clone();
                    match out_item_e.ex {
                        Video(_) => { play_video(out_item_e.id.clone(), out_item_e.name.clone(), output.final_key)}
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
                #[structopt(name = "no_gui", short = "n", help = "use without fzf")]
                no_gui:bool,
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

fn handle_subcommand(opt: Opts) {
    match opt.commands {
        Subcommands::Se(cfg) => {
            let search_mod= match cfg {
                SeOpts{ video: true, .. } => { 'v' },
                SeOpts{ playlist: true, .. } => { 'p' },
                SeOpts{ channel: true, .. } => { 'c' },
                _ => { 'n' }
            };
            let query = sanitize_query(cfg.query.unwrap()).to_string();
            // log::info!("Searching for {}...", cfg.query.clone().unwrap_or_default());
            if cfg.no_gui {
                let search_result = yt_search(query, 'g', Some(search_mod)).unwrap_or(ResponseList::new());
                for item in search_result.search_list { item.print() }
            } else {
                prompt_loop(query, 'g', Some(search_mod));
            }
        }
        Subcommands::Id(cfg) => {
            let id = cfg.id.trim().to_string();
            let link = match cfg {
                IdOpts{ video: true, .. } => {
                    if cfg.thumbnail {
                        format!("https://i.ytimg.com/vi/{}/hqdefault.jpg", &id)
                    } else {

                        ["https://www.youtube.com/watch?v=", &id].concat().to_string()
                    }
                },
                IdOpts{ playlist: true, .. } => { format!("https://youtube.com/playlist?list={}", &id) },
                IdOpts{ channel: true, .. } => { format!("https://www.youtube.com/channel/{}/videos", &id) },
                _ => { String::new() }
            };
            println!("{}", link);
            exit(0);
        }
        Subcommands::Ch(cfg) => {
            let search_result = yt_search(cfg.id, 'c', None)
                .expect("could not get channel videos");
            for item in search_result.search_list { item.print() }
        }
        Subcommands::Pl(cfg) => {
            let search_result = yt_search(cfg.id, 'p', None)
                .expect("could not get playlist videos");
            for item in search_result.search_list { item.print() }
        }
    }
}

fn main() {
    let env = Env::default()
        .filter_or("MY_LOG_LEVEL", "Info")
        .write_style_or("MY_LOG_STYLE", "always");
    env_logger::init_from_env(env);
    let opt = Opts::from_args();
    handle_subcommand(opt);
}

#[cfg(test)]
mod tests {
    use select::{document::Document, predicate::Name};
    use crate::yt_json::{strip_html_json,parse_generic};
    use crate::search_item::{ListEnum,ResponseList};
    use crate::yt_search;
    use ryts::fetch_yt_thumb;

    #[test]
    fn test_playlist_empty_query() {
        let playlist_videos = yt_search("".to_string(), 'p', None).expect("could not fetch");
        assert_eq!(playlist_videos.search_list.is_empty(), true);
    }

    #[test]
    fn test_get_thumbnail(){
        let thumbnail = fetch_yt_thumb("2zOqMK9fXIw".to_string());
        assert!(thumbnail.len() != 0);
        println!("{}",thumbnail);
    }

    #[test]
    fn test_get_image_data() {
        // let mut thumbnail = String::new();
        let _bytes;
        match reqwest::blocking::get(format!("https://i.ytimg.com/vi/{}/default.jpg","2zOqMK9fXIw".to_string())) {
            Ok(b) => { _bytes = b.bytes().unwrap_or_default() },
            Err(e) => { eprintln!("{}",e); return },
        };
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
            if let Some(sc) = strip_html_json(&node_text) {
                scr_txt = sc.to_string();
            }
        }
        search_result = parse_generic(&mut search_result, scr_txt).clone();
        let test_list = vec!["2zOqMK9fXIw","GO2F-e_D-bo","r0XoAoXo4tM", "FcUvf1R-fVY", "Vj5ZMcIHOy4", "WPX-yemalxA", "w8N4e7cfn-M","kPUAQd0NEv4", "iqdZIs7jGX8", "evXO9V0UQX4", "c0EufiNQH0c", "QVo_QIdOwQU", "OUbRIeGjeqU", "my1lUZ1M1b0", "Ig9Es7ri-Pc",  "jn_kFQIxNH8"];

        for item in search_result.search_list {
            match item.search_data.ex {
                ListEnum::Video(_) => { assert!(test_list.contains(&item.search_data.id.as_str())) }
                _ => {},
            }
        }
    }
}

