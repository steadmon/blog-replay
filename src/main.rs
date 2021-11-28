use std::error::Error;
use std::fmt::Display;

use serde::{Deserialize, Serialize};
use reqwest::{Client, Url};
use tokio;

static PROG_NAME: &str = env!("CARGO_PKG_NAME");
static USER_AGENT: &str = concat!(
    env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));

#[derive(Debug)]
struct ReplayError {
    msg: String
}

impl Display for ReplayError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "ReplayError: {}", self.msg)
    }
}

impl Error for ReplayError {
    fn source(&self) -> Option<&(dyn Error + 'static)> { None }
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct ItemSummary {
    total_items: u32,
    self_link: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct Blog {
    id: String,
    name: String,
    description: String,
    url: String,
    posts: ItemSummary,
    pages: ItemSummary,
}

async fn get_blog(config: &Config, client: &Client, blog_url: &str) -> Result<Blog, Box<dyn Error>>
{
    let resp = client.get(Url::parse("https://www.googleapis.com/blogger/v3/blogs/byurl")?)
        .query(&[("url", blog_url), ("key", &config.blogger_api_key)])
        .send()
        .await?;

    if resp.status() != 200 {
        return Err(Box::new(ReplayError { msg: "failed request".to_string() }));
    }

    Ok(serde_json::from_str(&resp.text().await?)?)
}

#[derive(Serialize, Deserialize)]
struct Config {
    blogger_api_key: String,
}

impl std::default::Default for Config {
    fn default() -> Self {
        Self { blogger_api_key: "".to_string() }
    }
}

#[tokio::main]
async fn main() {
    let config: Config = confy::load(PROG_NAME).unwrap();

    let client = reqwest::ClientBuilder::new()
        .user_agent(USER_AGENT)
        .build()
        .unwrap();

    // init(&config).unwrap();
    let blog = get_blog(&config, &client, "https://monstersandmanuals.blogspot.com")
        .await.unwrap();
    println!(blog.id);
}
