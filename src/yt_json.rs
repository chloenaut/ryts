// JSON Funtions
use regex::Regex;
use crate::search_item::*;
use select::{ document::Document, predicate::Name };
pub fn strip_html_json(text: &str) -> Option<&str> {
    lazy_static! {
        static ref RE: Regex = Regex::new(r"(?:var a = )(?P<json>.*)(?:;)").unwrap();
    }
    RE.captures(text)
        .and_then(|cap| cap.name("json").as_ref()
            .map(|json| json.as_str()))
}

async fn get_yt_html(url: String) -> Result<String, reqwest::Error> {
    let loading_icon: indicatif::ProgressBar = indicatif::ProgressBar::new_spinner();
    loading_icon.set_style(indicatif::ProgressStyle::default_bar().template("{spinner} {msg}").tick_strings(&[".   ", "..  ", "... ", "...."]));
    loading_icon.set_message("fetching youtube data");
    let resp = reqwest::get(url)
        .await?
        .text_with_charset("utf-8")
        .await;
    loading_icon.finish_and_clear();
    resp
}

pub async fn get_yt_json(search_url: String) -> String {
    let mut scr_txt: String = String::new();
    let resp_str_f = get_yt_html(search_url).await;
    let resp_str = resp_str_f.unwrap_or_default();
    if resp_str.is_empty() { return scr_txt }
    // println!("{}", strip_html_json(&scr_txt).unwrap().to_string());
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
    let pb = indicatif::ProgressBar::new(1);
    pb.set_draw_delta(1);
    let json: serde_json::Value = serde_json::from_str(&scr_txt).unwrap_or_default();
    let empty_ret: &Vec<serde_json::Value> = &Vec::<serde_json::Value>::new();
    let search_contents: &Vec<serde_json::Value> = json["contents"]
        ["twoColumnSearchResultsRenderer"]["primaryContents"]["sectionListRenderer"]
        ["contents"][0]["itemSectionRenderer"]["contents"]
        .as_array().unwrap_or(empty_ret);
    for i in 0..search_contents.len() {
        let mut id = String::new(); 
        let mut name = String::new();
        let mut ex = ListEnum::Channel(ChannelData{});
        if search_contents[i].get("videoRenderer") != None {
            let vid = search_contents[i]["videoRenderer"].clone();
            id = vid["videoId"].as_str().unwrap_or_default().to_string();
            name = vid["title"]["runs"][0]["text"].as_str().unwrap_or_default().to_string();
            ex = ListEnum::Video(VideoData{
                    length: vid["lengthText"]["simpleText"].as_str().unwrap_or_default().to_string(),
                    channel_name: vid["ownerText"]["runs"][0]["text"].as_str().unwrap_or_default().to_string(),
                    thumbnail:"".to_string() 
            }).clone();
        } else if search_contents[i].get("playlistRenderer") != None {
            let playlist = search_contents[i]["playlistRenderer"].clone();
            id = playlist["playlistId"].as_str().unwrap().to_string();
            name = playlist["title"]["simpleText"].as_str().unwrap().to_string();
            ex = ListEnum::Playlist(PlaylistData {
                    video_count: playlist["videoCount"].as_str().unwrap_or("0").parse().unwrap_or_default(),
            });
        } else if search_contents[i].get("channelRenderer") != None {
            let channel = search_contents[i]["channelRenderer"].clone();
            id = channel["channelId"].as_str().unwrap_or_default().to_string();
            name = channel["title"]["simpleText"].as_str().unwrap_or_default().to_string();
            ex = ListEnum::Channel(ChannelData{})

        }
        if !id.is_empty() && !name.is_empty() {
            result_list.add_item(&ListItem{ id: id.clone(), name: name.clone(), ex: ex.clone() });
            pb.inc_length(1);
            pb.inc(1);
        }
    }
    pb.finish_and_clear();
    result_list
}

