use std::collections::HashSet;
use std::fs::{File, Permissions};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use anyhow::{anyhow, Context, Result};
use atom_syndication::{Entry, Generator};
use chrono::Utc;
use clap::clap_app;
use sled::transaction::ConflictableTransactionError::Abort as SledTxAbort;
use sled::transaction::{TransactionError, Transactional};

mod blogger;
mod common;
mod substack;
mod wordpress;

use common::*;

static PROG_NAME: &str = env!("CARGO_PKG_NAME");
static VERSION: &str = env!("CARGO_PKG_VERSION");
static USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));

fn do_scrape(url: &str, config: &Config, gen: &Generator, db_path: &Path) -> Result<()> {
    let db = sled::open(db_path)?;
    let client = reqwest::blocking::ClientBuilder::new()
        .user_agent(USER_AGENT)
        .build()?;

    let blog = common::get_blog(config, &client, url)?;
    let feed_data = blog.feed_data();

    let meta_tree = db.open_tree("feed_metadata")?;
    let entry_tree = db.open_tree(format!("entries_{}", feed_data.key))?;

    let tx_result = (&meta_tree, &entry_tree).transaction(|(meta_tree, entry_tree)| {
        let serialized_feed_data =
            bincode::serialize(&feed_data).map_err(|e| SledTxAbort(e.into()))?;
        meta_tree.insert(feed_data.key.as_str(), serialized_feed_data)?;
        let blog = dyn_clone::clone_box(&*blog);
        let pb_iter = init_progress_bar(blog.size_hint().0.try_into().unwrap()).wrap_iter(blog);
        for entry in pb_iter {
            let entry = entry.map_err(SledTxAbort)?;
            let serialized_entry = bincode::serialize(&entry).map_err(|e| SledTxAbort(e.into()))?;
            entry_tree.insert(
                entry
                    .published
                    .unwrap_or(entry.updated)
                    .to_rfc3339()
                    .as_str(),
                serialized_entry,
            )?;
        }
        Ok(())
    });

    match tx_result {
        Ok(_) => {}
        Err(TransactionError::Abort(e)) => return Err(e),
        Err(e) => return Err(anyhow!(e)),
    }

    // Remove the corresponding generated feed if present, so that we don't duplicate entries.
    let _ = std::fs::remove_file(path_from_feed_data(config, &feed_data));

    // Generate this feed and tell us where it's located.
    generate_feed(config, &feed_data, gen, &db)?;
    println!("\nSUCCESS: replay located at {}.atom", feed_data.id);
    Ok(())
}

fn generate_feed(
    config: &Config,
    feed_data: &FeedData,
    gen: &Generator,
    db: &sled::Db,
) -> Result<()> {
    let feed_path = path_from_feed_data(config, feed_data);
    let mut feed = read_or_create_feed(&feed_path, gen, feed_data)?;
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
        std::fs::create_dir_all(&config.feed_path)
            .with_context(|| format!("Failed to create feed directory {}", config.feed_path))?;
        feed.write_to(
            File::create(&feed_path)
                .with_context(|| format!("Failed to create {}", feed_path.display()))?,
        )
        .with_context(|| format!("Failed to write {}", feed_path.display()))?;
        std::fs::set_permissions(&feed_path, Permissions::from_mode(0o644))
            .with_context(|| format!("Failed to set permissions on {}", feed_path.display()))?;
    }

    Ok(())
}

fn do_generate(config: &Config, gen: &Generator, db_path: &Path) -> Result<()> {
    let db = sled::open(db_path)?;
    let meta_tree = db.open_tree("feed_metadata")?;
    for (_, meta) in meta_tree.iter().flatten() {
        let feed_data: FeedData = bincode::deserialize(&meta)?;
        generate_feed(config, &feed_data, gen, &db)?;
    }

    Ok(())
}

fn do_ls(db_path: &Path, long: bool, blogs: &HashSet<&str>) -> Result<()> {
    let db = sled::open(db_path)?;
    let meta_tree = db.open_tree("feed_metadata")?;
    for (key, meta) in meta_tree.iter().flatten() {
        let key = String::from_utf8(key.to_vec())?;
        if !blogs.is_empty() && !blogs.contains(key.as_str()) {
            continue;
        }
        let feed_data: FeedData = bincode::deserialize(&meta)?;
        let entry_tree = db.open_tree(format!("entries_{}", feed_data.key))?;
        if long {
            println!(
                "{} \"{}\" ({}): {} posts",
                feed_data.key,
                feed_data.title,
                feed_data.id,
                entry_tree.len()
            );
            for (_, val) in entry_tree.iter().flatten() {
                let entry: Entry = bincode::deserialize(&val)?;
                println!("   {}", entry.title.value);
            }
        } else {
            println!("{}: {} posts", feed_data.key, entry_tree.len());
        }
    }
    Ok(())
}

fn main() -> Result<()> {
    let matches = clap_app!((PROG_NAME) =>
        (version: VERSION)
        (author: "Joshua Steadmon <josh@steadmon.net>")
        (about: "Replays blog archives into an Atom feed")
        (@setting VersionlessSubcommands)
        (@subcommand scrape =>
            (about: "loads a blog's archive into the local DB for later replay")
            (@arg URL: +required "URL of the blog to scrape")
        )
        (@subcommand generate =>
            (about: "generates a feed for each blog in the local DB")
        )
        (@subcommand ls =>
            (about: "lists blog metadata from the local DB")
            (@arg LONG: -l --long "Also show cached post titles")
            (@arg BLOGS: ... "Limit to the given blog key(s)")
        )
    )
    .get_matches();

    let config: Config = confy::load(PROG_NAME)?;
    let proj_dirs = directories::ProjectDirs::from("", "", PROG_NAME)
        .ok_or(anyhow!("Can't determine project dirs"))?;
    let db_path = proj_dirs.data_dir().join("sled_db");
    let generator = Generator {
        value: String::from(PROG_NAME),
        uri: None,
        version: Some(String::from(VERSION)),
    };

    match matches.subcommand() {
        ("scrape", Some(sub_match)) => {
            let url_arg = sub_match.value_of("URL");
            do_scrape(
                url_arg.ok_or(anyhow!("missing URL arg"))?,
                &config,
                &generator,
                &db_path,
            )
        }
        ("generate", Some(_)) => do_generate(&config, &generator, &db_path),
        ("ls", Some(sub_match)) => {
            let blogs = sub_match
                .values_of("BLOGS")
                .map_or_else(HashSet::new, |b| b.collect());
            do_ls(&db_path, sub_match.is_present("LONG"), &blogs)
        }
        _ => Ok(()),
    }
}
