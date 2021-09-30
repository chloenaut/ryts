extern crate skim;
use skim::{SkimItem, ItemPreview, PreviewContext};
use std::borrow::Cow;
use unicode_truncate::{Alignment, UnicodeTruncateStr};

#[derive(Clone, Debug)]
pub struct VideoData {
    pub length: String,
    pub channel_name: String,
    pub thumbnail: String
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
    pub ex: ListEnum
}

impl ListItem {
    pub fn get_type_char(self) -> char {
        use ListEnum::*;
        match self.ex {
            Video(_) => 'v',
            Playlist(_) => 'p',
            Channel(_) => 'c'
        }
    }
    fn _new() -> ListItem {
        ListItem {
            id: String::new(),
            name: String::new(),
            ex: ListEnum::Channel(ChannelData{})
        }
    }
}

#[derive(Clone,Debug)]
pub struct SearchResult {
    pub search_text: String,
    pub search_data: ListItem,
}

impl<'a> SkimItem for SearchResult {
    fn text(&self) -> Cow<str> {
        Cow::Borrowed(&self.search_text)
    }
    fn preview(&self, _context: PreviewContext) -> ItemPreview {
        use ListEnum::*;
        let item = &self.search_data;
        let preview_text = match item.ex.clone() {
            Video(v) => {
                format!(
                    "Video\n\nTitle: {}\nUploader: {}\nLength: {}\nID: {}\n",
                    item.name, v.channel_name, v.length, item.id, //get_ansi_thumb(item.id.clone())
                )
                            }
            Playlist(p) => {
                format!(
                    "Playlist\n\nTitle: {}\nID: {}\nVideo Count: {}",
                    item.name, item.id, p.video_count.to_string()
                )
            }
            Channel(_) => {
                format!(
                    "Channel\n\nName: {}\nID: {}",
                    item.name, item.id
                )
            }
        };
        ItemPreview::AnsiText(format!("{}", preview_text))
    }
}

#[derive(Clone, Debug)]
pub struct ResponseList {
    pub search_list: Vec<SearchResult>
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
            Channel(_) => "@" 
        };
        self.search_list.push(SearchResult{
            search_text: format!("{}{}\n", i_text, item.name.unicode_pad(100, Alignment::Left, true)),
            search_data: item.clone() 
        })
    }
}


