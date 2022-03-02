use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fmt::Result as FmtResult;
use std::path::{Path, PathBuf};

use chrono::{DateTime, FixedOffset, Offset, TimeZone, Utc};
use convert_case::{Case, Casing};
use lazy_static::lazy_static;
use regex::Regex;

pub use super::atom::{FeedData, read_or_create_feed};
pub use super::config::Config;

#[derive(Debug)]
pub struct ReplayError {
    pub msg: String,
    pub retryable: bool,
}

impl Display for ReplayError {
    fn fmt(&self, f: &mut Formatter) -> FmtResult {
        write!(f, "ReplayError: {}", self.msg)
    }
}

impl Error for ReplayError {
    fn source(&self) -> Option<&(dyn Error + 'static)> { None }
}

pub fn parse_datetime(s: &str) -> Option<DateTime<FixedOffset>> {
    DateTime::<FixedOffset>::parse_from_rfc3339(s).ok().map(|d| d.with_timezone(&Utc.fix()))
}

pub fn parse_datetime_or_default(s: &str) -> DateTime<FixedOffset> {
    parse_datetime(s).unwrap_or(
        Utc.fix().ymd(1970, 1, 1).and_hms(0, 0, 0)
    )
}

pub fn path_from_feed_data(config: &Config, f: &FeedData) -> PathBuf {
    Path::new(&config.feed_path).join(&f.key).with_extension("xml")
}

pub fn sanitize_blog_key(s: &String) -> String {
    lazy_static! {
        static ref SANITIZER: Regex = Regex::new(r"[^-&\[\]a-z0-9]+").unwrap();
    };
    SANITIZER.replace_all(&s.to_case(Case::Snake), "_").into_owned()
}
