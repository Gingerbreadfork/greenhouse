//! RSS and Atom feeds whose new items are added automatically.

use std::collections::HashMap;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::{Manager, Runtime};

use crate::commands::{queue_with_defaults, AppState};
use crate::torrentsrc::looks_like_source;

/// Seen items kept per feed, so a long-lived feed does not grow without end.
const SEEN_LIMIT: usize = 5000;

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Feed {
    pub id: String,
    pub name: String,
    pub url: String,
    /// Whether new matching items are added on their own.
    pub enabled: bool,
    /// Comma-separated words a title must all contain. Empty matches anything.
    pub must_contain: String,
    /// Comma-separated words that rule a title out.
    pub must_not_contain: String,
    /// Where this feed's torrents are saved. Empty means the usual folder.
    pub download_dir: String,
}

#[derive(Debug, PartialEq, Clone)]
pub struct FeedItem {
    pub guid: String,
    pub title: String,
    /// A magnet link or torrent URL.
    pub link: String,
    pub published: Option<String>,
}

fn child_text<'a>(node: roxmltree::Node<'a, '_>, name: &str) -> Option<&'a str> {
    node.children()
        .find(|c| c.is_element() && c.tag_name().name().eq_ignore_ascii_case(name))
        .and_then(|c| c.text())
        .map(str::trim)
        .filter(|t| !t.is_empty())
}

/// The torrent an item points at: a magnet field, then an attachment, then
/// the item's own link.
fn item_link(node: roxmltree::Node) -> Option<String> {
    let elements = || node.children().filter(|c| c.is_element());
    let magnet = child_text(node, "magnetURI");
    let attached = elements()
        .filter(|c| {
            let name = c.tag_name().name();
            name == "enclosure" || (name == "link" && c.attribute("rel") == Some("enclosure"))
        })
        .find_map(|c| c.attribute("url").or_else(|| c.attribute("href")));
    let plain = elements()
        .filter(|c| c.tag_name().name() == "link")
        .find_map(|c| c.attribute("href").or_else(|| c.text()).map(str::trim));
    [magnet, attached, plain, child_text(node, "guid")]
        .into_iter()
        .flatten()
        .find(|candidate| looks_like_source(candidate))
        .map(str::to_string)
}

/// Reads the items of an RSS or Atom document, skipping any with no torrent.
pub fn parse_feed(xml: &str) -> Result<Vec<FeedItem>, String> {
    let options = roxmltree::ParsingOptions {
        allow_dtd: true,
        ..Default::default()
    };
    let doc = roxmltree::Document::parse_with_options(xml, options)
        .map_err(|e| format!("that is not a feed: {e}"))?;
    let items = doc
        .descendants()
        .filter(|n| n.is_element() && matches!(n.tag_name().name(), "item" | "entry"))
        .filter_map(|node| {
            let link = item_link(node)?;
            let title = child_text(node, "title").unwrap_or(&link).to_string();
            let guid = child_text(node, "guid")
                .or_else(|| child_text(node, "id"))
                .unwrap_or(&link)
                .to_string();
            let published = ["pubDate", "published", "updated", "date"]
                .iter()
                .find_map(|name| child_text(node, name))
                .map(str::to_string);
            Some(FeedItem {
                guid,
                title,
                link,
                published,
            })
        })
        .collect();
    Ok(items)
}

fn terms(list: &str) -> Vec<String> {
    list.split(',')
        .map(|t| t.trim().to_lowercase())
        .filter(|t| !t.is_empty())
        .collect()
}

/// Whether a title passes a feed's filters.
pub fn matches(feed: &Feed, title: &str) -> bool {
    let title = title.to_lowercase();
    terms(&feed.must_contain).iter().all(|t| title.contains(t))
        && !terms(&feed.must_not_contain).iter().any(|t| title.contains(t))
}

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
struct Seen {
    added: bool,
    /// Seconds since 1970.
    at: u64,
}

#[derive(Serialize, Clone)]
pub struct FeedItemView {
    pub guid: String,
    pub title: String,
    pub link: String,
    pub published: Option<String>,
    pub matches: bool,
    pub added: bool,
}

#[derive(Serialize, Clone, Default)]
pub struct FeedStatus {
    /// Seconds since 1970.
    pub checked_at: Option<u64>,
    pub error: Option<String>,
    pub items: Vec<FeedItemView>,
}

#[derive(Default)]
pub struct FeedsState {
    /// Items already dealt with, by feed and then by item id.
    seen: HashMap<String, HashMap<String, Seen>>,
    status: HashMap<String, FeedStatus>,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

impl FeedsState {
    pub fn load(file: &Path) -> Self {
        let seen = std::fs::read_to_string(file)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        Self {
            seen,
            status: HashMap::new(),
        }
    }

    fn save(&self, file: &Path) {
        if let Ok(text) = serde_json::to_string(&self.seen) {
            let _ = std::fs::write(file, text);
        }
    }

    pub fn statuses(&self) -> HashMap<String, FeedStatus> {
        self.status.clone()
    }

