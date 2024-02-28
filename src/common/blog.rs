use reqwest::blocking::Client;

use super::atom::FeedData;
use super::config::Config;

#[derive(Debug)]
pub enum BlogType {
    Blogger,
    Wordpress,
}

pub trait Blog {
    fn blog_type(&self) -> BlogType;

    fn feed_data(&self, config: &Config) -> FeedData;
}

pub fn get_blog(config: &Config, client: &Client, url: &str) -> anyhow::Result<Box<dyn Blog>> {
    crate::blogger::get_blog(config, client, url)
        .or_else(|_| crate::wordpress::get_blog(config, client, url))
        .or_else(|_| anyhow::bail!("Could not determine blog type"))
}
