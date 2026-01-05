use anyhow::Result;
use atom_syndication::Entry;
use reqwest::blocking::Client;

use super::atom::FeedData;
use super::config::Config;

pub trait Blog {
    fn feed_data(&self) -> FeedData;
    fn entries(&self) -> Result<Vec<Entry>>;
}

pub fn get_blog<'a>(config: &'a Config, client: &'a Client, url: &str) -> Result<Box<dyn Blog + 'a>> {
    crate::blogger::get_blog(config, client, url)
        .or_else(|_| crate::wordpress::get_blog(config, client, url))
        .or_else(|_| anyhow::bail!("Could not determine blog type"))
}
