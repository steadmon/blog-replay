use std::collections::HashMap;

use atom_syndication::{ContentBuilder, Entry, EntryBuilder, LinkBuilder, Person};
use reqwest::Url;
use reqwest::blocking::Client;
use serde::Deserialize;

use crate::common::*;

// Parsed from Wordpress API endpoint
#[derive(Deserialize, Debug)]
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
    }))
}

fn get_users_once(client: &Client, url: &Url) -> anyhow::Result<HashMap<usize, String>> {
    let resp = client.get(url.clone()).send()?;
    let mut users: Vec<User> = resp.error_for_status()?.json()?;
    Ok(users.drain(..).map(|u| (u.id, u.name)).collect())
}

struct WordpressBlog<'a> {
    api_json: WordpressJson,
    posts_api_url: Url,
    pages_api_url: Url,
    key: String,
    feed_id: String,
    config: &'a Config,
    client: &'a Client,
    authors: HashMap<usize, String>,
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

    fn entries(&mut self) -> anyhow::Result<Vec<Entry>> {
        let mut posts: Vec<Entry> = Vec::new();

        // Get # api pages & # items
        for (url, display) in &[(&self.posts_api_url, "posts"), (&self.pages_api_url, "pages")] {
            let mut api_page = 1;
            let mut pb: Option<indicatif::ProgressBar> = None;
            loop {
                let (tmp_posts, num_posts, num_api_pages) = retry_request(self.config, || {
                    self.get_page_once(url, api_page)
                })?;
                if api_page == 1 {
                    println!(r#"Scraping "{}" ({} {})"#, &self.api_json.name, num_posts, display);
                    pb = Some(init_progress_bar(num_posts.try_into().unwrap()));
                }
                if let Some(pb) = pb.as_ref() { pb.inc(tmp_posts.len().try_into().unwrap()) };
                posts.extend(tmp_posts.iter().map(|p| self.post_to_entry(p)));
                if api_page == num_api_pages || posts.len() == num_posts {
                    break;
                }

                api_page += 1;
            }
            if let Some(pb) = pb { pb.finish() };
        }

        Ok(posts)
    }
}

impl WordpressBlog<'_> {
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
