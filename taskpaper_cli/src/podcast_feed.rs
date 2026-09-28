//! Builds podcast RSS feeds from the AI narration ("Listen to article") that Substack generates
//! for its posts.
//!
//! Substack's own RSS feed does not carry these mp3s, but its posts API (`/api/v1/posts`) lists
//! them under `audio_items` as plain, publicly downloadable S3 URLs. For every configured feed
//! that has a `podcast_file`, we fetch the newest posts, pick the narration (or the native podcast
//! episode, if the post is one) per post and write a podcast RSS feed that any podcast app can
//! subscribe to. The feed is regenerated from scratch on every run: it is a snapshot of the latest
//! posts, not an accumulating log, so it needs no state and heals itself.
//!
//! Paywalled posts report `audio_url: null`, so this only works for publications (or posts) that
//! are free to read.

use crate::check_feeds::FeedConfiguration;
use anyhow::{Context, Result, anyhow};
use chrono::DateTime;
use reqwest_middleware::ClientWithMiddleware;
use rss::extension::itunes::{ITunesChannelExtension, ITunesItemExtension};
use rss::{Channel, Enclosure, Guid, Item};
use serde::Deserialize;
use std::fs;
use std::path::Path;

/// Substack narrates posts as constant 48 kbps mp3, so the duration follows from the file size
/// without downloading the file.
const NARRATION_BITRATE_BITS_PER_SECOND: u64 = 48_000;

/// The posts API refuses page sizes above 50, which is also plenty for a podcast feed.
const MAX_POSTS: usize = 50;

