use std::collections::VecDeque;

use anyhow::Result;
use atom_syndication::{ContentBuilder, Entry, EntryBuilder, LinkBuilder, Person};
use reqwest::blocking::Client;
use reqwest::Url;
use serde::{Deserialize, Serialize};

use crate::common::*;

// Parsed from Blogger API endpoint
#[derive(Clone, Serialize, Deserialize, Debug)]
struct BloggerJson {
    id: String,
    name: String,
    description: String,
    url: String,
    posts: ItemSummary,
    pages: ItemSummary,
}

// Can't combine this with the above BloggerJson struct because we can't deserialize reqwest::Url
#[derive(Clone)]
struct BloggerBlog<'a> {
    api_json: BloggerJson,
    posts_api_url: Url,
    pages_api_url: Url,
    key: String,
    feed_id: String,
    seen_posts: usize,
    posts_done: bool,
    pages_done: bool,
    next_page_token: Option<String>,
    pending_entries: VecDeque<Entry>,
    config: &'a Config,
    client: &'a Client,
    pb: Option<indicatif::ProgressBar>,
}

pub fn get_blog<'a>(
    config: &'a Config,
    client: &'a Client,
    url: &str,
) -> Result<Box<dyn Blog + 'a>> {
    let api_url = Url::parse("https://www.googleapis.com/blogger/v3/blogs/byurl")?;
    let api_json: BloggerJson = retry_request(config, || {
        Ok(client
            .get(api_url.clone())
            .query(&[("url", url), ("key", &config.blogger_api_key)])
            .send()?
            .error_for_status()?
            .json()?)
    })?;

    let posts_api_url = Url::parse(&format!(
        "https://www.googleapis.com/blogger/v3/blogs/{}/posts",
        api_json.id
    ))?;

    let pages_api_url = Url::parse(&format!(
        "https://www.googleapis.com/blogger/v3/blogs/{}/pages",
        api_json.id
    ))?;

    let key = sanitize_blog_key(&api_json.name);
    let feed_id = format!("{}/{}", config.feed_url_base, key);

    Ok(Box::new(BloggerBlog {
        api_json,
        posts_api_url,
        pages_api_url,
        key,
        feed_id,
        seen_posts: 0,
        posts_done: false,
        pages_done: false,
        next_page_token: None,
        pending_entries: VecDeque::new(),
        config,
        client,
        pb: None,
    }))
}

impl Blog for BloggerBlog<'_> {
    fn feed_data(&self) -> FeedData {
        FeedData {
            id: self.feed_id.clone(),
            key: self.key.clone(),
            title: self.api_json.name.clone(),
            url: self.api_json.url.clone(),
        }
    }
}

impl BloggerBlog<'_> {
    fn query_once(&self, api_url: &Url, page_token: Option<&String>) -> Result<ListPostsResponse> {
        let req = self.client.get(api_url.clone()).query(&[
            ("key", &self.config.blogger_api_key),
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
}

// We don't want this to be a BloggerBlog method, because we'll be modifying the pending_posts
// member while mapping this function over the API results (so we'd have clashing mutable vs.
// immutable borrows). If we instead just take a ref to only the feed_id, there's no conflict.
fn post_to_entry(post: &Post, feed_id: &String) -> Entry {
    let content = post.content.as_ref().map(|v| {
        ContentBuilder::default()
            .value(v.clone())
            .content_type(Some("html".to_string()))
            .build()
    });

    EntryBuilder::default()
        .title(post.title.clone())
        .id(format!("{}/{}", feed_id, post.id))
        .published(parse_datetime(&post.published))
        .author(Person {
            name: post.author.display_name.clone(),
            email: None,
            uri: Some(post.author.url.clone()),
        })
        .content(content)
        .link(
            LinkBuilder::default()
                .href(post.url.clone())
                .rel("alternate")
                .build(),
        )
        .build()
}

impl Iterator for BloggerBlog<'_> {
    type Item = Result<Entry>;

    fn next(&mut self) -> Option<Result<Entry>> {
        if self.posts_done && self.pages_done {
            if let Some(pb) = &self.pb {
                pb.finish();
            }
            return None;
        }

        if self.pending_entries.is_empty() {
            if self.pb.is_none() {
                println!(
                    r#"Scraping "{}" ({} posts, {} pages)"#,
                    &self.api_json.name,
                    self.api_json.posts.total_items,
                    self.api_json.pages.total_items
                );
                self.pb = Some(init_progress_bar(
                    (self.api_json.posts.total_items + self.api_json.pages.total_items) as u64,
                ));
            } else {
                std::thread::sleep(std::time::Duration::from_secs(1));
            }

            if let Err(e) = self.get_new_entries() {
                self.posts_done = true;
                self.pages_done = true;
                return Some(Err(e));
            }
        }

        if let Some(pb) = &self.pb {
            pb.inc(1);
        }

        self.pending_entries.pop_front().map(Ok)
    }
}

impl BloggerBlog<'_> {
    fn get_new_entries(&mut self) -> Result<()> {
        if !self.posts_done && self.api_json.posts.total_items > 0 {
            let mut post_resp = retry_request(self.config, || {
                self.query_once(&self.posts_api_url, self.next_page_token.as_ref())
            })?;

            self.seen_posts += post_resp.items.len();
            self.pending_entries.extend(
                post_resp
                    .items
                    .iter()
                    .map(|p| post_to_entry(p, &self.feed_id)),
            );

            self.next_page_token = post_resp.next_page_token.take();
            if self.next_page_token.is_none() {
                self.posts_done = true;
                if self.seen_posts < self.api_json.posts.total_items {
                    anyhow::bail!(
                        "Expected {} posts, saw {}",
                        self.api_json.posts.total_items,
                        self.seen_posts,
                    );
                }
            }
        } else if !self.pages_done && self.api_json.pages.total_items > 0 {
            let page_resp =
                retry_request(self.config, || self.query_once(&self.pages_api_url, None))?;

            self.pending_entries.extend(
                page_resp
                    .items
                    .iter()
                    .map(|p| post_to_entry(p, &self.feed_id)),
            );

            self.pages_done = true;

            if page_resp.items.len() < self.api_json.pages.total_items {
                anyhow::bail!(
                    "Expected {} pages, saw {}",
                    self.api_json.pages.total_items,
                    page_resp.items.len(),
                );
            }
        }

        Ok(())
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct ItemSummary {
    total_items: usize,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct Author {
    display_name: String,
    url: String,
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

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct ListPostsResponse {
    next_page_token: Option<String>,
    items: Vec<Post>,
}
