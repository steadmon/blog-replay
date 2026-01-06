use std::collections::VecDeque;

use anyhow::{anyhow, Result};
use atom_syndication::{ContentBuilder, Entry, EntryBuilder, LinkBuilder, Person};
use reqwest::blocking::Client;
use reqwest::Url;
use serde::Deserialize;

use crate::common::*;

#[derive(Clone, Deserialize)]
struct SubstackMeta {
    custom_domain: Option<String>,
    id: usize,
    name: String,
    subdomain: String,
}

#[derive(Clone)]
struct SubstackBlog<'a> {
    config: &'a Config,
    client: &'a Client,
    url: String,
    archive_url: Url,
    posts_url: Url,
    meta: SubstackMeta,
    feed_id: String,
    pending: VecDeque<Post>,
    offset: usize,
    done: bool,
}

pub fn get_blog<'a>(
    config: &'a Config,
    client: &'a Client,
    url: &str,
) -> Result<Box<dyn Blog + 'a>> {
    let base_url = Url::parse(url)?;
    let domain = base_url
        .domain()
        .ok_or_else(|| anyhow!("Can't extract domain from url {url}"))?;
    let subdomain = if domain.ends_with("substack.com") {
        domain.split_once('.').unwrap_or((domain, "")).0
    } else {
        domain
            .rsplit('.')
            .nth(1)
            .ok_or_else(|| anyhow!("Could not find subdomain of {domain}"))?
    };
    let meta_url = Url::parse_with_params(
        "https://substack.com/api/v1/publication/search",
        &[("query", subdomain)],
    )?;
    let archive_url = base_url.join("api/v1/archive")?;
    let posts_url = base_url.join("api/v1/posts/")?;

    let search_resp: PubSearchResponse = retry_request(config, || {
        Ok(client
            .get(meta_url.clone())
            .send()?
            .error_for_status()?
            .json()?)
    })?;
    let meta = {
        let mut found: Option<SubstackMeta> = None;
        for result in search_resp.results {
            if result.subdomain == subdomain
                || result.custom_domain.as_ref().is_some_and(|d| d == domain)
            {
                found = Some(result);
                break;
            }
        }
        found.ok_or_else(|| anyhow!("Couldn't find blog info for {domain}"))
    }?;
    let feed_id = format!("{}/{}", config.feed_url_base, meta.subdomain);

    let mut blog = SubstackBlog {
        config,
        client,
        url: url.to_owned(),
        archive_url,
        posts_url,
        meta,
        feed_id,
        pending: VecDeque::new(),
        offset: 0,
        done: false,
    };
    blog.fetch_entries()?;
    Ok(Box::new(blog))
}

// TODO: preserve progress bar count
impl Iterator for SubstackBlog<'_> {
    type Item = Result<Entry>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if !self.pending.is_empty() {
                let post = self.pending.pop_front().unwrap();
                if post.publication_id != self.meta.id {
                    return Some(Err(anyhow!(
                        "Post publication {} did not match expected id {}",
                        post.publication_id,
                        self.meta.id
                    )));
                }
                let post_meta = match self.fetch_post(&post) {
                    Ok(pm) => pm,
                    Err(e) => return Some(Err(e)),
                };
                return Some(Ok(self.post_to_entry(&post, post_meta)));
            } else if self.done {
                return None;
            } else {
                std::thread::sleep(std::time::Duration::from_secs(1));
                if let Err(e) = self.fetch_entries() {
                    self.done = true;
                    return Some(Err(e));
                }
            }
        }
    }
}

impl Blog for SubstackBlog<'_> {
    fn feed_data(&self) -> FeedData {
        FeedData {
            id: self.feed_id.clone(),
            key: self.meta.subdomain.clone(),
            title: self.meta.name.clone(),
            url: self.url.to_owned(),
        }
    }
}

impl<'a> SubstackBlog<'a> {
    fn fetch_entries(&mut self) -> Result<()> {
        let posts: Vec<Post> = retry_request(self.config, || {
            Ok(self
                .client
                .get(self.archive_url.clone())
                .query(&[("offset", self.offset)])
                .send()?
                .error_for_status()?
                .json()?)
        })?;
        if posts.is_empty() {
            self.done = true;
        }
        self.offset += posts.len();
        // TODO: configure visibility filtering
        self.pending.extend(
            posts
                .into_iter()
                .filter(|p| p.visibility == Visibility::Public),
        );
        Ok(())
    }

    fn fetch_post(&mut self, post: &Post) -> Result<PostMeta> {
        retry_request(self.config, || {
            Ok(self
                .client
                .get(self.posts_url.join(&post.slug)?)
                .send()?
                .error_for_status()?
                .json()?)
        })
    }

    fn post_to_entry(&self, post: &Post, post_meta: PostMeta) -> Entry {
        let content = ContentBuilder::default()
            .value(post_meta.body_html)
            .content_type(Some("html".to_string()))
            .build();

        let authors: Vec<Person> = post_meta
            .published_bylines
            .into_iter()
            .map(|a| Person {
                name: a.name,
                email: None,
                uri: Some(format!("https://substack.com/@{}", a.handle)),
            })
            .collect();

        EntryBuilder::default()
            .title(post.title.clone())
            .id(format!("{}/{}", &self.feed_id, post.id))
            .published(parse_datetime(&post.post_date))
            .content(content)
            .authors(authors)
            .link(
                LinkBuilder::default()
                    .href(post.canonical_url.clone())
                    .rel("alternate")
                    .build(),
            )
            .build()
    }
}

#[derive(Clone, Deserialize, PartialEq)]
enum Visibility {
    #[serde(rename = "everyone")]
    Public,
    #[serde(rename = "only_paid")]
    Private,
}

#[derive(Clone, Deserialize)]
struct Post {
    id: usize,
    title: String,
    slug: String,
    post_date: String,
    canonical_url: String,
    #[serde(rename = "audience")]
    visibility: Visibility,
    publication_id: usize,
}

#[derive(Deserialize)]
struct PubSearchResponse {
    results: Vec<SubstackMeta>,
}

#[derive(Deserialize)]
struct Byline {
    name: String,
    handle: String,
}

#[derive(Deserialize)]
struct PostMeta {
    body_html: String,
    #[serde(rename = "publishedBylines")]
    published_bylines: Vec<Byline>,
}
