use crate::CliConfig;
use crate::podcast_feed;
use anyhow::{Context, Result, anyhow};
use chrono::prelude::*;
use clap::Args;
use futures::StreamExt;
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use reqwest_retry::Jitter;
use reqwest_retry::policies::ExponentialBackoff;
use reqwest_retry::{
    RetryTransientMiddleware, Retryable, RetryableStrategy, default_on_request_failure,
    default_on_request_success,
};
use serde::{Deserialize, Serialize};
use soup::{NodeExt, QueryBuilderExt, Soup};
use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::time::Duration;
use syndication::Feed;
use taskpaper::{Database, Position, sanitize_item_text};

const TASKPAPER_RSS_DONE_FILE: &str = ".taskpaper_rss_done.toml";

/// How many feeds are fetched at the same time. Firing all ~100 feeds (most of them YouTube) at
/// once is what makes YouTube start answering with errors.
const MAX_CONCURRENT_FEEDS: usize = 8;

/// Retries of a failed request. YouTube's errors often last for minutes, so the waits between
/// attempts grow to `MAX_RETRY_INTERVAL`: 5s, 5-10s, 5-20s, 5-40s, 5-80s, 5-120s, roughly 2.5
/// minutes on average before a feed is given up on.
const MAX_RETRIES: u32 = 6;
const MIN_RETRY_INTERVAL: Duration = Duration::from_secs(5);
const MAX_RETRY_INTERVAL: Duration = Duration::from_secs(120);

#[derive(Debug, Serialize, Deserialize, Copy, Clone)]
enum FeedPresentation {
    #[serde(rename = "feed")]
    FromFeed,

