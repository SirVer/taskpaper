use crate::ConfigurationFile;
use chrono::prelude::*;
use serde::{Deserialize, Serialize};
use soup::{NodeExt, QueryBuilderExt, Soup};
use std::collections::HashSet;
use std::fs;
use std::io;
use structopt::StructOpt;
use syndication::Feed;
use taskpaper::{Database, Error, Result};

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
}

#[derive(StructOpt, Debug)]
pub struct CommandLineArguments {
    /// Style to format with. The default is 'inbox'.
    #[structopt(short = "-s", long = "--style", default_value = "inbox")]
    style: String,
}

pub fn run(db: &Database, args: &CommandLineArguments, config: &ConfigurationFile) -> Result<()> {
    let rt = tokio::runtime::Runtime::new()?;
    let result: Result<Vec<TaskEntry>> = rt.block_on(async {
        let client = reqwest::Client::builder()
            .redirect(reqwest::RedirectPolicy::limited(10))
            .build()?;

        let feeds = read_feeds(&client, &config.feeds).await?;
        let mut rv = Vec::new();
        for (feed, feed_config) in feeds.into_iter().zip(&config.feeds) {
            match feed {
                Ok(feed_entries) => rv.extend(feed_entries.into_iter()),
                Err(e) => rv.push(TaskEntry {
                    title: format!("Could not fetch RSS for '{}'.", feed_config.url),
                    note_text: textwrap::wrap(&format!("{:?}", e), 80)
                        .into_iter()
                        .map(|l| l.to_string())
                        .collect(),
                }),
            }
        }

        Ok(rv)
    });

    let style = match config.formats.get(&args.style) {
        Some(format) => *format,
        None => return Err(Error::misc(format!("Style '{}' not found.", args.style))),
    };

    let mut tags = taskpaper::Tags::new();
    tags.insert(taskpaper::Tag::new("reading".to_string(), None));

    let mut inbox = db.parse_common_file(taskpaper::CommonFileKind::Inbox)?;
    for entry in result? {
        inbox.push_back(taskpaper::Entry::Task(taskpaper::Task {
            line_index: None,
            text: entry.title,
            tags: tags.clone(),
            note: Some(entry.note_text.join("\n")),
        }))
    }
    db.overwrite_common_file(&inbox, taskpaper::CommonFileKind::Inbox, style)?;

    Ok(())
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct SeenIds {
    seen_ids: HashSet<String>,
}

/// Broken down information for tasks.
#[derive(Debug)]
pub struct TaskEntry {
    pub title: String,
    pub note_text: Vec<String>,
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

pub fn get_summary_blocking(url: &str) -> Result<Option<TaskEntry>> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let client = reqwest::Client::builder()
            .redirect(reqwest::RedirectPolicy::limited(10))
            .build()?;
        Ok(get_summary(&client, url).await?)
    })
}

async fn get_page_body(client: &reqwest::Client, url: &str) -> Result<String> {
    Ok(client.get(url).send().await?.text().await?)
}

/// Turns a url into a TaskEntry, suitable for use in the inbox.
async fn get_summary(client: &reqwest::Client, url: &str) -> Result<Option<TaskEntry>> {
    let text = get_page_body(client, url).await?;
    let soup = Soup::new(&text);

    let mut title_text_lines = Vec::new();
    // Find and push the title.
    soup.tag("title").find().map(|node| {
        let text = node.text().trim().to_string();
        if !text.is_empty() {
            title_text_lines.push(text);
        }
    });
    let mut extra_notes = Vec::new();
    // Find and push the description.
    soup.tag("meta")
        .attr("name", "description")
        .find()
        .map(|node| {
            node.attrs().get("content").map(|t| match t.len() {
                0 => (),
                1..=100 => title_text_lines.push(t.to_string()),
                _ => {
                    extra_notes.extend(textwrap::wrap(t, 80).into_iter().map(|l| l.to_string()));
                }
            });
        });
    if title_text_lines.is_empty() {
        return Ok(None);
    }
    let mut note_text = Vec::new();
    note_text.push(url.to_string());
    note_text.extend(extra_notes.into_iter());
    Ok(Some(TaskEntry {
        title: title_text_lines.join(" • "),
        note_text,
    }))
}

