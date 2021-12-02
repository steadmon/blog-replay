use std::error::Error;

use reqwest::{Client, Url};
use serde::{Serialize, Deserialize};
use tokio::time::{sleep, Duration};

use super::common::{Config, ReplayError};

#[derive(Serialize, Deserialize, Debug)]
pub struct Blog {
    id: String,
    name: String,
    description: String,
    url: String,
    posts: ItemSummary,
    pages: ItemSummary,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct ItemSummary {
    total_items: u32,
    self_link: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct Post {
    id: String,
    url: String,
    title: String,
    content: Option<String>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct ListPostsResponse {
    next_page_token: Option<String>,
    items: Vec<Post>,
}

pub async fn get_blog(config: &Config, client: &Client, blog_url: &str) -> Result<Blog, Box<dyn Error>>
{
    let resp = client.get(Url::parse("https://www.googleapis.com/blogger/v3/blogs/byurl")?)
        .query(&[("url", blog_url), ("key", &config.blogger_api_key)])
        .send()
        .await?;

    if resp.status() != 200 {
        return Err(Box::new(ReplayError { msg: "failed request".to_string() }));
    }

    Ok(resp.json().await?)
}

pub async fn get_posts(config: &Config, client: &Client, blog: &Blog, delay: u8)
    -> Result<(), Box<dyn Error>>
{
    let posts_api_url = Url::parse(&format!(
            "https://www.googleapis.com/blogger/v3/blogs/{}/posts",
            blog.id))?;

    let mut page_count = 0;
    let mut next_page_token: Option<String> = None;
    loop {
        let req = client.get(posts_api_url.clone())
            .query(&[
                   ("key", &config.blogger_api_key),
                   ("orderBy", &String::from("published")),
                   ("fetchBodies", &String::from("false")),
            ]);

        let req = if let Some(ref token) = next_page_token {
            req.query(&[("pageToken", token)])
        } else {
            req
        };

        let resp = req.send().await?;

        if resp.status() != 200 {
            return Err(Box::new(ReplayError {
                msg: format!("failed request, status {}", resp.status())
            }));
        }

        let mut post_resp: ListPostsResponse = resp.json().await?;
        page_count += 1;
        println!("Page {}", page_count);
        println!("=======");
        for item in &post_resp.items {
            println!("{}", item.title);
        }
        println!();

        next_page_token = post_resp.next_page_token.take();
        if post_resp.items.len() < 1 || next_page_token.is_none() {
            println!("Breaking loop");
            println!("Last response had {} pages, token {:?}",
                     post_resp.items.len(), next_page_token);
            break;
        }

        if delay > 0 {
            sleep(Duration::from_secs(delay as u64)).await;
        }
    }

    Ok(())
}

