use criterion::{criterion_group, criterion_main, Criterion};
use image::{io::Reader,GenericImageView, DynamicImage, Pixel, ImageFormat, imageops::FilterType};
// use std::io::Cursor;
use bytes::Bytes;
pub fn fetch_yt_thumb(id: String) -> String {
    let mut thumbnail = String::new();
    let bytes: Bytes;
    
    // match reqwest::blocking::get(format!("https://i.ytimg.com/vi/{}/default.jpg",id)) {
    //     Ok(b) => { bytes = b.bytes().unwrap_or_default() },
    //     Err(e) => { eprintln!("{}",e); return thumbnail }, 
    // };
    let img: DynamicImage;
    // let mut reader = Reader::new(Cursor::new(bytes));
    let reader = Reader::open("/home/chlo/Documents/CodingProjects/rustStuff/ryts/default.jpg");
    match reader {
        Ok(i) => { img = i.decode().unwrap()},//.resize_exact(60, 45, FilterType::Nearest) },
        Err(e) => { eprintln!("{}", e); return thumbnail; },
    }
    // reader.set_format(ImageFormat::Jpeg);
    // match reader.decode() {
    //     Ok(i) => { img = i},//.resize_exact(60, 45, FilterType::Nearest) },
    //     Err(e) => { eprintln!("{}", e); return thumbnail; },
    // };
    let (width, height) = img.dimensions();
    let len = width * height/2; 
    // for y in 0..len {
        for x in 0..len {
            let upper_pixel = img.get_pixel(x, x % height).to_rgb();
            let lower_pixel = img.get_pixel(x, x % height + 1).to_rgb();
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
            if x % len == 0 { thumbnail += "{}\x1B[40m\n" };
        // }
    }
    [thumbnail,"\x1B[0m".to_string()].concat().to_string()
}

pub fn criterion_benchmark(c: &mut Criterion) {
    c.bench_function("Fetch yt_thumb 25", |b| b.iter(|| fetch_yt_thumb("2zOqMK9fXIw".to_string())));
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