    #[serde(rename = "website")]
    FromWebsite,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FeedConfiguration {
    pub url: String,
    presentation: Option<FeedPresentation>,
    tags: Option<Vec<String>>,
    /// File name, relative to `--podcast-dir`, of a podcast RSS feed built from the AI narration
    /// of this feed's posts. Only works for Substack publications, see `podcast_feed`.
    pub podcast_file: Option<String>,
}

#[derive(Args, Debug)]
pub struct CommandLineArguments {
    /// Directory to write podcast feeds into, one per feed that has a `podcast_file` in the
    /// config. Without this flag no podcast feeds are written.
    #[arg(long)]
    podcast_dir: Option<PathBuf>,
}

pub fn run(db: &Database, args: &CommandLineArguments, cli_config: &CliConfig) -> Result<()> {
    let rt = tokio::runtime::Runtime::new()?;

    let archive = db.root.join(TASKPAPER_RSS_DONE_FILE);
    let mut seen_ids = match fs::read_to_string(&archive) {
        Ok(data) => toml::from_str(&data)
            .with_context(|| format!("Could not parse {}", archive.display()))?,
        Err(e) if e.kind() == io::ErrorKind::NotFound => SeenIds::default(),
        Err(e) => return Err(e.into()),
    };

    let seen_ids_ref = &seen_ids.seen_ids;
    let result: Result<Vec<TaskItem>> = rt.block_on(async {
        let client = build_client()?;

        let (feeds, podcast_errors) = futures::future::join(
            read_feeds(&client, &cli_config.feeds, seen_ids_ref),
            podcast_feed::write_all(&client, &cli_config.feeds, args.podcast_dir.as_deref()),
        )
        .await;
        let feeds = feeds?;
        let mut rv = Vec::new();
        let mut errors: Vec<String> = podcast_errors;
        for (feed, feed_config) in feeds.into_iter().zip(&cli_config.feeds) {
            match feed {
                Ok(feed_items) => rv.extend(feed_items.into_iter()),
                Err(e) => {
                    errors.push(format!("{}: {:?}", feed_config.url, e));
                }
            }
        }

        if !errors.is_empty() {
            let mut note_text = Vec::new();
            for error in &errors {
                note_text.extend(textwrap::wrap(error, 80).into_iter().map(|l| l.to_string()));
            }
            rv.push(TaskItem {
                title: format!(
                    "Could not fetch or write {} RSS feed{}.",
                    errors.len(),
                    if errors.len() == 1 { "" } else { "s" }
                ),
                note_text,
                guid: None,
                tags: Vec::new(),
            });
        }

        Ok(rv)
    });
    let result = result?;

    let mut inbox = db.parse_common_file(taskpaper::CommonFileKind::Inbox)?;

    for item in result {
        let mut text = sanitize_item_text(&item.title);
        for tag in item.tags {
            // We push these as text, because then arguments are pushed correctly too.
            text.push(' ');
            text.push_str(&tag);
        }
        let node_id = inbox.insert(
            taskpaper::Item::new(taskpaper::ItemKind::Task, text),
            Position::AsLast,
        );

        for line in item.note_text {
            let text = sanitize_item_text(&line);
            inbox.insert(
                taskpaper::Item::new(taskpaper::ItemKind::Note, text),
                Position::AsLastChildOf(&node_id),
            );
        }

        if let Some(guid) = item.guid {
            seen_ids.seen_ids.insert(guid);
        }
    }

    db.overwrite_common_file(&inbox, taskpaper::CommonFileKind::Inbox)?;
    std::fs::write(&archive, toml::to_string_pretty(&seen_ids).unwrap())?;

    Ok(())
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct SeenIds {
    seen_ids: BTreeSet<String>,
}

/// Broken down information for tasks.
#[derive(Debug)]
pub struct TaskItem {
    pub title: String,
    pub note_text: Vec<String>,
    pub guid: Option<String>,
    pub tags: Vec<String>,
}

fn parse_date(input_opt: Option<&str>) -> Option<DateTime<Utc>> {
    let input = input_opt?;
    let (naive_date, offset) = dtparse::parse(&input).ok()?;
    let result = match offset {
        Some(offset) => {
            let local = offset.from_local_datetime(&naive_date).single().unwrap();
            local.into()
        }
        None => DateTime::from_naive_utc_and_offset(naive_date, Utc),
    };
    Some(result)
}

/// Treats 404 as transient (retryable) since YouTube intermittently returns 404
/// for valid RSS feeds. All other responses use the default retry classification.
struct RetryOn404;

impl RetryableStrategy for RetryOn404 {
    fn handle(
        &self,
        res: &Result<reqwest::Response, reqwest_middleware::Error>,
    ) -> Option<Retryable> {
        match res {
            Ok(response) if response.status() == reqwest::StatusCode::NOT_FOUND => {
                Some(Retryable::Transient)
            }
            Ok(response) => default_on_request_success(response),
            Err(error) => default_on_request_failure(error),
        }
    }
}

pub fn build_client() -> Result<ClientWithMiddleware> {
    build_client_with_retry_bounds(MIN_RETRY_INTERVAL, MAX_RETRY_INTERVAL)
}

fn build_client_with_retry_bounds(
    min_interval: Duration,
    max_interval: Duration,
) -> Result<ClientWithMiddleware> {
    // Bounded jitter never waits less than `min_interval`. The default (full) jitter can pick
    // waits close to zero, so all retries could be over within a second or two.
    let retry_policy = ExponentialBackoff::builder()
        .retry_bounds(min_interval, max_interval)
        .jitter(Jitter::Bounded)
        .build_with_max_retries(MAX_RETRIES);
    let client = ClientBuilder::new(reqwest::Client::builder().build()?)
        .with(RetryTransientMiddleware::new_with_policy_and_strategy(
            retry_policy,
            RetryOn404,
        ))
        .build();
    Ok(client)
}

pub fn get_summary_blocking(url: &str) -> Result<Option<TaskItem>> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let client = build_client()?;
        Ok(get_summary(&client, url, None).await?)
    })
}

async fn get_page_body(
    client: &reqwest_middleware::ClientWithMiddleware,
    url: &str,
) -> Result<String> {
    // Without `error_for_status` the HTML of an error page would be returned as body and later
    // be reported as unparsable feed, hiding the actual HTTP status.
    Ok(client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?)
}

