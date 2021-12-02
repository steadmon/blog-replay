use std::default::Default;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fmt::Result as FmtResult;

use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize)]
pub struct Config {
    pub blogger_api_key: String,
}

impl Default for Config {
    fn default() -> Self {
        Self { blogger_api_key: "".to_string() }
    }
}

#[derive(Debug)]
pub struct ReplayError {
    pub msg: String
}

impl Display for ReplayError {
    fn fmt(&self, f: &mut Formatter) -> FmtResult {
        write!(f, "ReplayError: {}", self.msg)
    }
}

impl Error for ReplayError {
    fn source(&self) -> Option<&(dyn Error + 'static)> { None }
}

