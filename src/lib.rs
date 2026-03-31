use image::{io::Reader, ImageFormat, imageops::FilterType};
use std::io::Cursor;

pub fn fetch_yt_thumb(id: String) -> String {
    let mut thumbnail = String::new();
    let bytes;
    match reqwest::blocking::get(format!("https://i.ytimg.com/vi/{}/default.jpg",id)) {
        Ok(b) => { bytes = b.bytes().unwrap_or_default() },
        Err(e) => { eprintln!("{}",e); return thumbnail }, 
    };
    let img;
    let mut reader = Reader::new(Cursor::new(bytes));
    reader.set_format(ImageFormat::Jpeg);
    match reader.decode() {
        Ok(i) => { img = i.resize_exact(60, 45, FilterType::Nearest).to_rgb8()},
        Err(e) => { eprintln!("{}", e); return thumbnail; },
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
                   lower_pixel[2]);
        }
        thumbnail = format!("{}\x1B[40m\n", thumbnail);
    }
    [thumbnail,"\x1B[0m".to_string()].concat().to_string()
}
