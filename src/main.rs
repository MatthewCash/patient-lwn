use chrono::{DateTime, Duration, Utc};
use rss::{Channel, Item};
use serde_json::json;
use std::error::Error;
use std::process::exit;
use std::{env, io::BufReader};
use tokio::fs::{self, File, OpenOptions};

async fn get_input_feed() -> Result<Channel, Box<dyn Error>> {
    let feed_url = env::var("INPUT_FEED_URL")
        .ok()
        .unwrap_or_else(|| "https://lwn.net/headlines/rss".into());

    let content = reqwest::get(feed_url).await?.bytes().await?;
    Ok(Channel::read_from(&*content)?)
}

fn get_output_feed_path() -> String {
    env::var("OUTPUT_FEED_PATH")
        .ok()
        .unwrap_or_else(|| "./data/feed.xml".into())
}

async fn get_output_feed() -> Result<Channel, Box<dyn Error>> {
    let file = File::open(get_output_feed_path()).await?;
    Ok(Channel::read_from(BufReader::new(file.into_std().await))?)
}

async fn save_output_feed(output_feed: &Channel) -> Result<(), Box<dyn Error>> {
    let feed_file = OpenOptions::new()
        .write(true)
        .open(get_output_feed_path())
        .await?;

    output_feed.write_to(feed_file.into_std().await)?;

    Ok(())
}

fn get_tracked_items_path() -> String {
    env::var("TRACKED_ARTICLES_PATH")
        .ok()
        .unwrap_or_else(|| "./data/tracked.json".into())
}

async fn get_tracked_items() -> Result<Vec<Item>, Box<dyn Error>> {
    Ok(serde_json::from_str(
        &fs::read_to_string(get_tracked_items_path()).await?,
    )?)
}

async fn save_tracked_items(tracked_items: &Vec<Item>) -> Result<(), std::io::Error> {
    fs::write(get_tracked_items_path(), json!(tracked_items).to_string()).await
}

fn is_paid(article: &Item) -> bool {
    article
        .title
        .as_ref()
        .is_some_and(|title| title.starts_with("[$]"))
}

fn is_old(article: &Item) -> bool {
    let one_week_ago = Utc::now() - Duration::try_weeks(1).unwrap();

    article
        .pub_date
        .as_ref()
        .and_then(|date| DateTime::parse_from_rfc2822(date).ok())
        .is_some_and(|date| date < one_week_ago)
}

#[tokio::main]
async fn main() {
    let input_feed = get_input_feed().await.unwrap_or_else(|why| {
        eprintln!("Failed to get input RSS feed: {:?}", why);
        exit(1);
    });

    let mut output_feed = get_output_feed().await.unwrap_or_else(|why| {
        eprintln!("Failed to get output RSS feed: {:?}", why);
        exit(1);
    });

    let mut tracked_items = get_tracked_items().await.unwrap_or_else(|why| {
        eprintln!("Failed to get tracked articles: {:?}", why);
        exit(1);
    });

    output_feed.set_pub_date(input_feed.pub_date.clone());
    output_feed.set_last_build_date(DateTime::to_rfc2822(&Utc::now()));

    tracked_items.retain_mut(|article| {
        // If the article is old enough, publish it
        if is_old(article) {
            output_feed.items.insert(0, article.clone());
            // Set the date far in the future so that it will never be published twice
            article.set_pub_date("Sat, 31 Dec 9999 23:59:59 +1400".to_string())
        }

        // Stop tracking articles that are not in the feed and are free or new
        (is_paid(article) && !is_old(article))
            || input_feed
                .items()
                .iter()
                .any(|item| item.guid == article.guid)
    });

    input_feed.items.iter().for_each(|article| {
        // Only consider articles that we are not already tracking
        if !tracked_items.iter().any(|item| item.guid == article.guid) {
            tracked_items.push(article.clone());

            // If it is free, immediately publish it
            if !is_paid(article) {
                output_feed.items.push(article.clone());
            }
        }
    });

    output_feed.items.truncate(input_feed.items.len());

    if let Err(why) = save_tracked_items(&tracked_items).await {
        eprintln!("Failed to save tracked articles: {:?}", why);
    }

    if let Err(why) = save_output_feed(&output_feed).await {
        eprintln!("Failed to save output feed: {:?}", why);
    }
}
