use std::error::Error;
use std::path::{Path, PathBuf};

use atom_syndication::Generator;
use clap::clap_app;

mod lib;
use lib::common::{Config, FeedData, write_feed};
use lib::blogger;

static PROG_NAME: &str = env!("CARGO_PKG_NAME");
static VERSION: &str = env!("CARGO_PKG_VERSION");
static USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));

async fn do_scrape<'a>(url: &str, config: &Config, db_path: &PathBuf) -> Result<(), Box<dyn Error>> {
    let db = sled::open(db_path)?;
    let client = reqwest::ClientBuilder::new()
        .user_agent(USER_AGENT)
        .build()?;

    let feed_data = blogger::get_feed(&config, &client, url, 1).await?;
    db.insert(&feed_data.key, bincode::serialize(&feed_data)?)?;

    Ok(())
}

async fn do_generate<'a>(config: &Config, db_path: &PathBuf) -> Result<(), Box<dyn Error>> {
    let generator = Generator {
        value: String::from(PROG_NAME),
        uri: None,
        version: Some(String::from(VERSION))
    };
    let db = sled::open(db_path)?;
    for i in db.iter() {
        if let Ok((key, val)) = i {
            let key = std::str::from_utf8(&key)?;
            let feed_data: FeedData = bincode::deserialize(&val)?;
            let mut feed_path = Path::new(&config.feed_path).join(&key);
            feed_path.set_extension("xml");
            write_feed(&feed_path, &generator, feed_data)?;
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let matches = clap_app!((PROG_NAME) =>
        (version: VERSION)
        (author: "Joshua Steadmon <josh@steadmon.net>")
        (about: "Replays blog archives into an RSS feed")
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
        ("generate", Some(_))         => do_generate(&config, &db_path).await,
        _ => Ok(()),
    }
}