async fn get_summary_or_current_information(
    client: &reqwest::Client,
    feed_presentation: FeedPresentation,
    url: &str,
    title: String,
    content: String,
    published: Option<DateTime<Utc>>,
) -> Result<TaskEntry> {
    let task = match feed_presentation {
        FeedPresentation::FromWebsite => get_summary(client, url)
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
            TaskEntry { title, note_text }
        }
    };
    Ok(task)
}
/// Returns a vector of same length then feeds, which contains either an Err if the feed could not
/// be read or a list of entries that we did not see before on any prior run.
async fn read_feeds(
    client: &reqwest::Client,
    feeds: &[FeedConfiguration],
) -> Result<Vec<Result<Vec<TaskEntry>>>> {
    let home = dirs::home_dir().expect("HOME not set.");
    let archive = home.join(TASKPAPER_RSS_DONE_FILE);
    let seen_ids = match fs::read_to_string(&archive) {
        Ok(data) => toml::from_str(&data)
            .map_err(|_| Error::misc(format!("Could not parse {}", archive.display())))?,
        Err(e) if e.kind() == io::ErrorKind::NotFound => SeenIds::default(),
        Err(e) => return Err(e.into()),
    };

    let seen_ids = ::std::sync::Mutex::new(seen_ids);

    let mut futures = Vec::new();
    let seen_ids_ref = &seen_ids;
    for feed in feeds {
        let presentation = feed.presentation.unwrap_or(FeedPresentation::FromWebsite);

        futures.push(async move {
            let body = get_page_body(client, &feed.url).await?;
            let mut entries = Vec::new();
            match body
                .parse::<Feed>()
                .map_err(|e| Error::misc(format!("Could not parse for {}: {}", feed.url, e)))?
            {
                Feed::RSS(channel) => {
                    for entry in channel.items() {
                        let url = entry.link();
                        if url.is_none() {
                            continue;
                        }
                        let published = parse_date(entry.pub_date());
                        let content = entry.content().or(entry.description()).unwrap_or("");
                        let guid = entry
                            .guid()
                            .map(|g| g.value())
                            .unwrap_or(url.unwrap())
                            .to_string();
                        {
                            let seen_ids = seen_ids_ref.lock().unwrap();
                            if seen_ids.seen_ids.contains(&guid) {
                                continue;
                            }
                        }

                        let title = entry
                            .title()
                            .unwrap_or_else(|| "No Title")
                            .trim()
                            .to_string();
                        let task = get_summary_or_current_information(
                            client,
                            presentation,
                            url.unwrap(),
                            title,
                            content.to_string(),
                            published,
                        )
                        .await?;

                        {
                            let mut seen_ids = seen_ids_ref.lock().unwrap();
                            seen_ids.seen_ids.insert(guid);
                        }
                        entries.push(task);
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
                                .or(entry.summary())
                                .unwrap_or("")
                        };
                        let guid = entry.id().to_string();
                        {
                            let seen_ids = seen_ids_ref.lock().unwrap();
                            if seen_ids.seen_ids.contains(&guid) {
                                continue;
                            }
                        }

                        let published = parse_date(entry.published());
                        let title = entry.title().trim().to_string();
                        let task = get_summary_or_current_information(
                            client,
                            presentation,
                            urls.first().unwrap(),
                            title,
                            content.to_string(),
                            published,
                        )
                        .await?;

                        {
                            let mut seen_ids = seen_ids_ref.lock().unwrap();
                            seen_ids.seen_ids.insert(guid);
                        }
                        entries.push(task);
                    }
                }
            };
            let rv: Result<Vec<TaskEntry>> = Ok(entries);
            rv
        })
    }

    let rv = futures::future::join_all(futures).await;
    std::fs::write(
        &archive,
        toml::to_string_pretty(&seen_ids.into_inner().unwrap()).unwrap(),
    )?;
    Ok(rv)
}
