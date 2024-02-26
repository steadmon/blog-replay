use std::error::Error;

use atom_syndication::{ContentBuilder, Entry, EntryBuilder, LinkBuilder, Person};
use reqwest::Url;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};

use crate::common::*;

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
        let content = post.content.map(|v| {
            ContentBuilder::default()
                .value(v)
                .content_type(Some("html".to_string()))
                .build()
        });

        EntryBuilder::default()
            .title(post.title)
            .id(post.id)
            .published(parse_datetime(&post.published))
            .author(post.author.into())
            .content(content)
            .link(
                LinkBuilder::default()
                    .href(post.url)
                    .rel("alternate")
                    .build(),
            )
            .build()
    }
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct ListPostsResponse {
    next_page_token: Option<String>,
    items: Vec<Post>,
}

fn get_blog_once(
    config: &Config,
    client: &Client,
    api_url: &Url,
    blog_url: &str,
) -> anyhow::Result<Blog> {
    let resp = client
        .get(api_url.clone())
        .query(&[("url", blog_url), ("key", &config.blogger_api_key)])
        .send()?;

    Ok(resp.error_for_status()?.json()?)
}

fn get_page_once(
    config: &Config,
    client: &Client,
    api_url: &Url,
    page_token: Option<&String>,
) -> anyhow::Result<ListPostsResponse> {
    let req = client.get(api_url.clone()).query(&[
        ("key", &config.blogger_api_key),
        ("orderBy", &String::from("published")),
        ("fetchBodies", &String::from("true")),
    ]);

    let req = if let Some(token) = page_token {
        req.query(&[("pageToken", token)])
    } else {
        req
    };

    let resp = req.send()?;

    Ok(resp.error_for_status()?.json()?)
}

pub fn detect(config: &Config, client: &Client, blog_url: &str) -> bool {
    let blog_api_url = Url::parse("https://www.googleapis.com/blogger/v3/blogs/byurl").unwrap();
    let res = retry_request(config, || {
        get_blog_once(config, client, &blog_api_url, blog_url)
    });

    res.is_ok()
}

pub fn get_feed(
    config: &Config,
    client: &Client,
    blog_url: &str,
    delay: u64,
) -> Result<(FeedData, Vec<Entry>), Box<dyn Error>> {
    let mut posts: Vec<Post> = Vec::new();

    let blog_api_url = Url::parse("https://www.googleapis.com/blogger/v3/blogs/byurl")?;
    let blog = retry_request(config, || {
        get_blog_once(config, client, &blog_api_url, blog_url)
    })?;

    println!(
        r#"Scraping "{}" ({} posts, {} pages)"#,
        blog.name, blog.posts.total_items, blog.pages.total_items
    );
    if blog.posts.total_items > 0 {
        let posts_api_url = Url::parse(&format!(
            "https://www.googleapis.com/blogger/v3/blogs/{}/posts",
            blog.id
        ))?;

        let mut next_page_token: Option<String> = None;
        let pb = init_progress_bar(blog.posts.total_items as u64);
        loop {
            let mut post_resp = retry_request(config, || {
                get_page_once(config, client, &posts_api_url, next_page_token.as_ref())
            })?;
            pb.inc(post_resp.items.len().try_into().unwrap());
            posts.append(&mut post_resp.items);

            next_page_token = post_resp.next_page_token.take();
            if next_page_token.is_none() {
                break;
            }

            if delay > 0 {
                std::thread::sleep(std::time::Duration::from_secs(delay));
            }
        }
        pb.finish()
    }

    if blog.pages.total_items > 0 {
        let pages_api_url = Url::parse(&format!(
            "https://www.googleapis.com/blogger/v3/blogs/{}/pages",
            blog.id
        ))?;
        let mut page_resp = retry_request(config, || {
            get_page_once(config, client, &pages_api_url, None)
        })?;
        posts.append(&mut page_resp.items);
    }

    // TODO: check posts.len == blog.pages.total_items + blog.posts.total_items

    // Add our prefix to Blogger's post IDs
    let blog_key = sanitize_blog_key(&blog.name);
    let blog_id = format!("{}/{}", config.feed_url_base, blog_key);
    for post in &mut posts {
        post.id = format!("{}/{}", blog_id, post.id);
    }

    Ok((
        FeedData {
            id: blog_id,
            key: blog_key,
            title: blog.name,
            url: blog.url,
        },
        posts.into_iter().map(|p| p.into()).collect::<Vec<Entry>>(),
    ))
}
