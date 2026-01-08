use std::collections::HashMap;
use std::collections::VecDeque;

use anyhow::Result;
use atom_syndication::{ContentBuilder, Entry, EntryBuilder, LinkBuilder, Person};
use reqwest::blocking::Client;
use reqwest::Url;
use serde::Deserialize;

use crate::common::*;

// Parsed from Wordpress API endpoint
#[derive(Clone, Deserialize)]
struct WordpressMeta {
    name: String,
    home: String,
}

pub fn get_blog<'a>(
    config: &'a Config,
    client: &'a Client,
    url: &str,
) -> anyhow::Result<Box<dyn Blog + 'a>> {
    // Technically we should use a HEAD request to discover[1] the API base (if it exists), but
    // this doesn't seem to be enabled on all sites.
    // [1]: https://developer.wordpress.org/rest-api/using-the-rest-api/discovery/#discovering-the-api
    let api_url = Url::parse(format!("{url}/wp-json/").as_str())?;
    let meta: WordpressMeta = retry_request(config, || {
        Ok(client
            .get(api_url.clone())
            .send()?
            .error_for_status()?
            .json()?)
    })?;

    let key = sanitize_blog_key(&meta.name);
    let feed_id = format!("{}/{}", config.feed_url_base, key);
    let users_url = api_url.join("wp/v2/users")?;
    let authors = retry_request(config, || get_users_once(client, &users_url))?;

    let posts = InternalIter::new(config, client, api_url.join("wp/v2/posts")?)?;
    let pages = InternalIter::new(config, client, api_url.join("wp/v2/pages")?)?;

    Ok(Box::new(WordpressBlog {
        meta,
        key,
        feed_id,
        authors,
        inner_iter: posts.chain(pages),
    }))
}

fn get_users_once(client: &Client, url: &Url) -> anyhow::Result<HashMap<usize, String>> {
    let resp = client.get(url.clone()).send()?;
    let mut users: Vec<User> = resp.error_for_status()?.json()?;
    Ok(users.drain(..).map(|u| (u.id, u.name)).collect())
}

#[derive(Clone)]
struct WordpressBlog<'a> {
    meta: WordpressMeta,
    key: String,
    feed_id: String,
    authors: HashMap<usize, String>,
    inner_iter: std::iter::Chain<InternalIter<'a>, InternalIter<'a>>,
}

impl WordpressBlog<'_> {
    fn post_to_entry(&self, post: &Post) -> Entry {
        let content = ContentBuilder::default()
            .value(post.content.rendered.clone())
            .content_type(Some("html".to_string()))
            .build();

        EntryBuilder::default()
            .title(post.title.rendered.clone())
            .id(format!("{}/{}", self.feed_id, post.id))
            .published(parse_assuming_utc(&post.date_gmt))
            .author(Person {
                name: self.authors[&post.author].clone(),
                email: None,
                uri: None,
            })
            .content(content)
            .link(
                LinkBuilder::default()
                    .href(post.link.clone())
                    .rel("alternate")
                    .build(),
            )
            .build()
    }
}

impl Blog for WordpressBlog<'_> {
    fn feed_data(&self) -> FeedData {
        FeedData {
            id: self.feed_id.clone(),
            key: self.key.clone(),
            title: self.meta.name.clone(),
            url: self.meta.home.clone(),
        }
    }
}

impl Iterator for WordpressBlog<'_> {
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
    done: bool,
    pending: VecDeque<Post>,
    api_page: usize,
    expected_posts: usize,
}

impl<'a> InternalIter<'a> {
    fn new(config: &'a Config, client: &'a Client, url: Url) -> Result<Self> {
        let mut this = Self {
            config,
            client,
            url,
            done: false,
            pending: VecDeque::new(),
            api_page: 1,
            expected_posts: 0,
        };
        this.fetch_entries()?;
        Ok(this)
    }

    fn fetch_entries(&mut self) -> Result<()> {
        let (tmp_posts, num_posts, num_api_pages) =
            retry_request(self.config, || self.query_once())?;
        self.expected_posts = num_posts;
        self.pending.extend(tmp_posts);
        if self.api_page == num_api_pages {
            self.done = true;
        } else {
            self.api_page += 1;
        }
        Ok(())
    }

    fn query_once(&self) -> anyhow::Result<(Vec<Post>, usize, usize)> {
        let req = self
            .client
            .get(self.url.clone())
            .query(&[("page", &format!("{}", self.api_page))]);
        let resp = req.send()?.error_for_status()?;
        let items = resp
            .headers()
            .get("X-WP-Total")
            .ok_or_else(|| anyhow::anyhow!("Missing expected X-WP-Total header"))?
            .to_str()?
            .parse::<usize>()?;
        let pages = resp
            .headers()
            .get("X-WP-TotalPages")
            .ok_or_else(|| anyhow::anyhow!("Missing expected X-WP-TotalPages header"))?
            .to_str()?
            .parse::<usize>()?;
        let posts = resp.json()?;

        Ok((posts, items, pages))
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
                if self.api_page > 1 {
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
        (self.expected_posts, Some(self.expected_posts))
    }
}

#[derive(Clone, Deserialize)]
struct Content {
    rendered: String,
}

#[derive(Deserialize)]
struct User {
    id: usize,
    name: String,
}

#[derive(Clone, Deserialize)]
struct Post {
    id: usize,
    date_gmt: String,
    link: String,
    title: Content,
    content: Content,
    author: usize,
}
