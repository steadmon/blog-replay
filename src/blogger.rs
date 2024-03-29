use anyhow::Result;
use atom_syndication::{ContentBuilder, Entry, EntryBuilder, LinkBuilder, Person};
use reqwest::blocking::Client;
use reqwest::Url;
use serde::{Deserialize, Serialize};

use crate::common::*;

// Parsed from Blogger API endpoint
#[derive(Serialize, Deserialize, Debug)]
struct BloggerJson {
    id: String,
    name: String,
    description: String,
    url: String,
    posts: ItemSummary,
    pages: ItemSummary,
}

// Can't combine this with the above BloggerJson struct because we can't deserialize reqwest::Url
struct BloggerBlog<'a> {
    api_json: BloggerJson,
    posts_api_url: Url,
    pages_api_url: Url,
    key: String,
    feed_id: String,
    config: &'a Config,
    client: &'a Client,
}

pub fn get_blog<'a>(config: &'a Config, client: &'a Client, url: &str) -> Result<Box<dyn Blog + 'a>> {
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
        config,
        client,
    }))
}

impl Blog for BloggerBlog<'_> {
    fn blog_type(&self) -> BlogType {
        BlogType::Blogger
    }

    fn feed_data(&self) -> FeedData {
        FeedData {
            id: self.feed_id.clone(),
            key: self.key.clone(),
            title: self.api_json.name.clone(),
            url: self.api_json.url.clone(),
        }
    }

    fn entries(&self) -> Result<Vec<Entry>> {
        let mut posts: Vec<Entry> = Vec::new();

        println!(
            r#"Scraping "{}" ({} posts, {} pages)"#,
            &self.api_json.name, self.api_json.posts.total_items, self.api_json.pages.total_items
        );
        if self.api_json.posts.total_items > 0 {
            let mut next_page_token: Option<String> = None;
            let pb = init_progress_bar(self.api_json.posts.total_items as u64);
            loop {
                let mut post_resp = retry_request(self.config, || {
                    self.query_once(&self.posts_api_url, next_page_token.as_ref())
                })?;
                pb.inc(post_resp.items.len().try_into().unwrap());
                posts.extend(post_resp.items.iter().map(|p| self.post_to_entry(p)));

                next_page_token = post_resp.next_page_token.take();
                if next_page_token.is_none() {
                    break;
                }

                std::thread::sleep(std::time::Duration::from_secs(1));
            }
            pb.finish()
        }

        if self.api_json.pages.total_items > 0 {
            let page_resp = retry_request(self.config, || {
                self.query_once(&self.pages_api_url, None)
            })?;
            posts.extend(page_resp.items.iter().map(|p| self.post_to_entry(p)));
        }

        // TODO: check posts.len == blog.pages.total_items + blog.posts.total_items

        Ok(posts)
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

    fn post_to_entry(&self, post: &Post) -> Entry {
        let content = post.content.as_ref().map(|v| {
            ContentBuilder::default()
                .value(v.clone())
                .content_type(Some("html".to_string()))
                .build()
        });

        EntryBuilder::default()
            .title(post.title.clone())
            .id(format!("{}/{}", self.feed_id, post.id))
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
