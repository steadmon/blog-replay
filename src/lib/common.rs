use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fmt::Result as FmtResult;

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

