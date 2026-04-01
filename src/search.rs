use crate::search_item::*;
use image::{ImageFormat, imageops::FilterType, io::Reader};
use std::io::Cursor;
type Error = Box<dyn std::error::Error>;
use rayon::prelude::*;
use regex::Regex;
use select::{document::Document, predicate::Name};

// JSON Funtions
// Get JSON From initialData variable in HTML Response
pub fn strip_html_json(text: &str) -> Option<&str> {
    lazy_static! {
        static ref RE: Regex = Regex::new(r"(?:var ytInitialData = )(?P<json>.*)(?:;)").unwrap();
    }
    RE.captures(text)
        .and_then(|cap| cap.name("json").as_ref().map(|json| json.as_str()))
}

fn get_yt_html(url: String) -> Result<String, reqwest::Error> {
    let resp = reqwest::blocking::get(url)?.text_with_charset("utf-8")?;
    Ok(resp)
}

// Process HTML Response
pub fn get_yt_json(search_url: String) -> String {
    let mut scr_txt: String = String::new();
    let resp_str_f = get_yt_html(search_url);
    let resp_str = resp_str_f.unwrap_or_default();
    if resp_str.is_empty() {
        return scr_txt;
    }
    let doc = Document::from_read(resp_str.as_bytes()).unwrap();
    let mut node_text: String;
    for node in doc.find(Name("script")) {
        node_text = node.text();
        if let Some(sc) = strip_html_json(&node_text) {
            scr_txt = sc.to_string();
        }
    }
    scr_txt
}

