use std::collections::VecDeque;

use anyhow::Result;
use atom_syndication::{ContentBuilder, Entry, EntryBuilder, LinkBuilder, Person};
use reqwest::blocking::Client;
use reqwest::Url;
use serde::Deserialize;

use crate::common::*;

// Parsed from Blogger API endpoint
#[derive(Clone, Deserialize)]
struct BloggerMeta {
    id: String,
    name: String,
    url: String,
    posts: ItemSummary,
    pages: ItemSummary,
}

// Can't combine this with the above BloggerMeta struct because we can't deserialize reqwest::Url
#[derive(Clone)]
struct BloggerBlog<'a> {
    meta: BloggerMeta,
    key: String,
    feed_id: String,
    inner_iter: std::iter::Chain<InternalIter<'a>, InternalIter<'a>>,
}

pub fn get_blog<'a>(
    config: &'a Config,
    client: &'a Client,
    url: &str,
) -> Result<Box<dyn Blog + 'a>> {
    let api_url = Url::parse("https://www.googleapis.com/blogger/v3/blogs/byurl")?;
    let meta: BloggerMeta = retry_request(config, || {
        Ok(client
            .get(api_url.clone())
            .query(&[("url", url), ("key", &config.blogger_api_key)])
            .send()?
            .error_for_status()?
            .json()?)
    })?;

    let key = sanitize_blog_key(&meta.name);
    let feed_id = format!("{}/{}", config.feed_url_base, key);

    let posts = InternalIter::new(
        config,
        client,
        Url::parse(&format!(
            "https://www.googleapis.com/blogger/v3/blogs/{}/posts",
            meta.id
        ))?,
        meta.posts.total_items,
    )?;

    let pages = InternalIter::new(
        config,
        client,
        Url::parse(&format!(
            "https://www.googleapis.com/blogger/v3/blogs/{}/pages",
            meta.id
        ))?,
        meta.pages.total_items,
    )?;

    Ok(Box::new(BloggerBlog {
        meta,
        key,
        feed_id,
        inner_iter: posts.chain(pages),
    }))
}

impl BloggerBlog<'_> {
    fn post_to_entry(&self, post: &Post) -> Entry {
        let content = post.content.as_ref().map(|v| {
            ContentBuilder::default()
                .value(v.clone())
                .content_type(Some("html".to_string()))
                .build()
        });

        EntryBuilder::default()
            .title(post.title.clone())
            .id(format!("{}/{}", &self.feed_id, post.id))
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
}

impl Blog for BloggerBlog<'_> {
    fn feed_data(&self) -> FeedData {
        FeedData {
            id: self.feed_id.clone(),
            key: self.key.clone(),
            title: self.meta.name.clone(),
            url: self.meta.url.clone(),
        }
    }
}

impl Iterator for BloggerBlog<'_> {
    type Item = Result<Entry>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner_iter
            .next()
            .map(|r| r.map(|p| self.post_to_entry(&p)))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner_iter.size_hint()
    }
}

#[derive(Clone)]
struct InternalIter<'a> {
    config: &'a Config,
    client: &'a Client,
    url: Url,
    num_expected: usize,
    num_seen: usize,
    done: bool,
    pending: VecDeque<Post>,
    next_page_token: Option<String>,
}

impl<'a> InternalIter<'a> {
    fn new(config: &'a Config, client: &'a Client, url: Url, num_expected: usize) -> Result<Self> {
        let mut this = Self {
            config,
            client,
            url,
            num_expected,
            num_seen: 0,
            done: num_expected == 0,
            pending: VecDeque::new(),
            next_page_token: None,
        };
        if !this.done {
            this.fetch_entries()?;
        }
        Ok(this)
    }

    fn fetch_entries(&mut self) -> Result<()> {
        let mut post_resp = retry_request(self.config, || self.query_once())?;

        self.num_seen += post_resp.items.len();
        self.pending.extend(post_resp.items);
        self.next_page_token = post_resp.next_page_token.take();
        if self.next_page_token.is_none() {
            self.done = true;
            if self.num_seen < self.num_expected {
                anyhow::bail!(
                    "Expected {} posts, saw {}",
                    self.num_expected,
                    self.num_seen
                );
            }
        }
        Ok(())
    }

    fn query_once(&mut self) -> Result<ListPostsResponse> {
        let req = self.client.get(self.url.clone()).query(&[
            ("key", &self.config.blogger_api_key),
            ("orderBy", &String::from("published")),
            ("fetchBodies", &String::from("true")),
        ]);

        let req = if let Some(token) = &self.next_page_token {
            req.query(&[("pageToken", token)])
        } else {
            req
        };

        let resp = req.send()?;

        Ok(resp.error_for_status()?.json()?)
    }
}

impl Iterator for InternalIter<'_> {
    type Item = Result<Post>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if !self.pending.is_empty() {
                return self.pending.pop_front().map(Ok);
            } else if self.done {
                return None;
            } else {
                if self.next_page_token.is_none() {
                    std::thread::sleep(std::time::Duration::from_secs(1));
                }
                if let Err(e) = self.fetch_entries() {
                    self.done = true;
                    return Some(Err(e));
                }
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.num_expected, Some(self.num_expected))
    }
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ItemSummary {
    total_items: usize,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Author {
    display_name: String,
    url: String,
}

#[derive(Clone, Deserialize)]
struct Post {
    id: String,
    url: String,
    title: String,
    content: Option<String>,
    author: Author,
    published: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListPostsResponse {
    next_page_token: Option<String>,
    items: Vec<Post>,
}
