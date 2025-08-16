use crate::{
    automation::{Prompter, SelectOptions},
    cookie::{CookieJar, CookieParam},
    downloader::{self, Decrypter},
};
use anyhow::Result;
use clap::Args;
use cookie::Cookie;
use reqwest::{
    Proxy, Url,
    blocking::Client,
    header::{HeaderMap, HeaderName, HeaderValue},
};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

type CookieParams = Vec<CookieParam>;

/// Download DASH and HLS playlists.
#[derive(Args, Clone, Debug)]
pub struct Save {
    /// http(s):// | .mpd | .xml | .m3u8
    #[arg(required = true)]
    pub input: String,

    /// Base url to be used for building absolute url to segment.
    /// This flag is usually needed for local input files.
    /// By default redirected playlist url is used.
    #[arg(long)]
    pub base_url: Option<Url>,

    /// Change directory path for temporarily downloaded files.
    /// By default current working directory is used.
    #[arg(short, long)]
    pub directory: Option<PathBuf>,

    /// Mux all downloaded streams to a video container (.mp4, .mkv, etc.) using ffmpeg.
    /// Note that existing files will be overwritten and downloaded streams will be deleted.
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Parse playlist and returns it in json format.
    /// Note that --output flag is ignored when this flag is used.
    #[arg(long)]
    pub parse: bool,

    /// Force some specific subtitle codec when muxing through ffmpeg.
    /// By default `mov_text` is used for .mp4 and `copy` for others.
    #[arg(long, default_value = "copy")]
    pub subs_codec: String,

    /// Prompt for custom streams selection with modern style input prompts. By default proceed with defaults.
    #[arg(short, long, help_heading = "Automation Options")]
    pub interactive: bool,

    /// Prompt for custom streams selection with raw style input prompts. By default proceed with defaults.
    #[arg(long, help_heading = "Automation Options")]
    pub interactive_raw: bool,

    /// List all the streams present inside the playlist.
    #[arg(short, long, help_heading = "Automation Options")]
    pub list_streams: bool,

    /// Filters to be applied for automatic stream selection.
    #[arg(
        short,
        long,
        help_heading = "Automation Options",
        default_value = "v=best:s=en",
        long_help = "Filters to be applied for automatic stream selection.\n\nSYNTAX: `v={}:a={}:s={}` where `{}` (in priority order) can contain\n|> all: select all streams.\n|> skip: skip all streams or select inverter.\n|> 1,2: ids obtained by --list-streams flag.\n|> 1080p,1280x720: stream resolution.\n|> en,fr: stream language.\n\nEXAMPLES:\n|> v=skip:a=skip:s=all (download all sub streams)\n|> a:en:s=en (prefer en lang)\n|> v=1080p:a=all:s=skip (1080p with all audio streams)"
    )]
    pub select_streams: String,

    /// Fill request client with some existing cookies value.
    /// Cookies value can be same as document.cookie or in json format same as puppeteer.
    #[arg(long, help_heading = "Client Options", default_value = "[]", hide_default_value = true, value_parser = cookie_parser)]
    pub cookies: CookieParams,

    /// Custom headers for requests.
    /// This option can be used multiple times.
    #[arg(long, help_heading = "Client Options", num_args = 2, value_names = &["KEY", "VALUE"])]
    pub header: Vec<String>, // Vec<(String, String)> not supported

    /// Skip checking and validation of site certificates.
    #[arg(long, help_heading = "Client Options")]
    pub no_certificate_checks: bool,

    /// Set http(s) / socks proxy address for requests.
    #[arg(long, help_heading = "Client Options", value_parser = proxy_address_parser)]
    pub proxy: Option<Proxy>,

    /// Set query parameters for requests.
    #[arg(long, help_heading = "Client Options", default_value = "", hide_default_value = true, value_parser = query_parser)]
    pub query: HashMap<String, String>,

    /// Fill request client with some existing cookies per domain.
    /// First value for this option is set-cookie header and second value is url which was requested to send this set-cookie header.
    /// EXAMPLE: --set-cookie "foo=bar; Domain=yolo.local" https://yolo.local.
    /// This option can be used multiple times.
    #[arg(long, help_heading = "Client Options", num_args = 2, value_names = &["SET_COOKIE", "URL"])]
    pub set_cookie: Vec<String>, // Vec<(String, String)> not supported

    /// Update and set user agent header for requests.
    #[arg(
        long,
        help_heading = "Client Options",
        default_value = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/137.0.0.0 Safari/537.36"
    )]
    pub user_agent: String,

    /// Keys for decrypting encrypted streams.
    /// KID:KEY should be specified in hex format.
    #[arg(long, help_heading = "Decrypt Options", value_name = "KID:KEY;...", default_value = "", hide_default_value = true, value_parser = keys_parser)]
    pub keys: Decrypter,

    /// Download encrypted streams without decrypting them.
    /// Note that --output flag is ignored if this flag is used.
    #[arg(long, help_heading = "Decrypt Options")]
    pub no_decrypt: bool,

    /// Maximum number of retries to download an individual segment.
    #[arg(long, help_heading = "Download Options", default_value_t = 15)]
    pub retries: u8,

    /// Download streams without merging them.
    /// Note that --output flag is ignored if this flag is used.
    #[arg(long, help_heading = "Download Options")]
    pub no_merge: bool,

    /// Total number of threads for parllel downloading of segments.
    /// Number of threads should be in range 1-16 (inclusive).
    #[arg(short, long, help_heading = "Download Options", default_value_t = 5, value_parser = clap::value_parser!(u8).range(1..=16))]
    pub threads: u8,
}

