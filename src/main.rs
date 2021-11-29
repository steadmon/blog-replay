use std::error::Error;
use std::fmt::Display;

use clap::clap_app;
use reqwest::{Client, Url};
use serde::{Deserialize, Serialize};

static PROG_NAME: &str = env!("CARGO_PKG_NAME");
static VERSION: &str = env!("CARGO_PKG_VERSION");
static USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));

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

#[derive(Serialize, Deserialize, Debug)]
struct Post {
    id: String,
    url: String,
    title: String,
    content: String,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct ListPostsResponse {
    next_page_token: String,
    items: Vec<Post>,
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

async fn get_posts(config: &Config, client: &Client, blog: &Blog) -> Result<(), Box<dyn Error>> {
    let resp = client.get(
        Url::parse(&format!("https://www.googleapis.com/blogger/v3/blogs/{}/posts", blog.id))?
    ).query(&[("key", &config.blogger_api_key), ("orderBy", &String::from("published"))])
        .send()
        .await?;

    if resp.status() != 200 {
        return Err(Box::new(ReplayError { msg: "failed request".to_string() }));
    }

    let post_resp: ListPostsResponse = serde_json::from_str(&resp.text().await?)?;
    println!();
    for item in &post_resp.items {
        println!("{}", item.title);
    }
    Ok(())
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
    let matches = clap_app!((PROG_NAME) =>
        (version: VERSION)
        (author: "Joshua Steadmon <josh@steadmon.net>")
        (about: "Replays blog archives into an RSS feed")
        (@subcommand scrape =>
            (about: "loads a blog's archive into the local DB for later replay")
            (@arg URL: +required "URL of the blog to scrape")
        )
    ).get_matches();

    let config: Config = confy::load(PROG_NAME).unwrap();

    if let Some(scrape_matches) = matches.subcommand_matches("scrape") {
        let client = reqwest::ClientBuilder::new()
            .user_agent(USER_AGENT)
            .build()
            .unwrap();

        let blog = get_blog(&config, &client, scrape_matches.value_of("URL").unwrap())
            .await.unwrap();
        get_posts(&config, &client, &blog).await.unwrap();
    }
}
