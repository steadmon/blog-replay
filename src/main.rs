use std::error::Error;
use std::fs::{File, Permissions};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use atom_syndication::{Entry, Generator};
use chrono::Utc;
use clap::clap_app;

mod lib;
use lib::common::*;
use lib::blogger;

static PROG_NAME: &str = env!("CARGO_PKG_NAME");
static VERSION: &str = env!("CARGO_PKG_VERSION");
static USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));

async fn do_scrape(url: &str, config: &Config, db_path: &PathBuf) -> Result<(), Box<dyn Error>> {
    let db = sled::open(db_path)?;
    let client = reqwest::ClientBuilder::new()
        .user_agent(USER_AGENT)
        .build()?;

    let (feed_data, entries) = blogger::get_feed(&config, &client, url, 1).await?;
    let meta_tree = db.open_tree("feed_metadata")?;
    meta_tree.insert(&feed_data.key, bincode::serialize(&feed_data)?)?;
    let entry_tree = db.open_tree(format!("entries_{}", feed_data.key))?;
    for entry in &entries {
        entry_tree.insert(
            entry.published.unwrap_or(entry.updated).to_rfc3339(),
            bincode::serialize(&entry)?)?;
    }
    // Remove the correspondng generated feed if present, so that we don't duplicate entries.
    let _ = std::fs::remove_file(path_from_feed_data(config, &feed_data));

    Ok(())
}

fn generate_feed(config: &Config, feed_data: &FeedData, gen: &Generator, db: &sled::Db)
    -> Result<(), Box<dyn Error>>
{
    let feed_path = path_from_feed_data(config, &feed_data);
    let mut feed = read_or_create_feed(&feed_path, gen, &feed_data)?;
    let entry_tree = db.open_tree(format!("entries_{}", feed_data.key))?;
    let item = entry_tree.pop_min()?;
    if let Some((_, val)) = item {
        let mut entry: Entry = bincode::deserialize(&val)?;
        entry.set_updated(Utc::now());
        feed.entries.push(entry);
        if let Some(max_entries) = config.max_entries {
            let len = feed.entries.len();
            if len > max_entries {
                feed.entries.rotate_left(len - max_entries);
                feed.entries.truncate(max_entries);
            }
        }
        feed.set_updated(Utc::now());
        feed.write_to(File::create(&feed_path)?)?;
        std::fs::set_permissions(&feed_path, Permissions::from_mode(0o644))?;
    }

    Ok(())
}

fn do_generate(config: &Config, db_path: &PathBuf) -> Result<(), Box<dyn Error>> {
    let generator = Generator {
        value: String::from(PROG_NAME),
        uri: None,
        version: Some(String::from(VERSION))
    };
    let db = sled::open(db_path)?;
    let meta_tree = db.open_tree("feed_metadata")?;
    for meta in meta_tree.iter() {
        if let Ok((_, val)) = meta {
            let feed_data: FeedData = bincode::deserialize(&val)?;
            generate_feed(config, &feed_data, &generator, &db)?;
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let matches = clap_app!((PROG_NAME) =>
        (version: VERSION)
        (author: "Joshua Steadmon <josh@steadmon.net>")
        (about: "Replays blog archives into an Atom feed")
        (@subcommand scrape =>
            (about: "loads a blog's archive into the local DB for later replay")
            (@arg URL: +required "URL of the blog to scrape")
        )
        (@subcommand generate =>
            (about: "generates a feed for each blog in the local DB")
        )
    ).get_matches();

    let config: Config = confy::load(PROG_NAME)?;
    let proj_dirs = directories::ProjectDirs::from("", "", PROG_NAME).ok_or("Can't determine project dirs")?;
    let db_path = proj_dirs.data_dir().join("sled_db");

    match matches.subcommand() {
        ("scrape",   Some(sub_match)) => {
            let url_arg = sub_match.value_of("URL");
            do_scrape(url_arg.ok_or("missing URL arg")?, &config, &db_path).await
        },
        ("generate", Some(_))         => do_generate(&config, &db_path),
        _ => Ok(()),
    }
}
