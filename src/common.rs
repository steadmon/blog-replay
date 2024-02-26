use std::path::{Path, PathBuf};

use chrono::{DateTime, FixedOffset, NaiveDateTime, Offset, Utc};
use convert_case::{Case, Casing};
use lazy_static::lazy_static;
use regex::Regex;
use reqwest::blocking::Client;

use crate::blogger;
use crate::wordpress;

mod atom;
mod config;

pub use atom::{read_or_create_feed, FeedData};
pub use config::Config;

pub fn parse_datetime(s: &str) -> Option<DateTime<FixedOffset>> {
    DateTime::<FixedOffset>::parse_from_rfc3339(s)
        .ok()
        .map(|d| d.with_timezone(&Utc.fix()))
}

pub fn parse_assuming_utc(s: &str) -> Option<DateTime<FixedOffset>> {
    NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S")
        .ok()
        .map(|d| DateTime::<Utc>::from_utc(d, Utc).with_timezone(&Utc.fix()))
}

pub fn path_from_feed_data(config: &Config, f: &FeedData) -> PathBuf {
    Path::new(&config.feed_path)
        .join(&f.key)
        .with_extension("atom")
}

pub fn sanitize_blog_key(s: &str) -> String {
    lazy_static! {
        static ref SANITIZER: Regex = Regex::new(r"[^-&\[\]a-z0-9]+").unwrap();
    };
    SANITIZER
        .replace_all(&s.to_case(Case::Snake), "_")
        .into_owned()
}

pub fn retry_request<F, R>(config: &Config, mut action: F) -> anyhow::Result<R>
where
    F: FnMut() -> anyhow::Result<R>
{
    let base_backoff_millis = 500u64;
    let mut ret = Err(anyhow::anyhow!("max_retries must be greater than zero"));

    for i in 0 .. config.max_retries {
        if i > 0 {
            std::thread::sleep(std::time::Duration::from_millis(
                    base_backoff_millis.pow(i.try_into().unwrap())
            ));
        }
        ret = action();
        if let Err(ref e) = ret {
            if let Some(re) = e.downcast_ref::<reqwest::Error>() {
                if re.status().map_or(false, |s| s.is_server_error()) {
                    continue;
                }
            }
        }
        break;
    }
    ret
}

#[derive(Debug)]
pub enum BlogType {
    Blogger,
    Wordpress,
}

pub fn detect_blog_type(config: &Config, client: &Client, blog_url: &str)
    -> anyhow::Result<BlogType>
{
    if wordpress::detect(config, client, blog_url) {
        Ok(BlogType::Wordpress)
    } else if blogger::detect(config, client, blog_url) {
        Ok(BlogType::Blogger)
    } else {
        Err(anyhow::anyhow!("Could not determine blog type"))
    }
}

pub fn init_progress_bar(len: u64) -> indicatif::ProgressBar {
    let pb = indicatif::ProgressBar::new(len);
    pb.set_style(
        indicatif::ProgressStyle::default_bar()
            .template(
                "{spinner:.blue} [{bar:.blue}] ({pos}/{len}) \
            [elapsed: {elapsed_precise}, eta: {eta_precise}]",
            )
            .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏ ")
            .progress_chars("█▉▊▋▌▍▎▏  "),
    );
    pb.enable_steady_tick(100);
    pb
}
