use std::default::Default;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct Config {
    pub blogger_api_key: String,
    pub feed_url_base: String,
    pub feed_path: String,
    pub max_retries: usize,
    pub max_entries: Option<usize>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            blogger_api_key: "".to_string(),
            feed_url_base: "".to_string(),
            feed_path: "".to_string(),
            max_retries: 5,
            max_entries: None,
        }
    }
}
