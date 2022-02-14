use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fmt::Result as FmtResult;

use chrono::{DateTime, FixedOffset, TimeZone};

pub use super::atom::{FeedData, write_feed};
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

pub fn parse_datetime_or_default(s: &str) -> DateTime<FixedOffset> {
    DateTime::<FixedOffset>::parse_from_rfc3339(s).unwrap_or(
        FixedOffset::east(0).ymd(1970, 1, 1).and_hms(0, 0, 0)
    )
}