#[derive(Debug, Deserialize)]
struct SubstackPost {
    id: u64,
    title: String,
    subtitle: Option<String>,
    description: Option<String>,
    canonical_url: String,
    /// RFC 3339, e.g. `2026-07-29T12:00:00.000Z`.
    post_date: String,
    /// Missing on some posts, hence the default.
    #[serde(default)]
    audio_items: Vec<SubstackAudioItem>,
    /// Set for native podcast episodes the author uploaded.
    podcast_url: Option<String>,
    podcast_duration: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct SubstackAudioItem {
    /// `null` when the post is paywalled or narration failed.
    audio_url: Option<String>,
    #[serde(rename = "type")]
    kind: String,
    /// `completed`, `paywalled` or `failed`.
    status: String,
}

/// One podcast episode before we know the size of its audio file.
#[derive(Debug, PartialEq)]
struct Episode {
    guid: String,
    title: String,
    link: String,
    description: String,
    /// RFC 2822, as required by RSS.
    pub_date: String,
    audio_url: String,
    /// Known for native podcast episodes. `None` for narrations, where it follows from the
    /// file size.
    duration_seconds: Option<u64>,
}

/// Writes one podcast feed per configured feed that has a `podcast_file`. Does nothing when
/// `podcast_dir` is `None`. Returns one error message per feed that could not be written; the
/// other feeds are unaffected.
pub async fn write_all(
    client: &ClientWithMiddleware,
    feeds: &[FeedConfiguration],
    podcast_dir: Option<&Path>,
) -> Vec<String> {
    let Some(podcast_dir) = podcast_dir else {
        return Vec::new();
    };
    let futures = feeds
        .iter()
        .filter_map(|feed| feed.podcast_file.as_ref().map(|file| (feed, file)))
        .map(|(feed, file)| async move {
            write_one(client, &feed.url, &podcast_dir.join(file))
                .await
                .map_err(|e| format!("{}: podcast feed: {:?}", feed.url, e))
        });
    futures::future::join_all(futures)
        .await
        .into_iter()
        .filter_map(Result::err)
        .collect()
}

async fn write_one(client: &ClientWithMiddleware, feed_url: &str, path: &Path) -> Result<()> {
    let source = fetch_source_channel(client, feed_url).await?;
    let posts = fetch_posts(client, feed_url).await?;
    let episodes = posts
        .iter()
        .filter_map(|post| episode_from_post(post).transpose())
        .collect::<Result<Vec<_>>>()?;

    let lengths = futures::future::join_all(
        episodes
            .iter()
            .map(|e| content_length(client, &e.audio_url)),
    )
    .await;
    let items = episodes
        .into_iter()
        .zip(lengths)
        .map(|(episode, length)| Ok(episode.into_item(length?)))
        .collect::<Result<Vec<_>>>()?;

    write_atomically(path, &build_channel(&source, items))
}

/// The publication's own RSS feed provides the channel metadata (title, description, artwork).
async fn fetch_source_channel(client: &ClientWithMiddleware, feed_url: &str) -> Result<Channel> {
    let body = client
        .get(feed_url)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    Channel::read_from(body.as_bytes())
        .with_context(|| format!("{feed_url} is not an RSS feed, podcast feeds need one"))
}

async fn fetch_posts(client: &ClientWithMiddleware, feed_url: &str) -> Result<Vec<SubstackPost>> {
    let url = posts_api_url(feed_url)?;
    let body = client
        .get(url.clone())
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    serde_json::from_str(&body).with_context(|| format!("Could not parse posts from {url}"))
}

/// Substack serves the same JSON API on every publication, also on custom domains, so the API
/// URL follows from the feed URL.
fn posts_api_url(feed_url: &str) -> Result<reqwest::Url> {
    let mut url = reqwest::Url::parse(feed_url)?;
    url.set_path("/api/v1/posts");
    url.set_query(Some(&format!("limit={MAX_POSTS}")));
    Ok(url)
}

/// Reads the header explicitly: `Response::content_length()` reports the (empty) body of a HEAD
/// response, not the header.
async fn content_length(client: &ClientWithMiddleware, url: &str) -> Result<u64> {
    let response = client.head(url).send().await?.error_for_status()?;
    response
        .headers()
        .get(reqwest::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok()?.parse().ok())
        .ok_or_else(|| anyhow!("No Content-Length for {url}"))
}

/// Picks the audio for a post: the native podcast episode if there is one, otherwise the completed
/// narration. `None` for posts without usable audio (paywalled, failed, plain text).
fn episode_from_post(post: &SubstackPost) -> Result<Option<Episode>> {
    let (audio_url, duration_seconds) = if let Some(url) = &post.podcast_url {
        (url.clone(), post.podcast_duration.map(|d| d.round() as u64))
    } else {
        let narration = post
            .audio_items
            .iter()
            .find(|a| a.kind == "tts" && a.status == "completed" && a.audio_url.is_some());
        match narration {
            Some(a) => (a.audio_url.clone().unwrap(), None),
            None => return Ok(None),
        }
    };
    let pub_date = DateTime::parse_from_rfc3339(&post.post_date)
        .with_context(|| format!("Bad post_date {:?} on post {}", post.post_date, post.id))?
        .to_rfc2822();
    Ok(Some(Episode {
        guid: format!("substack-post-{}", post.id),
        title: post.title.clone(),
        link: post.canonical_url.clone(),
        description: post
            .description
            .clone()
            .or_else(|| post.subtitle.clone())
            .unwrap_or_default(),
        pub_date,
        audio_url,
        duration_seconds,
    }))
}

impl Episode {
    fn into_item(self, length: u64) -> Item {
        let duration_seconds = self
            .duration_seconds
            .unwrap_or(length * 8 / NARRATION_BITRATE_BITS_PER_SECOND);

        let mut enclosure = Enclosure::default();
        enclosure.set_url(self.audio_url);
        enclosure.set_length(length.to_string());
        enclosure.set_mime_type("audio/mpeg");

        let mut guid = Guid::default();
        guid.set_value(self.guid);
        guid.set_permalink(false);

        let mut item = Item::default();
        item.set_title(self.title);
        item.set_link(self.link);
        item.set_description(self.description);
        item.set_pub_date(self.pub_date);
        item.set_guid(guid);
        item.set_enclosure(enclosure);
        item.set_itunes_ext(ITunesItemExtension {
            duration: Some(duration_seconds.to_string()),
            ..Default::default()
        });
        item
    }
}

fn build_channel(source: &Channel, items: Vec<Item>) -> Channel {
    let mut channel = Channel::default();
    channel.set_title(source.title());
    channel.set_link(source.link());
    channel.set_description(source.description());
    channel.set_language(source.language().map(str::to_string));
    channel.set_image(source.image().cloned());
    channel.set_itunes_ext(ITunesChannelExtension {
        image: source.image().map(|i| i.url().to_string()),
        summary: Some(source.description().to_string()),
        ..Default::default()
    });
    channel.set_items(items);
    channel
}

/// Writes next to the target and renames, so a web server never serves a half-written feed.
fn write_atomically(path: &Path, channel: &Channel) -> Result<()> {
    let tmp = path.with_extension("tmp");
    let bytes = channel.pretty_write_to(Vec::new(), b' ', 2)?;
    fs::write(&tmp, bytes).with_context(|| format!("Could not write {}", tmp.display()))?;
    fs::rename(&tmp, path)
        .with_context(|| format!("Could not move {} to {}", tmp.display(), path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const POSTS: &str = r#"[
      {"id": 1, "title": "Narrated", "subtitle": "Sub", "description": "Desc",
       "canonical_url": "https://example.io/p/narrated", "post_date": "2026-07-29T12:00:00.000Z",
       "audio_items": [{"post_id": 1, "voice_id": "v", "type": "tts", "status": "completed",
                        "audio_url": "https://s3.example/1.mp3"}],
       "podcast_url": null, "podcast_duration": null},
      {"id": 2, "title": "Native", "subtitle": null, "description": null,
       "canonical_url": "https://example.io/p/native", "post_date": "2026-07-28T12:00:00.000Z",
       "audio_items": [], "podcast_url": "https://api.example/2/src", "podcast_duration": 2359.09},
      {"id": 3, "title": "Paywalled", "subtitle": null, "description": null,
       "canonical_url": "https://example.io/p/paywalled", "post_date": "2026-07-27T12:00:00.000Z",
       "audio_items": [{"post_id": 3, "voice_id": "v", "type": "tts", "status": "paywalled",
                        "audio_url": null}],
       "podcast_url": null, "podcast_duration": null},
      {"id": 4, "title": "No audio field at all", "subtitle": null, "description": null,
       "canonical_url": "https://example.io/p/none", "post_date": "2026-07-26T12:00:00.000Z",
       "podcast_url": null, "podcast_duration": null}
    ]"#;

    fn episodes() -> Vec<Episode> {
        let posts: Vec<SubstackPost> = serde_json::from_str(POSTS).unwrap();
        posts
            .iter()
            .filter_map(|p| episode_from_post(p).unwrap())
            .collect()
    }

    #[test]
    fn posts_api_url_follows_from_feed_url() {
        assert_eq!(
            posts_api_url("https://currentaffairs.io/feed")
                .unwrap()
                .as_str(),
            "https://currentaffairs.io/api/v1/posts?limit=50"
        );
        assert_eq!(
            posts_api_url("https://charitydotwtf.substack.com/feed?x=1")
                .unwrap()
                .as_str(),
            "https://charitydotwtf.substack.com/api/v1/posts?limit=50"
        );
    }

    #[test]
    fn selects_narrations_and_native_episodes_only() {
        let episodes = episodes();
        assert_eq!(
            episodes,
            vec![
                Episode {
                    guid: "substack-post-1".into(),
                    title: "Narrated".into(),
                    link: "https://example.io/p/narrated".into(),
                    description: "Desc".into(),
                    pub_date: "Wed, 29 Jul 2026 12:00:00 +0000".into(),
                    audio_url: "https://s3.example/1.mp3".into(),
                    duration_seconds: None,
                },
                Episode {
                    guid: "substack-post-2".into(),
                    title: "Native".into(),
                    link: "https://example.io/p/native".into(),
                    description: "".into(),
                    pub_date: "Tue, 28 Jul 2026 12:00:00 +0000".into(),
                    audio_url: "https://api.example/2/src".into(),
                    duration_seconds: Some(2359),
                },
            ]
        );
    }

    #[test]
    fn writes_podcast_feed_with_enclosures_and_durations() {
        let source = Channel::read_from(
            r#"<rss version="2.0"><channel><title>Pub</title><link>https://example.io</link>
               <description>About</description><language>en</language>
               <image><url>https://example.io/logo.png</url><title>Pub</title>
               <link>https://example.io</link></image></channel></rss>"#
                .as_bytes(),
        )
        .unwrap();
        let items = episodes()
            .into_iter()
            .map(|e| e.into_item(10_250_640))
            .collect();
        let xml = build_channel(&source, items).to_string();

        assert!(xml.contains(r#"xmlns:itunes="http://www.itunes.com/dtds/podcast-1.0.dtd""#));
        assert!(xml.contains("<title>Pub</title>"));
        assert!(xml.contains(r#"<itunes:image href="https://example.io/logo.png"/>"#));
        assert!(xml.contains(
            r#"<enclosure url="https://s3.example/1.mp3" length="10250640" type="audio/mpeg"/>"#
        ));
        assert!(xml.contains(r#"<guid isPermaLink="false">substack-post-1</guid>"#));
        // 10250640 bytes at 48 kbps.
        assert!(xml.contains("<itunes:duration>1708</itunes:duration>"));
        // The native episode keeps the duration Substack reports.
        assert!(xml.contains("<itunes:duration>2359</itunes:duration>"));
    }
}