/// Turns a url into a TaskItem, suitable for use in the inbox.
async fn get_summary(
    client: &ClientWithMiddleware,
    url: &str,
    guid: Option<String>,
) -> Result<Option<TaskItem>> {
    let text = get_page_body(client, url).await?;
    let soup = Soup::new(&text);

    let mut title_text_lines = Vec::new();
    // Find and push the title.
    if let Some(node) = soup.tag("title").find() {
        let text = node.text().trim().to_string();
        if !text.is_empty() {
            title_text_lines.push(text);
        }
    };
    let mut extra_notes = Vec::new();
    // Find and push the description.
    if let Some(node) = soup.tag("meta").attr("name", "description").find() {
        if let Some(t) = node.attrs().get("content") {
            match t.len() {
                0 => (),
                1..=100 => title_text_lines.push(t.to_string()),
                _ => {
                    extra_notes.extend(textwrap::wrap(t, 80).into_iter().map(|l| l.to_string()));
                }
            }
        }
    };
    if title_text_lines.is_empty() {
        title_text_lines.push(url.to_string());
    }
    let mut note_text = Vec::new();
    note_text.push(url.to_string());
    note_text.extend(extra_notes.into_iter());
    Ok(Some(TaskItem {
        title: title_text_lines.join(" • "),
        note_text,
        guid,
        tags: Vec::new(),
    }))
}

async fn get_summary_or_current_information(
    client: &ClientWithMiddleware,
    feed_presentation: FeedPresentation,
    url: &str,
    title: String,
    content: String,
    published: Option<DateTime<Utc>>,
    guid: Option<String>,
) -> Result<TaskItem> {
    let task = match feed_presentation {
        FeedPresentation::FromWebsite => get_summary(client, url, guid)
            .await?
            .expect("Did not receive a useful summary."),
        FeedPresentation::FromFeed => {
            let mut note_text = vec![url.to_string()];
            if let Some(d) = published {
                let local: DateTime<Local> = d.into();
                note_text.push(format!("Published: {}", local.format("%Y-%m-%d")));
            }
            let content = html2text::from_read(io::Cursor::new(content), 80);
            if !content.is_empty() {
                let lines: Vec<String> = content
                    .split('\n')
                    .map(|s| s.to_string())
                    .filter(|l| !l.is_empty())
                    .collect();
                note_text.extend(match lines.len() {
                    0..=100 => lines.into_iter().take(50),
                    _ => lines.into_iter().take(15),
                });
            }
            TaskItem {
                title,
                note_text,
                guid,
                tags: Vec::new(),
            }
        }
    };
    Ok(task)
}

