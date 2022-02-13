use std::error::Error;
use std::fs::File;
use std::path::Path;

use atom_syndication::{Entry, FeedBuilder, FixedDateTime, Generator, LinkBuilder};
use serde::{Serialize, Deserialize};

use super::common::Config;

#[derive(Serialize, Deserialize)]
pub struct FeedData {
    pub id: String,
    pub title: String,
    pub url: String,
    pub entries: Vec<Entry>,
}

pub fn write_feed<P: AsRef<Path>>(path: P, config: &Config, gen: &Generator, feed_data: FeedData)
  -> Result<(), Box<dyn Error>>
{
    let f = File::create(path)?;
    let feed = FeedBuilder::default()
        .title(format!("{} ({})", feed_data.title, gen.value))
        .id(format!("{}/{}", config.feed_id_base, feed_data.id))
        .link(LinkBuilder::default()
                  .href(feed_data.url)
                  .rel("alternate")
                  .build())
        .generator(gen.clone())
        .updated(FixedDateTime::parse_from_rfc3339("2020-07-25T22:10:00-07:00").unwrap())
        .entries(feed_data.entries)
        .build();
    feed.write_to(&f)?;

    Ok(())
}
