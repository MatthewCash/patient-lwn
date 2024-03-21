use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use regex::Regex;
use rss::{Channel, Guid, Item};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::error::Error;
use std::process::exit;
use std::{env, io::BufReader};
use tokio::fs::{self, File, OpenOptions};

#[derive(Serialize, Deserialize, Debug)]
enum ArticleType {
    Free,
    Paid(DateTime<Utc>),
}

#[derive(Serialize, Deserialize, Debug)]
struct TrackedArticle {
    pub guid: Guid,
    pub published: bool,
    pub article_type: ArticleType,

    pub item: Item,
}

async fn get_item_text(url: &str) -> Result<String, reqwest::Error> {
    reqwest::get(url).await?.text().await
}

fn get_date_from_text(text: &str) -> Option<DateTime<Utc>> {
    let regexp = Regex::new(r"freely\s*available\s*on\s*(\w+)\s*(\d{1,2}),\s*(\d{4})").ok()?;
    let (_, [month, day, year]) = regexp.captures(text)?.extract();

    Some(
        Utc.from_utc_datetime(
            &NaiveDate::parse_from_str(&format!("{} {}, {}", month, day, year), "%B %d, %Y")
                .ok()?
                .and_hms_opt(0, 0, 0)?,
        ),
    )
}

impl TrackedArticle {
    pub async fn new(item: Item) -> Self {
        let article_type = if item
            .title
            .as_ref()
            .is_some_and(|title| title.starts_with("[$]"))
        {
            let text = get_item_text(item.link.as_ref().expect("Article missing link!"))
                .await
                .unwrap();
            let date = get_date_from_text(&text).expect("Failed to get date from article text!");
            ArticleType::Paid(date)
        } else {
            ArticleType::Free
        };

        TrackedArticle {
            guid: item.guid.as_ref().expect("Article missing GUID!").clone(),
            published: false,
            item,
            article_type,
        }
    }

    fn should_publish(&self) -> bool {
        match self.article_type {
            ArticleType::Free => true,
            ArticleType::Paid(date) => date < Utc::now(),
        }
    }

    fn publish_to(&mut self, to: &mut Vec<Item>) {
        to.insert(0, self.item.clone());
        self.published = true;
    }

    fn try_publish_to(&mut self, to: &mut Vec<Item>) {
        if self.should_publish() {
            self.publish_to(to)
        }
    }
}

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

    output_feed.pretty_write_to(feed_file.into_std().await, b' ', 4)?;

    Ok(())
}

fn get_tracked_items_path() -> String {
    env::var("TRACKED_ARTICLES_PATH")
        .ok()
        .unwrap_or_else(|| "./data/tracked.json".into())
}

async fn get_tracked_items() -> Result<Vec<TrackedArticle>, Box<dyn Error>> {
    Ok(serde_json::from_str(
        &fs::read_to_string(get_tracked_items_path()).await?,
    )?)
}

async fn save_tracked_items(tracked_items: &Vec<TrackedArticle>) -> Result<(), std::io::Error> {
    fs::write(get_tracked_items_path(), json!(tracked_items).to_string()).await
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

    let mut tracked_articles = get_tracked_items().await.unwrap_or_else(|why| {
        eprintln!("Failed to get tracked articles: {:?}", why);
        exit(1);
    });

    output_feed.set_pub_date(input_feed.pub_date.clone());
    output_feed.set_last_build_date(DateTime::to_rfc2822(&Utc::now()));

    tracked_articles.retain_mut(|article| {
        article.try_publish_to(&mut output_feed.items);

        // Stop tracking articles that are not in the feed and are free or new
        article.published
            && !input_feed
                .items()
                .iter()
                .any(|item| *item.guid.as_ref().unwrap() == article.guid)
    });

    for item in &input_feed.items {
        // Only consider articles that we are not already tracking
        if !tracked_articles
            .iter()
            .any(|article| *item.guid.as_ref().unwrap() == article.guid)
        {
            let mut article = TrackedArticle::new(item.clone()).await;

            article.try_publish_to(&mut output_feed.items);

            tracked_articles.push(article);
        }
    }

    output_feed.items.truncate(input_feed.items.len());

    if let Err(why) = save_tracked_items(&tracked_articles).await {
        eprintln!("Failed to save tracked articles: {:?}", why);
    }

    if let Err(why) = save_output_feed(&output_feed).await {
        eprintln!("Failed to save output feed: {:?}", why);
    }
}