/// Returns a vector of same length then feeds, which contains either an Err if the feed could not
/// be read or a list of items that we did not see before on any prior run.
async fn read_feeds(
    client: &ClientWithMiddleware,
    feeds: &[FeedConfiguration],
    seen_ids: &BTreeSet<String>,
) -> Result<Vec<Result<Vec<TaskItem>>>> {
    let mut futures = Vec::new();
    for feed in feeds {
        let presentation = feed.presentation.unwrap_or(FeedPresentation::FromWebsite);

        futures.push(async move {
            let body = get_page_body(client, &feed.url).await?;
            let mut items = Vec::new();
            match body
                .parse::<Feed>()
                .map_err(|e| anyhow!("Could not parse for {}: {}", feed.url, e))?
            {
                Feed::RSS(channel) => {
                    for item in channel.items() {
                        let url = item.link();
                        if url.is_none() {
                            continue;
                        }
                        let published = parse_date(item.pub_date());
                        let content = item.content().or_else(|| item.description()).unwrap_or("");
                        let guid = item
                            .guid()
                            .map(|g| g.value())
                            .unwrap_or_else(|| url.unwrap())
                            .to_string();
                        if seen_ids.contains(&guid) {
                            continue;
                        }

                        let title = item
                            .title()
                            .unwrap_or_else(|| "No Title")
                            .trim()
                            .to_string();
                        let mut task = get_summary_or_current_information(
                            client,
                            presentation,
                            url.unwrap(),
                            title,
                            content.to_string(),
                            published,
                            Some(guid),
                        )
                        .await?;
                        if let Some(tags) = &feed.tags {
                            task.tags.extend(tags.iter().cloned());
                        }
                        items.push(task);
                    }
                }
                Feed::Atom(channel) => {
                    for entry in channel.entries() {
                        let urls: Vec<_> =
                            entry.links().iter().map(|l| l.href().to_string()).collect();
                        if urls.is_empty() {
                            continue;
                        }
                        let content = {
                            entry
                                .content()
                                .and_then(|v| v.value())
                                .or_else(|| entry.summary())
                                .unwrap_or("")
                        };
                        let guid = entry.id().to_string();
                        if seen_ids.contains(&guid) {
                            continue;
                        }

                        let published = parse_date(entry.published());
                        let title = entry.title().trim().to_string();
                        let mut task = get_summary_or_current_information(
                            client,
                            presentation,
                            urls.first().unwrap(),
                            title,
                            content.to_string(),
                            published,
                            Some(guid),
                        )
                        .await?;
                        if let Some(tags) = &feed.tags {
                            task.tags.extend(tags.iter().cloned());
                        }
                        items.push(task);
                    }
                }
            };
            let rv: Result<Vec<TaskItem>> = Ok(items);
            rv
        })
    }

    let rv = futures::stream::iter(futures)
        .buffered(MAX_CONCURRENT_FEEDS)
        .collect()
        .await;
    Ok(rv)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const ATOM: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
        <feed xmlns="http://www.w3.org/2005/Atom">
          <id>yt:channel:test</id><title>Channel</title><updated>2026-09-01T00:00:00Z</updated>
          <entry>
            <id>yt:video:abc</id><title>Video</title><updated>2026-09-01T00:00:00Z</updated>
            <published>2026-09-01T00:00:00+00:00</published>
            <link rel="alternate" href="https://www.youtube.com/watch?v=abc"/>
          </entry>
        </feed>"#;

    /// Retries without waiting, so the tests run fast.
    fn fast_client() -> ClientWithMiddleware {
        build_client_with_retry_bounds(Duration::from_millis(1), Duration::from_millis(5)).unwrap()
    }

    fn feed(url: String) -> FeedConfiguration {
        FeedConfiguration {
            url,
            presentation: Some(FeedPresentation::FromFeed),
            tags: Some(vec!["@youtube".into()]),
            podcast_file: None,
        }
    }

    fn read(feeds: &[FeedConfiguration]) -> Vec<Result<Vec<TaskItem>>> {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(read_feeds(&fast_client(), feeds, &BTreeSet::new()))
            .unwrap()
    }

    #[test]
    fn feed_is_read_after_transient_errors() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let server = rt.block_on(async {
            let server = MockServer::start().await;
            // Mocks are matched in mount order, the first one is used up after two requests.
            Mock::given(method("GET"))
                .and(path("/feed"))
                .respond_with(ResponseTemplate::new(404).set_body_string("<html>404</html>"))
                .up_to_n_times(2)
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path("/feed"))
                .respond_with(ResponseTemplate::new(503))
                .up_to_n_times(1)
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path("/feed"))
                .respond_with(ResponseTemplate::new(200).set_body_string(ATOM))
                .mount(&server)
                .await;
            server
        });

        let mut results = read(&[feed(format!("{}/feed", server.uri()))]);
        let items = results.remove(0).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "Video");
        assert_eq!(items[0].guid.as_deref(), Some("yt:video:abc"));
        assert_eq!(items[0].tags, vec!["@youtube".to_string()]);
        let requests = rt.block_on(server.received_requests()).unwrap();
        assert_eq!(requests.len(), 4);
    }

    #[test]
    fn persistent_error_is_reported_with_status_after_all_retries() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let server = rt.block_on(async {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/feed"))
                .respond_with(ResponseTemplate::new(404).set_body_string("<html>404</html>"))
                .mount(&server)
                .await;
            server
        });

        let mut results = read(&[feed(format!("{}/feed", server.uri()))]);
        let error = format!("{:#}", results.remove(0).unwrap_err());
        assert!(error.contains("404 Not Found"), "{error}");
        let requests = rt.block_on(server.received_requests()).unwrap();
        assert_eq!(requests.len(), 1 + MAX_RETRIES as usize);
    }

    #[test]
    fn results_keep_feed_order_with_limited_concurrency() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let server = rt.block_on(async {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/ok"))
                .respond_with(ResponseTemplate::new(200).set_body_string(ATOM))
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path("/gone"))
                .respond_with(ResponseTemplate::new(410))
                .mount(&server)
                .await;
            server
        });

        // More feeds than MAX_CONCURRENT_FEEDS, every third one failing.
        let feeds: Vec<_> = (0..3 * MAX_CONCURRENT_FEEDS)
            .map(|i| {
                let p = if i % 3 == 2 { "gone" } else { "ok" };
                feed(format!("{}/{p}", server.uri()))
            })
            .collect();
        let results = read(&feeds);
        assert_eq!(results.len(), feeds.len());
        for (i, result) in results.iter().enumerate() {
            assert_eq!(result.is_err(), i % 3 == 2, "feed {i}");
        }
    }
}