// Parse Generic Search
// TODO FIX PLAYLIST HANDLING BECAUSE YOUTUBE BROKE IT
pub fn parse_generic(result_list: &mut ResponseList, scr_txt: String) -> &ResponseList {
    let json: serde_json::Value = serde_json::from_str(&scr_txt).unwrap_or_default();
    let empty_ret: &Vec<serde_json::Value> = &Vec::<serde_json::Value>::new();

    let search_contents: &Vec<serde_json::Value> = json["contents"]
        ["twoColumnSearchResultsRenderer"]["primaryContents"]["sectionListRenderer"]["contents"][0]
        ["itemSectionRenderer"]["contents"]
        .as_array()
        .unwrap_or(empty_ret);

    let (sender, receiver): (
        std::sync::mpsc::Sender<ListItem>,
        std::sync::mpsc::Receiver<ListItem>,
    ) = std::sync::mpsc::channel();
    search_contents.par_iter().for_each_with(sender, |s, item| {
        let mut id = String::new();
        let mut name = String::new();
        let mut ex = ListEnum::Channel(ChannelData {});

        if item.get("videoRenderer") != None {
            let vid = item["videoRenderer"].clone();
            id = vid["videoId"].as_str().unwrap_or_default().to_string();
            name = vid["title"]["runs"][0]["text"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            ex = ListEnum::Video(VideoData {
                length: vid["lengthText"]["simpleText"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
                channel_name: vid["ownerText"]["runs"][0]["text"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
                thumbnail: String::new(),
            })
            .clone();
        } else if item.get("playlistRenderer") != None {
            let playlist = item["playlistRenderer"].clone();
            id = playlist["playlistId"].as_str().unwrap().to_string();
            name = playlist["title"]["simpleText"]
                .as_str()
                .unwrap()
                .to_string();
            ex = ListEnum::Playlist(PlaylistData {
                video_count: playlist["videoCount"]
                    .as_str()
                    .unwrap_or("0")
                    .parse()
                    .unwrap_or_default(),
            });
        } else if item.get("channelRenderer") != None {
            let channel = item["channelRenderer"].clone();
            id = channel["channelId"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            name = channel["title"]["simpleText"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            ex = ListEnum::Channel(ChannelData {})
        } else if item.get("lockupViewModel") != None {
            println!("GOT LOCKUP");
        }

        if !id.is_empty() && !name.is_empty() {
            s.send(ListItem { id, name, ex }).unwrap();
        }
    });
    receiver.iter().for_each(|x| result_list.add_item(&x));
    result_list
}

// Parse Playlist page
pub fn parse_playlist(result_list: &mut ResponseList, scr_txt: String) -> &ResponseList {
    let empty_ret: &Vec<serde_json::Value> = &Vec::<serde_json::Value>::new();
    let json: serde_json::Value = serde_json::from_str(&scr_txt).unwrap_or_default();

    let search_contents = json["contents"]["twoColumnBrowseResultsRenderer"]["tabs"][0]
        ["tabRenderer"]["content"]["sectionListRenderer"]["contents"][0]["itemSectionRenderer"]
        ["contents"][0]["playlistVideoListRenderer"]["contents"]
        .as_array()
        .unwrap_or(empty_ret);

    let (sender, receiver): (
        std::sync::mpsc::Sender<ListItem>,
        std::sync::mpsc::Receiver<ListItem>,
    ) = std::sync::mpsc::channel();
    search_contents.par_iter().for_each_with(sender, |s, item| {
        if item.get("playlistVideoRenderer") != None {
            let vid = item["playlistVideoRenderer"].clone();
            let id = vid["videoId"].as_str().unwrap_or_default().to_string();
            let name = vid["title"]["runs"][0]["text"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            let ex = ListEnum::Video(VideoData {
                length: vid["lengthText"]["simpleText"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
                channel_name: vid["shortBylineText"]["runs"][0]["text"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
                thumbnail: String::new(), //get_ansi_thumb(id.clone())
            });
            s.send(ListItem { id, name, ex }).unwrap();
        }
    });
    println!("Playlist Data:");
    receiver.iter().for_each(|x| result_list.add_item(&x));
    result_list.clone().print();
    result_list
}

// Parse Channel Page
pub fn parse_channel(result_list: &mut ResponseList, scr_txt: String) -> &ResponseList {
    let json: serde_json::Value = serde_json::from_str(&scr_txt).unwrap_or_default();
    let empty_ret = &Vec::<serde_json::Value>::new();

    let search_contents: &Vec<serde_json::Value> = json["contents"]
        ["twoColumnBrowseResultsRenderer"]["tabs"][1]["tabRenderer"]["content"]
        ["richGridRenderer"]["contents"]
        .as_array()
        .unwrap_or(empty_ret);

    for i in 0..search_contents.len() {
        if search_contents[i].get("richItemRenderer") != None {
            let vid = search_contents[i]["richItemRenderer"]["content"]["videoRenderer"].clone();
            let id = vid["videoId"].as_str().unwrap_or_default().to_string();
            result_list.add_item(&ListItem {
                id: id.clone(),
                name: vid["title"]["runs"][0]["text"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
                ex: ListEnum::Video(VideoData {
                    length: vid["thumbnailOverlays"][0]["thumbnailOverlayTimeStatusRenderer"]
                        ["text"]["simpleText"]
                        .as_str()
                        .unwrap_or_default()
                        .to_string(),
                    channel_name: json["metadata"]["channelMetadataRenderer"]["title"]
                        .as_str()
                        .unwrap_or_default()
                        .to_string(),
                    thumbnail: String::new(),
                }),
            });
        }
    }
    result_list
}

// Parse Suggested Videos
pub fn parse_suggestions(result_list: &mut ResponseList, scr_txt: String) -> &ResponseList {
    let json: serde_json::Value = serde_json::from_str(&scr_txt).unwrap_or_default();
    let empty_ret = &Vec::<serde_json::Value>::new();

    let search_contents: &Vec<serde_json::Value> = json["contents"]["twoColumnWatchNextResults"]
        ["secondaryResults"]["secondaryResults"]["results"]
        .as_array()
        .unwrap_or(empty_ret);

    for i in 0..search_contents.len() {
        if search_contents[i].get("compactVideoRenderer") != None {
            let vid = search_contents[i]["compactVideoRenderer"].clone();
            let id = vid["videoId"].as_str().unwrap_or_default().to_string();
            result_list.add_item(&ListItem {
                id: id.clone(),
                name: vid["title"]["simpleText"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
                ex: ListEnum::Video(VideoData {
                    length: vid["lengthText"]["simpleText"]
                        .as_str()
                        .unwrap_or_default()
                        .to_string(),
                    channel_name: vid["longBylineText"]["runs"][0]["text"]
                        .as_str()
                        .unwrap_or_default()
                        .to_string(),
                    thumbnail: String::new(),
                }),
            });
        }
    }
    result_list
}

// Get Search Modifier
pub fn get_search_mod(search_mod: char) -> String {
    match search_mod {
        'c' => "&sp=EgIQAg%253D%253D",
        'p' => "&sp=EgIQAw%253D%253D",
        'v' => "&sp=EgIQAQ%253D%253D",
        _ => "",
    }
    .to_string()
}

// Query youtube based on search type
// Change parsing method based on type
pub fn yt_search(
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

// Get thumbnail for video
pub fn fetch_yt_thumb(id: String) -> String {
    let mut thumbnail = String::new();
    let bytes;
    match reqwest::blocking::get(format!("https://i.ytimg.com/vi/{}/default.jpg", id)) {
        Ok(b) => bytes = b.bytes().unwrap_or_default(),
        Err(e) => {
            eprintln!("{}", e);
            return thumbnail;
        }
    };
    let img;
    let mut reader = Reader::new(Cursor::new(bytes));
    reader.set_format(ImageFormat::Jpeg);
    match reader.decode() {
        Ok(i) => img = i.resize_exact(60, 45, FilterType::Nearest).to_rgb8(),
        Err(e) => {
            eprintln!("{}", e);
            return thumbnail;
        }
    };
    let (width, height) = img.dimensions();
    for y in 0..height / 2 {
        for x in 0..width {
            let upper_pixel = img.get_pixel(x, y * 2);
            let lower_pixel = img.get_pixel(x, y * 2 + 1);
            thumbnail = format!(
                "{}\x1B[38;2;{};{};{}m\
                    \x1B[48;2;{};{};{}m\u{2580}", // ▀
                thumbnail,
                upper_pixel[0],
                upper_pixel[1],
                upper_pixel[2],
                lower_pixel[0],
                lower_pixel[1],
                lower_pixel[2]
            );
        }
        thumbnail = format!("{}\x1B[40m\n", thumbnail);
    }
    [thumbnail, "\x1B[0m".to_string()].concat().to_string()
}
