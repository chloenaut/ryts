extern crate skim;
use skim::{ItemPreview, PreviewContext, SkimItem};
use std::borrow::Cow;
use unicode_truncate::{Alignment, UnicodeTruncateStr};

/**pub enum QueryType {
    General,
    Video,
    Playlist,
    Channel,
    Suggestions,
}
*/
#[derive(Clone, Debug)]
pub struct VideoData {
    pub length: String,
    pub channel_name: String,
    pub thumbnail: String,
}

#[derive(Clone, Debug)]
pub struct PlaylistData {
    pub video_count: i32,
}

#[derive(Clone, Debug)]
pub struct ChannelData {
    // subscriber_count: i32
}

#[derive(Clone, Debug)]
pub enum ListEnum {
    Video(VideoData),
    Playlist(PlaylistData),
    Channel(ChannelData),
}

#[derive(Clone, Debug)]
pub struct ListItem {
    pub id: String,
    pub name: String,
    pub ex: ListEnum,
}

impl ListItem {
    pub fn get_type_char(self) -> char {
        use ListEnum::*;
        match self.ex {
            Video(_) => 'v',
            Playlist(_) => 'p',
            Channel(_) => 'c',
        }
    }
    fn _new() -> ListItem {
        ListItem {
            id: String::new(),
            name: String::new(),
            ex: ListEnum::Channel(ChannelData {}),
        }
    }
}

#[derive(Clone, Debug)]
pub struct SearchResult {
    pub search_text: String,
    pub search_data: ListItem,
}

//TODO optimize thumbnail shennanigans
// pub fn get_ansi_thumb(id: String) -> String {
// let cmd = std::process::Command::new("pixterm")
//     .arg("-tc").arg("60")
//     .arg("-tr").arg("20")
//     .arg(format!("https://i.ytimg.com/vi/{}/default.jpg", id))
//     .output().expect("could not get thumb");
// String::from_utf8_lossy(&cmd.stdout).to_string()
// }

impl SearchResult {
    pub fn _set_thumbnail(&mut self) -> SearchResult {
        match &self.search_data.ex {
            ListEnum::Video(v) => {
                let new_ex = ListEnum::Video(VideoData {
                    thumbnail: ryts::fetch_yt_thumb(self.search_data.id.clone()),
                    ..v.clone()
                });
                self.search_data = ListItem {
                    ex: new_ex,
                    ..self.search_data.clone()
                };
            }
            _ => {}
        }
        self.clone()
    }
    pub fn _check_thumbnail(&self) -> bool {
        match self.search_data.ex.clone() {
            ListEnum::Video(v) => !v.thumbnail.is_empty(),
            _ => false,
        }
    }
    pub fn print(self) {
        println!(
            "{} {}\t {}",
            self.search_data.clone().get_type_char(),
            self.search_data
                .name
                .unicode_pad(100, Alignment::Left, true),
            self.search_data.id
        );
    }
}

impl<'a> SkimItem for SearchResult {
    fn text(&self) -> Cow<'_, str> {
        Cow::Borrowed(&self.search_text)
    }
    fn preview(&self, _context: PreviewContext) -> ItemPreview {
        use ListEnum::*;
        let item = &self.search_data;
        let preview_text = match item.ex.clone() {
            Video(v) => {
                format!(
                    "Video\n\nTitle: {}\nUploader: {}\nLength: {}\nID: {}\n{}",
                    item.name,
                    v.channel_name,
                    v.length,
                    item.id,
                    v.thumbnail //fetch_yt_thumb(item.id.clone())
                )
            }
            Playlist(p) => {
                format!(
                    "Playlist\n\nTitle: {}\nID: {}\nVideo Count: {}",
                    item.name,
                    item.id,
                    p.video_count.to_string()
                )
            }
            Channel(_) => {
                format!("Channel\n\nName: {}\nID: {}", item.name, item.id)
            }
        };
        ItemPreview::AnsiText(format!("{}", preview_text))
    }
}

#[derive(Clone, Debug)]
pub struct ResponseList {
    pub search_list: Vec<SearchResult>,
}

impl ResponseList {
    pub fn new() -> ResponseList {
        ResponseList {
            search_list: Vec::new(),
        }
    }
    pub fn add_item(&mut self, item: &ListItem) {
        use ListEnum::*;
        let i_text = match item.ex {
            Video(_) => "▶",
            Playlist(_) => "≡",
            Channel(_) => "@",
        };
        self.search_list.push(SearchResult {
            search_text: format!(
                "{} {}\n",
                i_text,
                item.name.unicode_pad(100, Alignment::Left, true)
            ),
            search_data: item.clone(),
        })
    }
}
