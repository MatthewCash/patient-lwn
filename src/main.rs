use article::TrackedArticle;
use chrono::{DateTime, Utc};
use std::process::exit;

mod article;
mod data;

#[tokio::main]
async fn main() {
    let input_feed = data::get_input_feed().await.unwrap_or_else(|why| {
        eprintln!("Failed to get input RSS feed: {:?}", why);
        exit(1);
    });

    let mut output_feed = data::get_output_feed().await.unwrap_or_else(|why| {
        eprintln!("Failed to get output RSS feed: {:?}", why);
        exit(1);
    });

    let mut tracked_articles = data::get_tracked_items().await.unwrap_or_else(|why| {
        eprintln!("Failed to get tracked articles: {:?}", why);
        exit(1);
    });

    output_feed.set_pub_date(input_feed.pub_date.clone());
    output_feed.set_last_build_date(DateTime::to_rfc2822(&Utc::now()));

    tracked_articles.retain_mut(|article| {
        article.try_publish_to(&mut output_feed.items);

        // Stop tracking articles that have been published and are not in the feed
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

    if let Err(why) = data::save_tracked_items(&tracked_articles).await {
        eprintln!("Failed to save tracked articles: {:?}", why);
    }

    if let Err(why) = data::save_output_feed(&output_feed).await {
        eprintln!("Failed to save output feed: {:?}", why);
    }
}