    /// Records a fetch and returns the items to add. The first fetch of a feed
    /// only takes note of what is there, so its backlog is never downloaded.
    fn absorb(&mut self, feed: &Feed, items: &[FeedItem]) -> Vec<FeedItem> {
        let first_fetch = !self.seen.contains_key(&feed.id);
        let seen = self.seen.entry(feed.id.clone()).or_default();
        let at = now();
        let mut to_add = Vec::new();
        for item in items {
            if seen.contains_key(&item.guid) {
                continue;
            }
            let add = !first_fetch && feed.enabled && matches(feed, &item.title);
            if add {
                to_add.push(item.clone());
            }
            seen.insert(item.guid.clone(), Seen { added: add, at });
        }
        if seen.len() > SEEN_LIMIT {
            let mut ages: Vec<u64> = seen.values().map(|s| s.at).collect();
            ages.sort_unstable();
            let cutoff = ages[seen.len() - SEEN_LIMIT];
            seen.retain(|_, s| s.at >= cutoff);
        }

        let views = items
            .iter()
            .map(|item| FeedItemView {
                guid: item.guid.clone(),
                title: item.title.clone(),
                link: item.link.clone(),
                published: item.published.clone(),
                matches: matches(feed, &item.title),
                added: seen.get(&item.guid).is_some_and(|s| s.added),
            })
            .collect();
        self.status.insert(
            feed.id.clone(),
            FeedStatus {
                checked_at: Some(at),
                error: None,
                items: views,
            },
        );
        to_add
    }

    fn fail(&mut self, feed: &Feed, error: String) {
        let status = self.status.entry(feed.id.clone()).or_default();
        status.checked_at = Some(now());
        status.error = Some(error);
    }

    /// Marks an item as added and hands back what to add.
    fn take_item(&mut self, feed_id: &str, guid: &str) -> Option<FeedItemView> {
        let item = self
            .status
            .get_mut(feed_id)?
            .items
            .iter_mut()
            .find(|i| i.guid == guid)?;
        item.added = true;
        let seen = self.seen.entry(feed_id.to_string()).or_default();
        seen.insert(guid.to_string(), Seen { added: true, at: now() });
        Some(item.clone())
    }

