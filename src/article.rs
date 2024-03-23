use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use regex::Regex;
use rss::{Guid, Item};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub enum ArticleType {
    Free,
    Paid(DateTime<Utc>),
}

#[derive(Serialize, Deserialize, Debug)]
pub struct TrackedArticle {
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

    pub fn should_publish(&self) -> bool {
        match self.article_type {
            ArticleType::Free => true,
            ArticleType::Paid(date) => date < Utc::now(),
        }
    }

    pub fn publish_to(&mut self, to: &mut Vec<Item>) {
        to.insert(0, self.item.clone());
        self.published = true;
    }

    pub fn try_publish_to(&mut self, to: &mut Vec<Item>) {
        if self.should_publish() {
            self.publish_to(to)
        }
    }
}
