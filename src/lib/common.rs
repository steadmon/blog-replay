use std::path::{Path, PathBuf};

use chrono::{DateTime, FixedOffset, Offset, Utc};
use convert_case::{Case, Casing};
use lazy_static::lazy_static;
use regex::Regex;

pub use super::atom::{FeedData, read_or_create_feed};
pub use super::config::Config;

pub fn parse_datetime(s: &str) -> Option<DateTime<FixedOffset>> {
    DateTime::<FixedOffset>::parse_from_rfc3339(s).ok().map(|d| d.with_timezone(&Utc.fix()))
}

pub fn path_from_feed_data(config: &Config, f: &FeedData) -> PathBuf {
    Path::new(&config.feed_path).join(&f.key).with_extension("atom")
}

pub fn sanitize_blog_key(s: &str) -> String {
    lazy_static! {
        static ref SANITIZER: Regex = Regex::new(r"[^-&\[\]a-z0-9]+").unwrap();
    };
    SANITIZER.replace_all(&s.to_case(Case::Snake), "_").into_owned()
}
