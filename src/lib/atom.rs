use std::error::Error;
use std::fs::File;
use std::path::Path;

use atom_syndication::{Entry, FeedBuilder, Generator, LinkBuilder};
use chrono::Utc;
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize)]
pub struct FeedData {
    pub id: String,
    pub title: String,
    pub url: String,
    pub entries: Vec<Entry>,
}

pub fn write_feed<P: AsRef<Path>>(path: P, gen: &Generator, feed_data: FeedData)
  -> Result<(), Box<dyn Error>>
{
    let f = File::create(path)?;
    let feed = FeedBuilder::default()
        .title(format!("{} ({})", feed_data.title, gen.value))
        .id(feed_data.id)
        .link(LinkBuilder::default()
                  .href(feed_data.url)
                  .rel("alternate")
                  .build())
        .generator(gen.clone())
        .updated(Utc::now())
        .entries(feed_data.entries)
        .build();
    feed.write_to(&f)?;

    Ok(())
}
