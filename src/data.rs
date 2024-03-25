use article::TrackedArticle;
use rss::Channel;
use serde_json::json;
use std::error::Error;
use std::{env, io::BufReader};
use tokio::fs::{self, File, OpenOptions};

use crate::article;

pub async fn get_input_feed() -> Result<Channel, Box<dyn Error>> {
    let feed_url = env::var("INPUT_FEED_URL")
        .ok()
        .unwrap_or_else(|| "https://lwn.net/headlines/rss".into());

    let content = reqwest::get(feed_url).await?.bytes().await?;
    Ok(Channel::read_from(&*content)?)
}

pub fn get_output_feed_path() -> String {
    env::var("OUTPUT_FEED_PATH")
        .ok()
        .unwrap_or_else(|| "./data/feed.xml".into())
}

pub async fn get_output_feed() -> Result<Channel, Box<dyn Error>> {
    let file = File::open(get_output_feed_path()).await?;
    Ok(Channel::read_from(BufReader::new(file.into_std().await))?)
}

pub async fn save_output_feed(output_feed: &Channel) -> Result<(), Box<dyn Error>> {
    let feed_file = OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(get_output_feed_path())
        .await?;

    output_feed.pretty_write_to(feed_file.into_std().await, b' ', 4)?;

    Ok(())
}

pub fn get_tracked_items_path() -> String {
    env::var("TRACKED_ARTICLES_PATH")
        .ok()
        .unwrap_or_else(|| "./data/tracked.json".into())
}

pub async fn get_tracked_items() -> Result<Vec<TrackedArticle>, Box<dyn Error>> {
    Ok(serde_json::from_str(
        &fs::read_to_string(get_tracked_items_path()).await?,
    )?)
}

pub async fn save_tracked_items(tracked_items: &Vec<TrackedArticle>) -> Result<(), std::io::Error> {
    fs::write(get_tracked_items_path(), json!(tracked_items).to_string()).await
}