pub fn parse_playlist(result_list: &mut ResponseList, scr_txt: String) -> &ResponseList {
    let pb = indicatif::ProgressBar::new(1);
    pb.set_draw_delta(1);
    let empty_ret: &Vec<serde_json::Value> = &Vec::<serde_json::Value>::new();
    let json: serde_json::Value = serde_json::from_str(&scr_txt).unwrap_or_default();
    let search_contents = json["contents"]["twoColumnBrowseResultsRenderer"]["tabs"][0]
        ["tabRenderer"]["content"]["sectionListRenderer"]["contents"][0]
        ["itemSectionRenderer"]["contents"][0]["playlistVideoListRenderer"]["contents"]
        .as_array().unwrap_or(empty_ret);
    for i in 0..search_contents.len() {
        if search_contents[i].get("playlistVideoRenderer") != None {
            let vid = search_contents[i]["playlistVideoRenderer"].clone();
            let id = vid["videoId"].as_str().unwrap_or_default().to_string();
            result_list.add_item(&ListItem {
                id: id.clone(),
                name: vid["title"]["runs"][0]["text"].as_str().unwrap_or_default().to_string(),
                ex: ListEnum::Video(VideoData {
                    length: vid["lengthText"]["simpleText"].as_str().unwrap_or_default().to_string(),
                    channel_name: vid["shortBylineText"]["runs"][0]["text"].as_str().unwrap_or_default().to_string(),
                    thumbnail: "".to_string()
                })
            });
            pb.inc_length(1);
            pb.inc(1);
        }
    }
    result_list
}

pub fn parse_channel(result_list: &mut ResponseList, scr_txt: String) -> &ResponseList { 
    let pb = indicatif::ProgressBar::new(1);
    pb.set_draw_delta(1);
    let json: serde_json::Value = serde_json::from_str(&scr_txt).unwrap_or_default();
    let empty_ret = &Vec::<serde_json::Value>::new();
    let search_contents: &Vec<serde_json::Value> = json["contents"]
        ["twoColumnBrowseResultsRenderer"]["tabs"][1]["tabRenderer"]["content"]
        ["sectionListRenderer"]["contents"][0]["itemSectionRenderer"]["contents"][0]
        ["gridRenderer"]["items"].as_array().unwrap_or(empty_ret);
    for i in 0..search_contents.len() {
        if search_contents[i].get("gridVideoRenderer") != None {
            let vid = search_contents[i]["gridVideoRenderer"].clone();
            let id = vid["videoId"].as_str().unwrap_or_default().to_string();
            result_list.add_item(&ListItem {
                    id: id.clone(),
                    name: vid["title"]["runs"][0]["text"].as_str().unwrap_or_default().to_string(),
                    ex: ListEnum::Video(VideoData {
                        length: vid["thumbnailOverlays"][0]["thumbnailOverlayTimeStatusRenderer"]
                            ["text"]["simpleText"].as_str().unwrap_or_default().to_string(),
                        channel_name: json["metadata"]["channelMetadataRenderer"]["title"]
                            .as_str().unwrap_or_default().to_string(),
                        thumbnail:"".to_string() 
                })
            });
            pb.inc_length(1);
            pb.inc(1);
        }
    }
    result_list
}

pub fn parse_suggestions(result_list: &mut ResponseList, scr_txt: String) -> &ResponseList {
    let json: serde_json::Value = serde_json::from_str(&scr_txt).unwrap_or_default();
    let empty_ret = &Vec::<serde_json::Value>::new();
    let search_contents: &Vec<serde_json::Value> = json["contents"]
        ["twoColumnWatchNextResults"]["secondaryResults"]["secondaryResults"]["results"].as_array().unwrap_or(empty_ret);
    for i in 0..search_contents.len() {
        if search_contents[i].get("compactVideoRenderer") != None {
            let vid = search_contents[i]["compactVideoRenderer"].clone();
            let id = vid["videoId"].as_str().unwrap_or_default().to_string();
            result_list.add_item(&ListItem {
                    id: id.clone(),
                    name: vid["title"]["simpleText"].as_str().unwrap_or_default().to_string(),
                    ex: ListEnum::Video(VideoData {
                        length: vid["lengthText"]["simpleText"].as_str().unwrap_or_default().to_string(),
                        channel_name: vid["longBylineText"]["runs"][0]["text"]
                            .as_str().unwrap_or_default().to_string(),
                        thumbnail:"".to_string() 
                })
            });
        }
    } 
    result_list
}