    /// Drops what is kept for feeds that no longer exist.
    fn retain(&mut self, feeds: &[Feed]) {
        let known = |id: &String| feeds.iter().any(|f| &f.id == id);
        self.seen.retain(|id, _| known(id));
        self.status.retain(|id, _| known(id));
    }
}

fn download_dir(feed: &Feed) -> Option<String> {
    let dir = feed.download_dir.trim();
    (!dir.is_empty()).then(|| dir.to_string())
}

/// Fetches a feed and queues whatever is new and wanted.
pub async fn check<R: Runtime>(app: &tauri::AppHandle<R>, feed: &Feed) -> FeedStatus {
    let state = app.state::<AppState>();
    let fetched = async {
        let response = state.http.get(&feed.url).send().await.map_err(|e| e.to_string())?;
        if !response.status().is_success() {
            return Err(format!("the server replied {}", response.status()));
        }
        parse_feed(&response.text().await.map_err(|e| e.to_string())?)
    }
    .await;

    let to_add = {
        let mut feeds = state.feeds.lock();
        match fetched {
            Ok(items) => {
                let to_add = feeds.absorb(feed, &items);
                feeds.save(&state.paths.feeds_file());
                to_add
            }
            Err(error) => {
                feeds.fail(feed, error);
                Vec::new()
            }
        }
    };
    for item in to_add {
        let origin = Some(format!("Added from the feed {}", feed.name));
        let _ = queue_with_defaults(app, item.link, Some(item.title), download_dir(feed), origin);
    }
    let status = state.feeds.lock().status.get(&feed.id).cloned();
    status.unwrap_or_default()
}

/// Adds one listed item by hand.
pub fn add_item<R: Runtime>(
    app: &tauri::AppHandle<R>,
    feed: &Feed,
    guid: &str,
) -> Result<(), String> {
    let state = app.state::<AppState>();
    let item = {
        let mut feeds = state.feeds.lock();
        let item = feeds
            .take_item(&feed.id, guid)
            .ok_or("that item is no longer in the feed")?;
        feeds.save(&state.paths.feeds_file());
        item
    };
    let origin = Some(format!("Picked from the feed {}", feed.name));
    queue_with_defaults(app, item.link, Some(item.title), download_dir(feed), origin)
}

/// Checks each enabled feed when its turn comes round.
pub fn spawn<R: Runtime>(app: tauri::AppHandle<R>) {
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        loop {
            interval.tick().await;
            let state = app.state::<AppState>();
            let (feeds, every) = {
                let settings = state.settings.read();
                let minutes = u64::from(settings.feed_interval_minutes.max(5));
                (settings.feeds.clone(), minutes * 60)
            };
            let due: Vec<Feed> = {
                let mut kept = state.feeds.lock();
                kept.retain(&feeds);
                feeds
                    .into_iter()
                    .filter(|f| f.enabled && !f.url.trim().is_empty())
                    .filter(|f| {
                        let last = kept.status.get(&f.id).and_then(|s| s.checked_at);
                        last.is_none_or(|at| now().saturating_sub(at) >= every)
                    })
                    .collect()
            };
            for feed in due {
                check(&app, &feed).await;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const RSS: &str = r#"<?xml version="1.0"?>
      <rss version="2.0" xmlns:torrent="http://xmlns.ezrss.it/0.1/"><channel>
        <title>Releases</title>
        <item>
          <title>Show S01E02 1080p</title>
          <link>https://site.example/details/2</link>
          <guid isPermaLink="false">ep-2</guid>
          <pubDate>Mon, 21 Sep 2026 10:00:00 GMT</pubDate>
          <enclosure url="https://site.example/dl/2.torrent" type="application/x-bittorrent"/>
        </item>
        <item>
          <title><![CDATA[Show S01E01 720p]]></title>
          <torrent:magnetURI><![CDATA[magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef01234567]]></torrent:magnetURI>
          <guid>ep-1</guid>
        </item>
        <item><title>Site news, no torrent</title><guid>news</guid></item>
      </channel></rss>"#;

    const ATOM: &str = r#"<feed xmlns="http://www.w3.org/2005/Atom">
        <entry>
          <title>Distro 13.1 netinst</title>
          <id>urn:distro:13.1</id>
          <updated>2026-09-21T10:00:00Z</updated>
          <link rel="alternate" href="https://distro.example/13.1"/>
          <link rel="enclosure" href="https://distro.example/13.1.iso.torrent"/>
        </entry>
      </feed>"#;

    fn feed(must: &str, must_not: &str) -> Feed {
        Feed {
            id: "f".into(),
            enabled: true,
            must_contain: must.into(),
            must_not_contain: must_not.into(),
            ..Default::default()
        }
    }

    #[test]
    fn reads_rss_items_and_prefers_the_torrent_over_the_page() {
        let items = parse_feed(RSS).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].title, "Show S01E02 1080p");
        assert_eq!(items[0].link, "https://site.example/dl/2.torrent");
        assert_eq!(items[0].guid, "ep-2");
        assert!(items[0].published.as_deref().unwrap().starts_with("Mon, 21 Sep"));
        assert!(items[1].link.starts_with("magnet:?xt=urn:btih:0123"));
    }

    #[test]
    fn reads_atom_entries() {
        let items = parse_feed(ATOM).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].link, "https://distro.example/13.1.iso.torrent");
        assert_eq!(items[0].guid, "urn:distro:13.1");
        assert!(parse_feed("<html>not a feed").is_err());
    }

    #[test]
    fn filters_match_every_wanted_word_and_no_unwanted_one() {
        assert!(matches(&feed("", ""), "Anything at all"));
        assert!(matches(&feed("show, 1080p", ""), "Show S01E02 1080p"));
        assert!(!matches(&feed("show, 1080p", ""), "Show S01E01 720p"));
        assert!(!matches(&feed("show", "720p, cam"), "Show S01E01 720p"));
    }

    #[test]
    fn a_new_feeds_backlog_is_noted_but_not_downloaded() {
        let mut state = FeedsState::default();
        let wanted = feed("1080p", "");
        let mut items = parse_feed(RSS).unwrap();

        assert!(state.absorb(&wanted, &items).is_empty());

        items.insert(
            0,
            FeedItem {
                guid: "ep-3".into(),
                title: "Show S01E03 1080p".into(),
                link: "https://site.example/dl/3.torrent".into(),
                published: None,
            },
        );
        items.insert(
            0,
            FeedItem {
                guid: "ep-3-low".into(),
                title: "Show S01E03 480p".into(),
                link: "https://site.example/dl/3b.torrent".into(),
                published: None,
            },
        );
        let added = state.absorb(&wanted, &items);
        assert_eq!(added.len(), 1);
        assert_eq!(added[0].guid, "ep-3");
        // Seen once, never again.
        assert!(state.absorb(&wanted, &items).is_empty());

        let views = &state.status["f"].items;
        assert!(views.iter().find(|v| v.guid == "ep-3").unwrap().added);
        assert!(!views.iter().find(|v| v.guid == "ep-2").unwrap().added);
    }

    /// Uses the network: a real release feed parses into torrent links.
    #[tokio::test]
    #[ignore]
    async fn a_real_feed_parses() {
        let xml = reqwest::get("https://archlinux.org/feeds/releases/")
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        let items = parse_feed(&xml).unwrap();
        assert!(!items.is_empty());
        assert!(items.iter().all(|i| looks_like_source(&i.link)), "{items:?}");
    }

    #[test]
    fn a_switched_off_feed_adds_nothing() {
        let mut state = FeedsState::default();
        let mut off = feed("", "");
        off.enabled = false;
        state.absorb(&off, &[]);
        assert!(state.absorb(&off, &parse_feed(RSS).unwrap()).is_empty());
    }
}
