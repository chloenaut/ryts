#[macro_use]
extern crate lazy_static;
use crate::search_item::{ListEnum, ListItem, ResponseList, SearchResult};
use crate::yt_json::{
    get_yt_json, parse_channel, parse_generic, parse_playlist, parse_suggestions,
};
use env_logger::Env;
type Error = Box<dyn std::error::Error>;
use std::process::exit;
use structopt::{clap::ArgGroup, StructOpt};
extern crate skim;
use skim::prelude::*;

mod search;
mod search_item;
mod yt_json;

// Apply different search modifier to query string
fn get_search_mod(search_mod: char) -> String {
    match search_mod {
        'c' => "&sp=EgIQAg%253D%253D",
        'p' => "&sp=EgIQAw%253D%253D",
        'v' => "&sp=EgIQAQ%253D%253D",
        _ => "",
    }
    .to_string()
}

fn yt_search(
    query: String,
    search_type: char,
    search_mod: Option<char>,
) -> Result<ResponseList, Error> {
    let mut result_list = ResponseList::new();
    let search_url = match search_type {
        'g' => {
            format!(
                "https://www.youtube.com/results?search_query={}{}",
                &query,
                get_search_mod(search_mod.unwrap_or_default())
            )
        }
        'p' => {
            format!("https://www.youtube.com/playlist?list={}", &query)
        }
        'c' => {
            format!("https://www.youtube.com/channel/{}/videos", &query)
        }
        's' => {
            format!("https://www.youtube.com/watch?v={}", &query)
        }
        _ => {
            format!("https://www.youtube.com/results?search_query={}", &query)
        }
    };

    let scr_txt = get_yt_json(search_url);
    if scr_txt.is_empty() {
        return Err("Search returned Empty")?;
    }
    return Ok(match search_type {
        'g' => parse_generic(&mut result_list, scr_txt),
        'p' => parse_playlist(&mut result_list, scr_txt),
        'c' => parse_channel(&mut result_list, scr_txt),
        's' => parse_suggestions(&mut result_list, scr_txt),
        _ => &result_list,
    }
    .clone());
}

