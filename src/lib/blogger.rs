use std::error::Error;
use std::future::Future;

use atom_syndication::{ContentBuilder, Entry, EntryBuilder, LinkBuilder, Person};
use reqwest::{Client, Url};
use serde::{Serialize, Deserialize};
use tokio::time::{sleep, Duration};
use tokio_retry::RetryIf;
use tokio_retry::strategy::{ExponentialBackoff, jitter};

use super::common::{
    Config, FeedData, parse_datetime, sanitize_blog_key
};

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
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct Author {
    display_name: String,
    url: String,
}

impl From<Author> for Person {
    fn from(author: Author) -> Self {
        Person {
            name: author.display_name,
            email: None,
            uri: Some(author.url),
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
struct Post {
    id: String,
    url: String,
    title: String,
    content: Option<String>,
    author: Author,
    published: String,
}

impl From<Post> for Entry {
    fn from(post: Post) -> Self {
        let content = if let Some(value) = post.content {
            Some(ContentBuilder::default()
                 .value(value)
                 .content_type(Some("html".to_string()))
                 .build())
        } else {
            None
        };

        EntryBuilder::default()
            .title(post.title)
            .id(post.id)
            .published(parse_datetime(&post.published))
            .author(post.author.into())
            .content(content)
            .link(LinkBuilder::default()
                  .href(post.url)
                  .rel("alternate")
                  .build())
            .build()
    }
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct ListPostsResponse {
    next_page_token: Option<String>,
    items: Vec<Post>,
}

async fn get_blog_once(config: &Config, client: &Client, api_url: &Url, blog_url: &str)
    -> Result<Blog, reqwest::Error>
{
    let resp = client.get(api_url.clone())
        .query(&[("url", blog_url), ("key", &config.blogger_api_key)])
        .send()
        .await?;

    Ok(resp.error_for_status()?.json().await?)
}

async fn get_page_once(
    config: &Config, client: &Client, api_url: &Url, page_token: Option<&String>)
    -> Result<ListPostsResponse, reqwest::Error>
{
    let req = client.get(api_url.clone())
        .query(&[
               ("key", &config.blogger_api_key),
               ("orderBy", &String::from("published")),
               ("fetchBodies", &String::from("true")),
        ]);

    let req = if let Some(token) = page_token {
        req.query(&[("pageToken", token)])
    } else {
        req
    };

    let resp = req.send().await?;

    Ok(resp.error_for_status()?.json().await?)
}

async fn retry_request<F, R, T>(config: &Config, action: F)
    -> Result<R, reqwest::Error>
    where F: FnMut() -> T,
          T: Future<Output = Result<R, reqwest::Error>>
{
    RetryIf::spawn(
        ExponentialBackoff::from_millis(500).map(jitter).take(config.max_retries),
        action,
        |e: &reqwest::Error| e.status().map_or(false, |s| s.is_server_error())
    ).await
}

pub async fn get_feed(config: &Config, client: &Client, blog_url: &str, delay: u64)
    -> Result<(FeedData, Vec<Entry>), Box<dyn Error>>
{
    let mut posts: Vec<Post> = Vec::new();

    let blog_api_url = Url::parse("https://www.googleapis.com/blogger/v3/blogs/byurl")?;
    let blog = retry_request(
        config, || get_blog_once(config, client, &blog_api_url, blog_url)).await?;

    println!(r#"Scraping "{}" ({} posts, {} pages)"#,
        blog.name, blog.posts.total_items, blog.pages.total_items);
    if blog.posts.total_items > 0 {
        let posts_api_url = Url::parse(&format!(
                "https://www.googleapis.com/blogger/v3/blogs/{}/posts",
                blog.id))?;

        let mut next_page_token: Option<String> = None;
        let pb = indicatif::ProgressBar::new(blog.posts.total_items as u64);
        pb.set_style(indicatif::ProgressStyle::default_bar().template(
                "{spinner:.blue} [{bar:.blue}] ({pos}/{len}) \
                [elapsed: {elapsed_precise}, eta: {eta_precise}]")
            .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏ ")
            .progress_chars("█▉▊▋▌▍▎▏  ")
        );
        pb.enable_steady_tick(100);
        loop {
            let mut post_resp = retry_request(
                config,
                || get_page_once(config, client, &posts_api_url, next_page_token.as_ref())
            ).await?;
            pb.inc(post_resp.items.len().try_into().unwrap());
            posts.append(&mut post_resp.items);

            next_page_token = post_resp.next_page_token.take();
            if next_page_token.is_none() {
                break;
            }

            if delay > 0 {
                sleep(Duration::from_secs(delay)).await;
            }
        }
        pb.finish()
    }

    if blog.pages.total_items > 0 {
        let pages_api_url = Url::parse(&format!(
                "https://www.googleapis.com/blogger/v3/blogs/{}/pages",
                blog.id))?;
        let mut page_resp = retry_request(
            config,
            || get_page_once(config, client, &pages_api_url, None)
        ).await?;
        posts.append(&mut page_resp.items);
    }

    // TODO: check posts.len == blog.pages.total_items + blog.posts.total_items

    // Add our prefix to Blogger's post IDs
    let blog_key = sanitize_blog_key(&blog.name);
    let blog_id = format!("{}/{}", config.feed_url_base, blog_key);
    for post in &mut posts {
        post.id = format!("{}/{}", blog_id, post.id);
    }

    Ok((FeedData {
            id: blog_id,
            key: blog_key,
            title: blog.name,
            url: blog.url,
        },
        posts.into_iter().map(|p| p.into()).collect::<Vec<Entry>>()
    ))
}
