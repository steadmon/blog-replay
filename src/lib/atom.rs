use std::error::Error;
use std::io::{BufReader, ErrorKind};
use std::fs::File;
use std::path::Path;

use atom_syndication::{Feed, FeedBuilder, Generator, LinkBuilder};
use chrono::Utc;
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Clone)]
pub struct FeedData {
    pub id: String,
    pub key: String,
    pub title: String,
    pub url: String,
}

pub fn read_or_create_feed<P: AsRef<Path>>(
    path: P, gen: &Generator, feed_data: &FeedData)
-> Result<Feed, Box<dyn Error>>
{
    match File::open(path) {
        Ok(f) => Ok(Feed::read_from(BufReader::new(f))?),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(feed_from_metadata(gen, feed_data.clone())),
        Err(e) => Err(Box::new(e)),
    }
}

fn feed_from_metadata(gen: &Generator, feed_data: FeedData) -> Feed {
    FeedBuilder::default()
        .title(format!("{} ({})", feed_data.title, gen.value))
        .id(feed_data.id)
        .link(LinkBuilder::default()
                  .href(feed_data.url)
                  .rel("alternate")
                  .build())
        .generator(gen.clone())
        .updated(Utc::now())
        .build()
}
