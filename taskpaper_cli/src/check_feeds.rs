use crate::CliConfig;
use anyhow::{anyhow, Context, Result};
use chrono::prelude::*;
use serde::{Deserialize, Serialize};
use soup::{NodeExt, QueryBuilderExt, Soup};
use std::collections::BTreeSet;
use std::fs;
use std::io;
use structopt::StructOpt;
use syndication::Feed;
use taskpaper::{sanitize_item_text, Database, Position};

const TASKPAPER_RSS_DONE_FILE: &str = ".taskpaper_rss_done.toml";

#[derive(Debug, Serialize, Deserialize, Copy, Clone)]
enum FeedPresentation {
    #[serde(rename = "feed")]
    FromFeed,

    #[serde(rename = "website")]
    FromWebsite,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FeedConfiguration {
    url: String,
    presentation: Option<FeedPresentation>,
    tags: Option<Vec<String>>,
}

#[derive(StructOpt, Debug)]
pub struct CommandLineArguments {}

pub fn run(db: &Database, _args: &CommandLineArguments, cli_config: &CliConfig) -> Result<()> {
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
        let client = reqwest::Client::builder().build()?;

        let feeds = read_feeds(&client, &cli_config.feeds, seen_ids_ref).await?;
        let mut rv = Vec::new();
        for (feed, feed_config) in feeds.into_iter().zip(&cli_config.feeds) {
            match feed {
                Ok(feed_items) => rv.extend(feed_items.into_iter()),
                Err(e) => rv.push(TaskItem {
                    title: format!("Could not fetch RSS for '{}'.", feed_config.url),
                    note_text: textwrap::wrap(&format!("{:?}", e), 80)
                        .into_iter()
                        .map(|l| l.to_string())
                        .collect(),
                    guid: None,
                    tags: Vec::new(),
                }),
            }
        }

        Ok(rv)
    });
    let result = result?;

    let mut inbox = db.parse_common_file(taskpaper::CommonFileKind::Inbox)?;

    let mut tags = taskpaper::Tags::new();
    tags.insert(taskpaper::Tag::new("reading".to_string(), None));

    for item in result {
        let mut text = sanitize_item_text(&item.title);
        for tag in item.tags {
            text.push(' ');
            text.extend(tag.chars());
        }
        let node_id = inbox.insert(
            taskpaper::Item::new_with_tags(taskpaper::ItemKind::Task, text, tags.clone()),
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
        None => DateTime::from_utc(naive_date, Utc),
    };
    Some(result)
}

pub fn get_summary_blocking(url: &str) -> Result<Option<TaskItem>> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let client = reqwest::Client::builder().build()?;
        Ok(get_summary(&client, url, None).await?)
    })
}

async fn get_page_body(client: &reqwest::Client, url: &str) -> Result<String> {
    Ok(client.get(url).send().await?.text().await?)
}

/// Turns a url into a TaskItem, suitable for use in the inbox.
async fn get_summary(
    client: &reqwest::Client,
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
    client: &reqwest::Client,
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
    client: &reqwest::Client,
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

    let rv = futures::future::join_all(futures).await;
    Ok(rv)
}
