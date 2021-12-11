use clap::clap_app;

mod lib;
use lib::common::Config;
use lib::blogger;

static PROG_NAME: &str = env!("CARGO_PKG_NAME");
static VERSION: &str = env!("CARGO_PKG_VERSION");
static USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));

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
    ).get_matches();

    let config: Config = confy::load(PROG_NAME).unwrap();

    if let Some(scrape_matches) = matches.subcommand_matches("scrape") {
        let client = reqwest::ClientBuilder::new()
            .user_agent(USER_AGENT)
            .build()
            .unwrap();

        let url = scrape_matches.value_of("URL").unwrap();
        blogger::get_posts(&config, &client, url, 1).await.unwrap();
    }
}
