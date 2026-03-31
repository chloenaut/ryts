use std::borrow::Cow;
use std::env;
use std::io::Write;
use std::process::Stdio;
// Play Youtube Video using MPV
pub fn play_video(video_link: String, video_title: String) {
    let mpv_command = env::var("MPV_DIR").unwrap_or("mpv".to_string());
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
    let mut mpv = cmd.spawn().expect("cannot start mpv");
    let status = mpv.wait().expect("could not get exit status of mpv");
    log::info!("the command exited with {}", status);
    if !log::log_enabled!(log::Level::Info) {
        std::io::stdout().flush().expect("could not flush")
    }
}

// Show Thumbnail with feh
pub fn show_thumbnail(id: String) {
    let _cmd = std::process::Command::new("feh")
        .arg("-B")
        .arg("Black")
        .arg("--no-fehbg")
        .arg("-Z")
        .arg(format!("https://i.ytimg.com/vi/{}/mqdefault.jpg", id))
        .stderr(Stdio::null())
        .spawn()
        .expect("feh command failed to start");
}

// Sanitizing our query input so we don't get any issues passing it to the request
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
