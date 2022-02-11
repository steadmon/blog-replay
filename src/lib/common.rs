use std::default::Default;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fmt::Result as FmtResult;
use std::fs::File;
use std::path::Path;

use atom_syndication::{Entry, EntryBuilder, FeedBuilder, FixedDateTime};
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

impl From<Post> for Entry {
    fn from(post: Post) -> Self {
        EntryBuilder::default()
            .title(post.title)
            .id("blah")
            .updated(FixedDateTime::parse_from_rfc3339("2020-07-25T22:10:00-07:00").unwrap())
            .build()
    }
}

pub fn write_feed<P: AsRef<Path>>(path: P, posts: Vec<Post>) -> Result<(), Box<dyn Error>> {
    let f = File::create(path)?;
    let feed = FeedBuilder::default()
        .title("Re-hosted MM")
        .id("https://feeds.steadmon.net/feed-replay/blah")
        .updated(FixedDateTime::parse_from_rfc3339("2020-07-25T22:10:00-07:00").unwrap())
        .entries(posts.into_iter().map(|p| p.into()).collect::<Vec<Entry>>())
        .build();
    feed.write_to(&f)?;

    Ok(())
}
