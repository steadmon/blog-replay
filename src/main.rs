use std::path::{Path, PathBuf};

use atom_syndication::Generator;
use clap::{ArgMatches, clap_app};

mod lib;
use lib::common::{Config, FeedData, write_feed};
use lib::blogger;

static PROG_NAME: &str = env!("CARGO_PKG_NAME");
static VERSION: &str = env!("CARGO_PKG_VERSION");
static USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));

async fn do_scrape<'a>(sub_match: &ArgMatches<'a>, config: &Config, db_path: &PathBuf) {
    let db = sled::open(db_path).unwrap();
    let client = reqwest::ClientBuilder::new()
        .user_agent(USER_AGENT)
        .build()
        .unwrap();

    let url = sub_match.value_of("URL").unwrap();
    let feed_data = blogger::get_feed(&config, &client, url, 1).await.unwrap();
    db.insert(&feed_data.key, bincode::serialize(&feed_data).unwrap()).unwrap();
}

async fn do_generate<'a>(config: &Config, db_path: &PathBuf) {
    let generator = Generator {
        value: String::from(PROG_NAME),
        uri: None,
        version: Some(String::from(VERSION))
    };
    let db = sled::open(db_path).unwrap();
    for i in db.iter() {
        if let Ok((key, val)) = i {
            let key = std::str::from_utf8(&key).unwrap();
            let feed_data: FeedData = bincode::deserialize(&val).unwrap();
            let mut feed_path = Path::new(&config.feed_path).join(&key);
            feed_path.set_extension("xml");
            write_feed(&feed_path, &generator, feed_data).unwrap();
        }
    }
}

#[tokio::main]
async fn main() {
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

    let config: Config = confy::load(PROG_NAME).unwrap();
    let proj_dirs = directories::ProjectDirs::from("", "", PROG_NAME).unwrap();
    let db_path = proj_dirs.data_dir().join("sled_db");

    match matches.subcommand() {
        ("scrape",   Some(sub_match)) => do_scrape(sub_match, &config, &db_path).await,
        ("generate", Some(_))         => do_generate(&config, &db_path).await,
        _ => (),
    }
}
