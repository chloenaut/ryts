#[macro_use]
extern crate lazy_static;
use env_logger::Env;
use indicatif::ProgressBar;
use indicatif::ProgressStyle;
use structopt::{StructOpt, clap::ArgGroup};
extern crate skim;
mod ryts_util;
use crate::ryts_util::*;
mod search;
use crate::search::*;
mod search_item;
use crate::search_item::*;
use skim::prelude::*;

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
            ryts_util::play_video("https://youtu.be/".to_string() + id.as_str(), name.clone());
        }
        Key::Ctrl('t') => show_thumbnail(id.clone()),
        _ => (),
    }
}

fn prompt_loop(query: String, search_type: char, search_mod: Option<char>) {
    let loading_icon = ProgressBar::new_spinner();
    loading_icon.set_style(
        ProgressStyle::default_bar()
            .template("{spinner} {msg}")
            .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈ "),
    );
    loading_icon.set_message("Fetching Youtube Data");
    loading_icon.enable_steady_tick(120);
    // Search for query
    let results = yt_search(query, search_type, search_mod).expect("Could not search");

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
    // #[structopt(name = "id", group = ArgGroup::with_name("search"))]
    // Id(IdOpts),
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
            let query = sanitize_query(cfg.query.unwrap()).to_string();
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
    use crate::search::*;
    use crate::search_item::{ListEnum, ResponseList};
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
