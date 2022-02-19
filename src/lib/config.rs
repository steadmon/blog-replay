use std::default::Default;

use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize)]
pub struct Config {
    pub blogger_api_key: String,
    pub feed_url_base: String,
    pub max_retries: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            blogger_api_key: "".to_string(),
            feed_url_base: "".to_string(),
            max_retries: 5,
        }
    }
}

