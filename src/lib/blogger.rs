use std::error::Error;

use reqwest::{Client, Response, Url};
use serde::{Serialize, Deserialize};
use tokio::time::{sleep, Duration};
use tokio_retry::RetryIf;
use tokio_retry::strategy::{ExponentialBackoff, jitter};

use super::common::{Config, Post, ReplayError};

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
#[serde(rename_all = "camelCase")]
struct ItemSummary {
    total_items: u32,
    self_link: String,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct ListPostsResponse {
    next_page_token: Option<String>,
    items: Vec<Post>,
}

fn check_ok_or_retryable(resp: &Response) -> Result<(), ReplayError> {
    if resp.status().is_success() {
        Ok(())
    } else if resp.status().is_server_error() {
        Err(ReplayError {
            msg: format!("failed request, status {}", resp.status()),
            retryable: true,
        })
    } else {
        Err(ReplayError {
            msg: format!("failed request, status {}", resp.status()),
            retryable: false,
        })
    }
}

async fn get_blog_once(config: &Config, client: &Client, blog_url: &str)
    -> Result<Blog, Box<dyn Error>>
{
    let resp = client.get(Url::parse("https://www.googleapis.com/blogger/v3/blogs/byurl")?)
        .query(&[("url", blog_url), ("key", &config.blogger_api_key)])
        .send()
        .await?;

    check_ok_or_retryable(&resp)?;
    Ok(resp.json().await?)
}

async fn get_page_once(
    config: &Config, client: &Client, api_url: &Url, page_token: Option<&String>)
    -> Result<ListPostsResponse, Box<dyn Error>>
{
    let req = client.get(api_url.clone())
        .query(&[
               ("key", &config.blogger_api_key),
               ("orderBy", &String::from("published")),
               ("fetchBodies", &String::from("false")),
        ]);

    let req = if let Some(token) = page_token {
        req.query(&[("pageToken", token)])
    } else {
        req
    };

    let resp = req.send().await?;

    check_ok_or_retryable(&resp)?;
    Ok(resp.json().await?)
}

pub async fn get_posts(config: &Config, client: &Client, blog_url: &str, delay: u8)
    -> Result<Vec<Post>, Box<dyn Error>>
{
    let mut ret: Vec<Post> = Vec::new();

    let blog = RetryIf::spawn(
        ExponentialBackoff::from_millis(500)
            .map(jitter)
            .take(config.max_retries),
        || get_blog_once(config, client, blog_url),
        |e: &Box<dyn Error>| match e.downcast_ref::<ReplayError>() {
            Some(re) => re.retryable,
            _ => false
        }
    ).await?;

    let posts_api_url = Url::parse(&format!(
            "https://www.googleapis.com/blogger/v3/blogs/{}/posts",
            blog.id))?;

    let mut next_page_token: Option<String> = None;
    loop {
        let mut post_resp = RetryIf::spawn(
            ExponentialBackoff::from_millis(500)
                .map(jitter)
                .take(config.max_retries),
            || get_page_once(config, client, &posts_api_url, next_page_token.as_ref()),
            |e: &Box<dyn Error>| match e.downcast_ref::<ReplayError>() {
                Some(re) => re.retryable,
                _ => false
            }
        ).await?;

        ret.append(&mut post_resp.items);

        next_page_token = post_resp.next_page_token.take();
        if next_page_token.is_none() {
            break;
        }

        if delay > 0 {
            sleep(Duration::from_secs(delay as u64)).await;
        }
    }

    Ok(ret)
}
