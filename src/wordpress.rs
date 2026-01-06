use std::collections::HashMap;

use anyhow::Result;
use atom_syndication::{ContentBuilder, Entry, EntryBuilder, LinkBuilder, Person};
use reqwest::Url;
use reqwest::blocking::Client;
use serde::Deserialize;

use crate::common::*;

// Parsed from Wordpress API endpoint
#[derive(Clone, Deserialize, Debug)]
struct WordpressJson {
    name: String,
    home: String,
}

pub fn get_blog<'a>(config: &'a Config, client: &'a Client, url: &str)
    -> anyhow::Result<Box<dyn Blog + 'a>>
{
    // Technically we should use a HEAD request to discover[1] the API base (if it exists), but
    // this doesn't seem to be enabled on all sites.
    // [1]: https://developer.wordpress.org/rest-api/using-the-rest-api/discovery/#discovering-the-api
    let api_url = Url::parse(format!("{url}/wp-json/").as_str())?;
    let api_json: WordpressJson = retry_request(config, || {
        Ok(client
                .get(api_url.clone())
                .send()?
                .error_for_status()?
                .json()?)
    })?;

    let key = sanitize_blog_key(&api_json.name);
    let feed_id = format!("{}/{}", config.feed_url_base, key);
    let users_url = api_url.join("wp/v2/users")?;
    let authors = retry_request(config, || get_users_once(client, &users_url))?;

    Ok(Box::new(WordpressBlog {
        api_json,
        posts_api_url: api_url.join("wp/v2/posts")?,
        pages_api_url: api_url.join("wp/v2/pages")?,
        key,
        feed_id,
        config,
        client,
        authors,
        api_page: 1,
        posts_done: false,
        pages_done: false,
        pending_entries: Vec::new(),
        pb: None,
    }))
}

fn get_users_once(client: &Client, url: &Url) -> anyhow::Result<HashMap<usize, String>> {
    let resp = client.get(url.clone()).send()?;
    let mut users: Vec<User> = resp.error_for_status()?.json()?;
    Ok(users.drain(..).map(|u| (u.id, u.name)).collect())
}

#[derive(Clone)]
struct WordpressBlog<'a> {
    api_json: WordpressJson,
    posts_api_url: Url,
    pages_api_url: Url,
    key: String,
    feed_id: String,
    config: &'a Config,
    client: &'a Client,
    authors: HashMap<usize, String>,
    api_page: usize,
    posts_done: bool,
    pages_done: bool,
    pending_entries: Vec<Entry>,
    pb: Option<indicatif::ProgressBar>,
}

impl Blog for WordpressBlog<'_> {
    fn feed_data(&self) -> FeedData {
        FeedData {
            id: self.feed_id.clone(),
            key: self.key.clone(),
            title: self.api_json.name.clone(),
            url: self.api_json.home.clone(),
        }
    }
}

impl Iterator for WordpressBlog<'_> {
    type Item = Result<Entry>;

    fn next(&mut self) -> Option<Result<Entry>> {
        if self.posts_done && self.pages_done {
            if let Some(pb) = &self.pb {
                pb.finish();
            }
            return None;
        }

        if self.pending_entries.is_empty() {
            if self.api_page > 1 {
                std::thread::sleep(std::time::Duration::from_secs(1));
            }

            if let Err(e) = self.get_new_entries() {
                self.posts_done = true;
                self.pages_done = true;
                return Some(Err(e));
            }
        }

        if let Some(pb) = &self.pb {
            // TODO: handle split between posts & pages bars
            pb.inc(1);
        }

        self.pending_entries.pop().map(Ok)
    }
}

impl WordpressBlog<'_> {
    fn get_new_entries(&mut self) -> Result<()> {
        if !self.posts_done {
            let (tmp_posts, num_posts, num_api_pages) = retry_request(self.config, || {
                self.get_page_once(&self.posts_api_url, self.api_page)
            })?;
            if self.api_page == 1 {
                println!(r#"Scraping "{}" ({} posts)"#, &self.api_json.name, num_posts);
                self.pb = Some(init_progress_bar(num_posts.try_into().unwrap()));
            }
            self.pending_entries.extend(tmp_posts.iter().map(|p| post_to_entry(p, &self.feed_id, &self.authors)));
            if self.api_page == num_api_pages {
                self.posts_done = true;
                if let Some(pb) = &self.pb {
                    pb.finish();
                }
                self.api_page = 1;
            } else {
                self.api_page += 1;
            }
        } else if !self.pages_done {
            let (tmp_posts, num_posts, num_api_pages) = retry_request(self.config, || {
                self.get_page_once(&self.pages_api_url, self.api_page)
            })?;
            if self.api_page == 1 {
                println!(r#"Scraping "{}" ({} pages)"#, &self.api_json.name, num_posts);
                self.pb = Some(init_progress_bar(num_posts.try_into().unwrap()));
            }
            self.pending_entries.extend(tmp_posts.iter().map(|p| post_to_entry(p, &self.feed_id, &self.authors)));
            if self.api_page == num_api_pages {
                self.pages_done = true;
                if let Some(pb) = &self.pb {
                    pb.finish();
                }
                self.api_page = 1;
            } else {
                self.api_page += 1;
            }
        }

        Ok(())
    }

    fn get_page_once(&self, api_url: &Url, page: usize)
        -> anyhow::Result<(Vec<Post>, usize, usize)>
    {
        let req = self.client.get(api_url.clone()).query(&[("page", &format!("{page}"))]);
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

fn post_to_entry(post: &Post, feed_id: &String, authors: &HashMap<usize, String>) -> Entry {
    let content = ContentBuilder::default()
        .value(post.content.rendered.clone())
        .content_type(Some("html".to_string()))
        .build();

    EntryBuilder::default()
        .title(post.title.rendered.clone())
        .id(format!("{}/{}", feed_id, post.id))
        .published(parse_assuming_utc(&post.date_gmt))
        .author(Person {
            name: authors[&post.author].clone(),
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

#[derive(Deserialize, Debug)]
struct Content {
    rendered: String,
}

#[derive(Deserialize, Debug)]
struct User {
    id: usize,
    name: String,
}

#[derive(Deserialize, Debug)]
struct Post {
    id: usize,
    date_gmt: String,
    link: String,
    title: Content,
    content: Content,
    author: usize,
}
