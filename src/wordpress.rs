use std::collections::HashMap;
use std::error::Error;

use atom_syndication::{ContentBuilder, Entry, EntryBuilder, LinkBuilder, Person};
use reqwest::{Client, Url};
use serde::Deserialize;

use crate::common::*;

#[derive(Deserialize, Debug)]
struct Blog {
    name: String,
    home: String,
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

fn post_to_entry(post: &Post, blog_id: &str, author_map: &HashMap<usize, String>) -> Entry {
    let content = ContentBuilder::default()
        .value(post.content.rendered.clone())
        .content_type(Some("html".to_string()))
        .build();

    EntryBuilder::default()
        .title(post.title.rendered.clone())
        .id(format!("{}/{}", blog_id, post.id))
        .published(parse_assuming_utc(&post.date_gmt))
        .author(Person {
            name: author_map[&post.author].clone(),
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

async fn get_blog_once(client: &Client, api_url: &Url) -> anyhow::Result<Blog> {
    let resp = client
        .get(api_url.clone())
        .send()
        .await?;

    Ok(resp.error_for_status()?.json().await?)
}

async fn get_users_once(client: &Client, api_url: &Url) -> anyhow::Result<HashMap<usize, String>> {
    let resp = client
        .get(api_url.clone())
        .send()
        .await?;

    let mut users: Vec<User> = resp.error_for_status()?.json().await?;
    Ok(users.drain(..).map(|u| (u.id, u.name)).collect())
}

async fn get_page_once(
    client: &Client,
    api_url: &Url,
    page: usize,
) -> anyhow::Result<(Vec<Post>, usize, usize)> {
    let req = client
        .get(api_url.clone())
        .query(&[("page", &format!("{page}"))]);

    let resp = req.send().await?.error_for_status()?;

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
    let posts = resp.json().await?;

    Ok((posts, items, pages))
}

pub async fn detect(config: &Config, client: &Client, blog_url: &str) -> bool {
    // Technically we should use a HEAD request to discover[1] the API base (if it exists), but
    // this doesn't seem to be enabled on all sites.
    // [1]: https://developer.wordpress.org/rest-api/using-the-rest-api/discovery/#discovering-the-api
    let blog_api_url = Url::parse(format!("{blog_url}/wp-json/").as_str()).unwrap();
    let res = retry_request(config, || {
        get_blog_once(client, &blog_api_url)
    }).await;

    res.is_ok()
}

pub async fn get_feed(
    config: &Config,
    client: &Client,
    blog_url: &str,
) -> Result<(FeedData, Vec<Entry>), Box<dyn Error>> {
    let mut posts: Vec<Post> = Vec::new();

    let blog_api_url = Url::parse(&format!("{blog_url}/wp-json/"))?;
    let blog = retry_request(config, || {
        get_blog_once(client, &blog_api_url)
    })
    .await?;

    // Get author map
    let users_api_url = Url::parse(&format!("{blog_url}/wp-json/wp/v2/users"))?;
    let authors = retry_request(config, || {
        get_users_once(client, &users_api_url)
    })
    .await?;

    // Get # api pages & # items
    let posts_api_url = Url::parse(&format!("{blog_url}/wp-json/wp/v2/posts"))?;
    let mut api_page = 1;
    let mut pb: Option<indicatif::ProgressBar> = None;
    loop {
        let (mut tmp_posts, num_posts, num_api_pages) = retry_request(config, || {
            get_page_once(client, &posts_api_url, api_page)
        }).await?;
        if api_page == 1 {
            println!(r#"Scraping "{}" ({} posts)"#, blog.name, num_posts);
            pb = Some(init_progress_bar(num_posts.try_into().unwrap()));
        }
        if let Some(pb) = pb.as_ref() { pb.inc(tmp_posts.len().try_into().unwrap()) };
        posts.append(&mut tmp_posts);
        if api_page == num_api_pages || posts.len() == num_posts {
            break;
        }

        api_page += 1;
    }
    if let Some(pb) = pb { pb.finish() };

    // Repeat for posts vs. pages
    let posts_api_url = Url::parse(&format!("{blog_url}/wp-json/wp/v2/pages"))?;
    let mut api_page = 1;
    let mut pb: Option<indicatif::ProgressBar> = None;
    loop {
        let (mut tmp_posts, num_posts, num_api_pages) = retry_request(config, || {
            get_page_once(client, &posts_api_url, api_page)
        }).await?;
        if api_page == 1 {
            println!(r#"Scraping "{}" ({} pages)"#, blog.name, num_posts);
            pb = Some(init_progress_bar(num_posts.try_into().unwrap()));
        }
        if let Some(pb) = pb.as_ref() { pb.inc(tmp_posts.len().try_into().unwrap()) };
        posts.append(&mut tmp_posts);
        if api_page == num_api_pages || posts.len() == num_posts {
            break;
        }

        api_page += 1;
    }
    if let Some(pb) = pb { pb.finish() };

    let blog_key = sanitize_blog_key(&blog.name);
    let blog_id = format!("{}/{}", config.feed_url_base, blog_key);
    let feed_data = FeedData {
        id: blog_id.clone(),
        key: blog_key,
        title: blog.name,
        url: blog.home,
    };
    let entries: Vec<Entry> = posts.iter().map(|p| post_to_entry(p, &blog_id, &authors)).collect();
    Ok((feed_data, entries))
}