impl Save {
    fn client(&self) -> Result<Client> {
        let mut client_builder = Client::builder()
            .cookie_store(true)
            .danger_accept_invalid_certs(self.no_certificate_checks)
            .user_agent(&self.user_agent)
            .timeout(std::time::Duration::from_secs(60));

        if !self.header.is_empty() {
            let mut headers = HeaderMap::new();

            for i in (0..self.header.len()).step_by(2) {
                headers.insert(
                    self.header[i].parse::<HeaderName>()?,
                    self.header[i + 1].parse::<HeaderValue>()?,
                );
            }

            client_builder = client_builder.default_headers(headers);
        }

        if let Some(proxy) = &self.proxy {
            client_builder = client_builder.proxy(proxy.clone());
        }

        let mut jar = CookieJar::new();

        if !self.set_cookie.is_empty() {
            for i in (0..self.set_cookie.len()).step_by(2) {
                jar.add_cookie_str(&self.set_cookie[i], &self.set_cookie[i + 1].parse::<Url>()?);
            }
        }

        for cookie in &self.cookies {
            if let Some(url) = &cookie.url {
                jar.add_cookie_str(&format!("{}", cookie.as_cookie()), &url.parse::<Url>()?);
            } else {
                jar.add_cookie(cookie.as_cookie());
            }
        }

        let client = client_builder.cookie_provider(Arc::new(jar)).build()?;
        Ok(client)
    }

    pub fn execute(self) -> Result<()> {
        let client = self.client()?;

        let prompter = Prompter {
            interactive: self.interactive,
            interactive_raw: self.interactive_raw,
        };

        let meta = downloader::fetch_playlist(
            self.base_url.clone(),
            &client,
            &self.input,
            &prompter,
            &self.query,
        )?;

        if self.list_streams {
            downloader::list_all_streams(&meta)?;
        } else if self.parse {
            let playlist =
                downloader::parse_all_streams(self.base_url.clone(), &client, &meta, &self.query)?;
            serde_json::to_writer(std::io::stdout(), &playlist)?;
        } else {
            let streams = downloader::parse_selected_streams(
                self.base_url.clone(),
                &client,
                &meta,
                &prompter,
                &self.query,
                SelectOptions::parse(&self.select_streams),
            )?;

            downloader::download(
                self.base_url,
                client,
                self.keys,
                self.directory,
                self.no_decrypt,
                self.no_merge,
                self.output,
                self.query,
                streams,
                self.subs_codec,
                self.retries,
                self.threads,
            )?;
        }

        Ok(())
    }
}

fn cookie_parser(s: &str) -> Result<CookieParams, String> {
    if Path::new(s).exists() {
        Ok(serde_json::from_slice::<CookieParams>(
            &std::fs::read(s).map_err(|_| format!("could not read {s}."))?,
        )
        .map_err(|_| "could not deserialize cookies from json file.")?)
    } else if let Ok(cookies) = serde_json::from_str::<CookieParams>(s) {
        Ok(cookies)
    } else {
        let mut cookies = vec![];
        for cookie in Cookie::split_parse(s) {
            match cookie {
                Ok(x) => cookies.push(CookieParam::new(x.name(), x.value())),
                Err(_) => return Err("could not split parse cookies.".to_owned()),
            }
        }
        Ok(cookies)
    }
}

fn keys_parser(s: &str) -> Result<Decrypter, String> {
    if s.is_empty() {
        return Ok(Decrypter::None);
    }

    let mut kid_key_pairs = HashMap::new();

    for pair in s.split(';') {
        if let Some((kid, key)) = pair.split_once(':') {
            let kid = kid.to_ascii_lowercase().replace('-', "");
            let key = key.to_ascii_lowercase().replace('-', "");

            if kid.len() == 32
                && key.len() == 32
                && kid.chars().all(|c| c.is_ascii_hexdigit())
                && key.chars().all(|c| c.is_ascii_hexdigit())
            {
                kid_key_pairs.insert(kid, key);
            } else {
                return Err("invalid kid key format used.".to_owned());
            }
        }
    }

    Ok(Decrypter::Mp4Decrypt(kid_key_pairs))
}

fn proxy_address_parser(s: &str) -> Result<Proxy, String> {
    Proxy::all(s).map_err(|x| x.to_string())
}

fn query_parser(s: &str) -> Result<HashMap<String, String>, String> {
    let mut queries = HashMap::new();

    if s.is_empty() {
        return Ok(queries);
    }

    for pair in s.split('&') {
        let mut parts = pair.splitn(2, '=');
        if let (Some(key), Some(value)) = (parts.next(), parts.next()) {
            queries.insert(key.to_owned(), value.to_owned());
        }
    }

    Ok(queries)
}
