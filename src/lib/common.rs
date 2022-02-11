use std::default::Default;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fmt::Result as FmtResult;

use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize)]
pub struct Config {
    pub blogger_api_key: String,
    pub max_retries: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            blogger_api_key: "".to_string(),
            max_retries: 5,
        }
    }
}

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

#[derive(Serialize, Deserialize, Debug)]
pub struct Post {
    pub id: String,
    pub url: String,
    pub title: String,
    pub content: Option<String>,
}