fn display_prompt<'a>(mut _result_list: &'a ResponseList) -> SkimOutput {
    let header_text = "Search Results\nCtrl-P : toggle preview\nCtrl-T: show thumbnail";
    let binds: Vec<&str> = vec![
        "esc:execute(exit 0)+abort",
        "ctrl-p:toggle-preview",
        "ctrl-t:accept",
        "ctrl-s:toggle-sort",
        "ctrl-space:accept",
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
    _result_list.search_list.iter().for_each(|item| {
        tx_item.send(Arc::new(item.clone())).unwrap();
    });
    drop(tx_item);
    let output = Skim::run_with(&options, Some(rx_item)).unwrap();
    output
}

fn get_output_search_list(output: &SkimOutput) -> Vec<ListItem> {
    output
        .selected_items
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

// Process video action based on key input
fn handle_video_item_actions<'a>(id: String, name: String, key: Key) {
    match key {
        Key::Enter => {
            ryts::play_video("https://youtu.be/".to_string() + id.as_str(), name.clone());
        }
        Key::Ctrl('t') => ryts::show_thumbnail(id.clone()),
        _ => (),
    }
}

fn prompt_loop(query: String, search_type: char, search_mod: Option<char>) {
    let loading_icon: indicatif::ProgressBar = indicatif::ProgressBar::new_spinner();
    loading_icon.set_style(
        indicatif::ProgressStyle::default_bar()
            .template("{spinner} {msg}")
            .tick_strings(&[".   ", "..  ", "... ", "...."]),
    );
    loading_icon.set_message("fetching youtube data");
    let results = match yt_search(query, search_type, search_mod) {
        Ok(k) => k,
        // Fix Error Handling
        Err(e) => {
            eprintln!("{}", e);
            ResponseList::new()
        }
    };
    loading_icon.finish_and_clear();
    loop {
        let mut output = display_prompt(&results);
        if output.is_abort {
            break;
        }
        let out_item_o = get_output_search_list(&output);
        let out_item = out_item_o.get(0).unwrap().clone();
        use ListEnum::*;
        match out_item.ex {
            Video(_) => handle_video_item_actions(out_item.id, out_item.name, output.final_key),
            Playlist(_) | Channel(_) => {
                let results = yt_search(
                    out_item.id.to_owned(),
                    out_item.clone().get_type_char(),
                    None,
                )
                .expect("could not get playlist videos");
                loop {
                    output = display_prompt(&results);
                    if output.is_abort {
                        break;
                    }
                    let out_item_e = get_output_search_list(&output).get(0).unwrap().clone();
                    match out_item_e.ex {
                        Video(_) => handle_video_item_actions(
                            out_item_e.id.clone(),
                            out_item_e.name.clone(),
                            output.final_key,
                        ),
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
    Search(SearchOpts),
    #[structopt(name = "id", group = ArgGroup::with_name("search"))]
    Id(IdOpts),
    #[structopt(name = "ch", group = ArgGroup::with_name("search"))]
    Channel(ChannelOpts),
    #[structopt(name = "pl")]
    Playlist(PlaylistOpts),
}

#[derive(StructOpt, Debug)]
struct SearchOpts {
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
    no_gui: bool,
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
struct ChannelOpts {
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
struct PlaylistOpts {
    #[structopt(name = "id", required = true)]
    id: String,
}

fn handle_subcommand(opt: Opts) {
    match opt.commands {
        Subcommands::Search(cfg) => {
            let search_mod = match cfg {
                SearchOpts { video: true, .. } => 'v',
                SearchOpts { playlist: true, .. } => 'p',
                SearchOpts { channel: true, .. } => 'c',
                _ => 'n',
            };
            let query = ryts::sanitize_query(cfg.query.unwrap()).to_string();
            log::info!("Searching for {}...", query);
            if cfg.no_gui {
                let search_result =
                    yt_search(query, 'g', Some(search_mod)).expect("Could Not Find Video");
                for item in search_result.search_list {
                    item.print()
                }
            } else {
                prompt_loop(query, 'g', Some(search_mod));
            }
        }
        Subcommands::Id(cfg) => {
            let id = cfg.id.trim().to_string();
            let link = match cfg {
                IdOpts { video: true, .. } => {
                    if cfg.thumbnail {
                        format!("https://i.ytimg.com/vi/{}/hqdefault.jpg", &id)
                    } else {
                        ["https://www.youtube.com/watch?v=", &id]
                            .concat()
                            .to_string()
                    }
                }
                IdOpts { playlist: true, .. } => {
                    format!("https://youtube.com/playlist?list={}", &id)
                }
                IdOpts { channel: true, .. } => {
                    format!("https://www.youtube.com/channel/{}/videos", &id)
                }
                _ => String::new(),
            };
            println!("{}", link);
            exit(0);
        }
        Subcommands::Channel(cfg) => {
            let search_result = yt_search(cfg.id, 'c', None).expect("could not get channel videos");
            for item in search_result.search_list {
                item.print()
            }
        }
        Subcommands::Playlist(cfg) => {
            let search_result =
                yt_search(cfg.id, 'p', None).expect("could not get playlist videos");
            for item in search_result.search_list {
                item.print()
            }
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
    use crate::search_item::{ListEnum, ResponseList};
    use crate::yt_json::{parse_generic, strip_html_json};
    use crate::yt_search;
    use ryts::fetch_yt_thumb;
    use select::{document::Document, predicate::Name};

    #[test]
    fn test_playlist_empty_query() {
        let playlist_videos = yt_search("".to_string(), 'p', None).expect("could not fetch");
        assert_eq!(playlist_videos.search_list.is_empty(), true);
    }

    #[test]
    fn test_get_thumbnail() {
        let thumbnail = fetch_yt_thumb("2zOqMK9fXIw".to_string());
        assert!(thumbnail.len() != 0);
        println!("{}", thumbnail);
    }

    #[test]
    fn test_get_image_data() {
        // let mut thumbnail = String::new();
        let _bytes;
        match reqwest::blocking::get(format!(
            "https://i.ytimg.com/vi/{}/default.jpg",
            "2zOqMK9fXIw".to_string()
        )) {
            Ok(b) => _bytes = b.bytes().unwrap_or_default(),
            Err(e) => {
                eprintln!("{}", e);
                return;
            }
        };
    }
    #[test]
    fn test_parse_generic() {
        let contents = std::fs::read_to_string("./testNoFormat.html")
            .expect("Something went wrong reading the file");
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
        let test_list = vec![
            "2zOqMK9fXIw",
            "GO2F-e_D-bo",
            "r0XoAoXo4tM",
            "FcUvf1R-fVY",
            "Vj5ZMcIHOy4",
            "WPX-yemalxA",
            "w8N4e7cfn-M",
            "kPUAQd0NEv4",
            "iqdZIs7jGX8",
            "evXO9V0UQX4",
            "c0EufiNQH0c",
            "QVo_QIdOwQU",
            "OUbRIeGjeqU",
            "my1lUZ1M1b0",
            "Ig9Es7ri-Pc",
            "jn_kFQIxNH8",
        ];

        for item in search_result.search_list {
            match item.search_data.ex {
                ListEnum::Video(_) => {
                    assert!(test_list.contains(&item.search_data.id.as_str()))
                }
                _ => {}
            }
        }
    }
}
