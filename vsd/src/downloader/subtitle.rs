use super::mux::Stream;
use crate::{
    playlist::{MediaPlaylist, MediaType},
    utils,
};
use anyhow::{Result, anyhow};
use kdam::{BarExt, Column, RichProgress, term::Colorizer};
use reqwest::{Url, blocking::Client, header};
use std::{collections::HashMap, ffi::OsStr, fs::File, io::Write, path::PathBuf};
use vsd_mp4::text::{Mp4TtmlParser, Mp4VttParser, ttml_text_parser};

enum SubtitleType {
    Mp4Vtt,
    Mp4Ttml,
    SrtText,
    TtmlText,
    Unknown,
    VttText,
}

pub fn download_subtitle_streams(
    base_url: &Option<Url>,
    client: &Client,
    directory: Option<&PathBuf>,
    streams: &[MediaPlaylist],
    pb: &mut RichProgress,
    query: &HashMap<String, String>,
    temp_files: &mut Vec<Stream>,
) -> Result<()> {
    for stream in streams {
        if stream.media_type == MediaType::Subtitles {
            download_subtitle_stream(base_url, client, directory, stream, pb, query, temp_files)?;
        }
    }

    Ok(())
}

fn download_subtitle_stream(
    base_url: &Option<Url>,
    client: &Client,
    directory: Option<&PathBuf>,
    stream: &MediaPlaylist,
    pb: &mut RichProgress,
    query: &HashMap<String, String>,
    temp_files: &mut Vec<Stream>,
) -> Result<()> {
    pb.write(format!(
        " {} [{:>5}] {}",
        "Processing".colorize("cyan"),
        stream.media_type.to_string(),
        stream.display_stream(),
    ))?;

    if stream.segments.is_empty() {
        pb.write(format!(
            "    {} skipping stream (no segments)",
            "Warning".colorize("yellow"),
        ))?;
        return Ok(());
    }

    let mut ext = stream.extension();
    let mut codec = None;

    if let Some(codecs) = &stream.codecs {
        match codecs.as_str() {
            "vtt" => {
                ext = OsStr::new("vtt");
                codec = Some(SubtitleType::VttText);
            }
            "wvtt" => {
                ext = OsStr::new("vtt");
                codec = Some(SubtitleType::Mp4Vtt);
            }
            "stpp" | "stpp.ttml" | "stpp.ttml.im1t" | "stpp.TTML.im1t" => {
                ext = OsStr::new("srt");
                codec = Some(SubtitleType::Mp4Ttml);
            }
            _ => (),
        }
    }

    let mut temp_file = PathBuf::new();
    let mut first_run = true;
    let mut subs_data = vec![];

    let stream_base_url = base_url
        .clone()
        .unwrap_or(stream.uri.parse::<Url>().unwrap());

    for segment in &stream.segments {
        if let Some(map) = &segment.map {
            let url = stream_base_url.join(&map.uri)?;
            let mut request = client.get(url).query(query);

            if let Some(range) = &map.range {
                request = request.header(header::RANGE, range.as_header_value());
            }

            let response = request.send()?;
            let bytes = response.bytes()?;
            subs_data.extend_from_slice(&bytes);
        }

        let url = stream_base_url.join(&segment.uri)?;
        let mut request = client.get(url).query(query);

        if let Some(range) = &segment.range {
            request = request.header(header::RANGE, range.as_header_value());
        }

        let response = request.send()?;
        let bytes = response.bytes()?;
        subs_data.extend_from_slice(&bytes);

        if first_run {
            first_run = false;

            if codec.is_none() {
                if subs_data.starts_with(b"WEBVTT") || ext == "vtt" {
                    ext = OsStr::new("vtt");
                    codec = Some(SubtitleType::VttText);
                } else if subs_data.starts_with(b"1") || ext == "srt" {
                    ext = OsStr::new("srt");
                    codec = Some(SubtitleType::SrtText);
                } else if subs_data.starts_with(b"<?xml") || subs_data.starts_with(b"<tt") || ext == "ttml" {
                    ext = OsStr::new("srt");
                    codec = Some(SubtitleType::TtmlText);
                } else if Mp4VttParser::parse_init(&subs_data).is_ok() {
                    ext = OsStr::new("vtt");
                    codec = Some(SubtitleType::Mp4Vtt);
                } else if Mp4TtmlParser::parse_init(&subs_data).is_ok() {
                    ext = OsStr::new("srt");
                    codec = Some(SubtitleType::Mp4Ttml);
                } else {
                    pb.write(format!(
                        "    {} unknown subtitle codec used",
                        "Warning".colorize("yellow"),
                    ))?;
                    ext = OsStr::new("txt");
                    codec = Some(SubtitleType::Unknown);
                }
            }

            temp_file = stream.path(directory, ext);
            temp_files.push(Stream {
                language: stream.language.clone(),
                media_type: stream.media_type.clone(),
                path: temp_file.clone(),
            });
            pb.write(format!(
                "{} {}",
                "Downloading".colorize("bold green"),
                temp_file.to_string_lossy()
            ))?;
        }

        pb.replace(
            0,
            Column::Text(format!(
                "[bold blue]{}",
                utils::format_bytes(subs_data.len(), 2).2
            )),
        );
        pb.update(1)?;
    }

    match codec {
        Some(SubtitleType::Mp4Vtt) => {
            pb.write(format!(" {} wvtt subs", "Extracting".colorize("cyan")))?;
            let vtt = Mp4VttParser::parse_init(&subs_data)?;
            let subs = vtt.parse_media(&subs_data, None)?;
            File::create(&temp_file)?.write_all(subs.as_vtt().as_bytes())?;
        }
        Some(SubtitleType::Mp4Ttml) => {
            pb.write(format!(" {} stpp subs", "Extracting".colorize("cyan")))?;
            let ttml = Mp4TtmlParser::parse_init(&subs_data)?;
            let subs = ttml.parse_media(&subs_data)?;
            File::create(&temp_file)?.write_all(subs.as_srt().as_bytes())?;
        }
        Some(SubtitleType::TtmlText) => {
            pb.write(format!(" {} ttml+xml subs", "Extracting".colorize("cyan")))?;
            let xml = String::from_utf8(subs_data)
                .map_err(|_| anyhow!("cannot decode subs as valid utf-8 data."))?;
            let ttml = ttml_text_parser::parse(&xml).map_err(|x| {
                anyhow!(
                    "couldn't parse xml string as ttml content.\n\n{}\n\n{:#?}",
                    xml,
                    x,
                )
            })?;
            File::create(&temp_file)?.write_all(ttml.into_subtitles().as_srt().as_bytes())?;
        }
        _ => File::create(&temp_file)?.write_all(&subs_data)?,
    };

    pb.write(format!(
        " {} stream successfully",
        "Downloaded".colorize("bold green"),
    ))?;
    Ok(())
}
