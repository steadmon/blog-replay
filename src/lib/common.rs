use std::future::Future;
use std::path::{Path, PathBuf};

use chrono::{DateTime, FixedOffset, Offset, Utc};
use convert_case::{Case, Casing};
use lazy_static::lazy_static;
use regex::Regex;
use reqwest::Client;
use tokio_retry::strategy::{jitter, ExponentialBackoff};
use tokio_retry::RetryIf;

pub use super::atom::{read_or_create_feed, FeedData};
pub use super::config::Config;

pub fn parse_datetime(s: &str) -> Option<DateTime<FixedOffset>> {
    DateTime::<FixedOffset>::parse_from_rfc3339(s)
        .ok()
        .map(|d| d.with_timezone(&Utc.fix()))
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

pub async fn retry_request<F, R, T>(config: &Config, action: F) -> anyhow::Result<R>
where
    F: FnMut() -> T,
    T: Future<Output = anyhow::Result<R>>,
{
    RetryIf::spawn(
        ExponentialBackoff::from_millis(500)
            .map(jitter)
            .take(config.max_retries),
        action,
        |e: &anyhow::Error| {
            match e.downcast_ref::<reqwest::Error>() {
                Some(re) => re.status().map_or(false, |s| s.is_server_error()),
                None => false,
            }
        }
    ).await
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
