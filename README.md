# blog-replay

A small utility for replaying blog archives into an Atom feed. Written in Rust.

`blog-replay` scrapes articles from a given blog and stores them in a local [Sled](https://github.com/spacejam/sled) database. It can then gradually replay these articles into an Atom feed, which can be hosted on a website or consumed by a local feed reader.

## Disclaimer

This is a learning project. It has missing functionality. There are probably some dumb decisions due to me being unaware of Rust best-practices. However, it scratches an itch that I've had for a while, and I hope it will be useful to others as well.

## Installation

`blog-replay` can be installed with `cargo` or `nix`.

## Usage

### Configuration

Currently, `blog-replay` can only scrape Blogger blogs. To interact with Blogger's API, a Google API key is necessary. See [Creating an API key](https://cloud.google.com/docs/authentication/api-keys#creating_an_api_key) from Google Cloud's documentation. Once you've created the key, you can store the key in `blog-replay`'s config file, located at `~/.config/blog-replay/blog-replay.toml`. For example:

```
blogger_api_key = 'SOME_API_KEY_STRING'
feed_path = '/path/to/generated/feeds'
feed_url_base = 'URL_PREFIX_FOR_HOSTED_FEEDS'
max_retries = 5
max_entries = 20  # optional, max entries per generated feed. Defaults is unlimited
```

### Scraping

To scrape a blog's archive to local storage, run:

`blog-replay scrape <URL>`

This can be a slow operation, due to rate-limiting to avoid being blocked by Blogger's servers.

### Feed generation

To generate or update feeds for all scraped blogs, run:

`blog-replay generate`

This command takes the oldest entry out of each blog stored in the local database and adds it to the corresponding Atom file. This command should be scheduled through cron or systemd to run regularly.
