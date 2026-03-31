// JSON Funtions
use crate::search_item::*;
use rayon::prelude::*;
use regex::Regex;
use select::{document::Document, predicate::Name};

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
                thumbnail: String::new(), //get_ansi_thumb(id.clone())
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
        }

        if !id.is_empty() && !name.is_empty() {
            s.send(ListItem { id, name, ex }).unwrap();
        }
    });
    receiver.iter().for_each(|x| result_list.add_item(&x));
    result_list
}

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
    receiver.iter().for_each(|x| result_list.add_item(&x));
    result_list
}

pub fn parse_channel(result_list: &mut ResponseList, scr_txt: String) -> &ResponseList {
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
