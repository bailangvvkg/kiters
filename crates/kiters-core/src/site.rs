//! The site vocabulary: kinds, statuses, front matter, content, targets,
//! plans and the pages they become.

use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A kind of content: `posts` today, whatever a site invents tomorrow.
///
/// It is a newtype rather than an enum so a site may add a kind without a
/// recompile — the vocabulary of the plan is the site's, not ours.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Kind(pub String);

impl Kind {
    pub const POST: &'static str = "post";
    pub const PAGE: &'static str = "page";
}

impl Kind {
    pub fn new(name: impl Into<String>) -> Self {
        Kind(name.into())
    }
    /// The directory a kind lives under, plural the way the tree is.
    pub fn dir(&self) -> &str {
        match self.0.as_str() {
            Kind::POST => "posts",
            Kind::PAGE => "pages",
            other => other,
        }
    }
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A status decides who may see an item: a draft is the author's alone, a
/// page out is the world's.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Draft,
    Scheduled,
    #[default]
    Published,
    Archived,
}

impl Status {
    /// Whether the item stands in the public site. A published item
    /// always does; a scheduled one does the moment its date has come;
    /// a draft never does. `dated` is the item's own instant, `now` the
    /// judge.
    pub fn stands(self, dated: DateTime<Utc>, now: DateTime<Utc>) -> bool {
        match self {
            Status::Published => true,
            Status::Scheduled => dated <= now,
            _ => false,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Draft => "draft",
            Status::Scheduled => "scheduled",
            Status::Published => "published",
            Status::Archived => "archived",
        }
    }
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The front matter of an item: everything about it that is not its prose.
///
/// `extra` carries the fields we have no name for, so a site may invent
/// metadata without us standing in the way.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct FrontMatter {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub slug: Option<String>,
    #[serde(default)]
    pub status: Status,
    /// Dates in front matter are their own format's problem; here they
    /// are already instants.
    #[serde(default, deserialize_with = "de_date")]
    pub date: Option<DateTime<Utc>>,
    #[serde(default)]
    pub cover: Option<String>,
    #[serde(default)]
    pub taxonomies: BTreeMap<String, Vec<String>>,
    /// Addresses this item has answered under before. Each one is a page
    /// that says where the item lives now.
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub kind: Option<Kind>,
    #[serde(flatten)]
    pub extra: serde_yaml::Mapping,
}

/// A date as front matter says it: RFC 3339 whole, or a plain
/// `2026-01-01` that means the midnight it starts at. Both are a
/// site's habit; a date neither of them is, is not.
fn de_date<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<DateTime<Utc>>, D::Error> {
    let Some(raw) = Option::<String>::deserialize(d)? else { return Ok(None) };
    let raw = raw.trim();
    if let Ok(whole) = DateTime::parse_from_rfc3339(raw) {
        return Ok(Some(whole.with_timezone(&Utc)));
    }
    if let Ok(day) = chrono::NaiveDate::parse_from_str(raw, "%Y-%m-%d") {
        return day
            .and_hms_opt(0, 0, 0)
            .map(|midnight| DateTime::from_naive_utc_and_offset(midnight, chrono::Utc))
            .map(Some)
            .ok_or_else(|| serde::de::Error::custom("a date with no midnight"));
    }
    Err(serde::de::Error::custom(format!("date, not as one is read: {raw}")))
}

/// An item of content: the front matter and the prose, and where both
/// came from.
#[derive(Clone, Debug)]
pub struct Content {
    pub kind: Kind,
    pub slug: String,
    pub front: FrontMatter,
    /// The prose, markdown, exactly as written.
    pub body: String,
    /// The file the item was read from, for messages a human reads.
    pub source: PathBuf,
}

impl Content {
    /// The address the item is served under: a kind's directory, the
    /// slug, a trailing slash — the shape the whole site speaks.
    pub fn url(&self) -> String {
        format!("/{}/{}/", self.kind.dir(), self.slug)
    }

    pub fn status(&self) -> Status {
        self.front.status
    }

    /// The instant an item is ordered and dated by: its own date if it
    /// has one, the moment of reading otherwise. A plan wants one answer.
    pub fn dated(&self, now: DateTime<Utc>) -> DateTime<Utc> {
        self.front.date.unwrap_or(now)
    }
}

/// What a target renders as.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TargetKind {
    /// A page of one item.
    Item,
    /// The home page, which belongs to every item at once.
    Home,
    /// A page the site answers when it has nothing else: rendered, not a
    /// bare 404, so the lost reader lands softly.
    NotFound,
    /// A feed, a sitemap — a whole site said in one file.
    Extra(&'static str),
    /// An address an item answered under before, now saying where it
    /// lives. Served as a page, not a redirect: a page carries the new
    /// address in it, and readers without JavaScript still land.
    Alias { to: String },
    /// The index of one taxonomy: every term it has, each a link.
    Taxonomy(String),
    /// One term of one taxonomy: every public item carrying it.
    Term { taxonomy: String, term: String },
}

/// A target is one address the finished site answers: the kind of page it
/// is, the address, where the bytes land, and the item when there is one.
#[derive(Clone, Debug)]
pub struct Target {
    pub kind: TargetKind,
    pub url: String,
    /// Where under the output the target's bytes land.
    pub out: PathBuf,
    /// The item a target renders, for the targets that render one.
    pub content: Option<Content>,
}

impl Target {
    /// The file path an address lands at: an address ending in `/` is a
    /// directory with an `index.html` in it, the way a static site is
    /// laid out for any server to read.
    pub fn address_to_path(url: &str) -> PathBuf {
        let trimmed = url.trim_start_matches('/');
        let mut path = PathBuf::new();
        if trimmed.is_empty() {
            path.push("index.html");
        } else if url.ends_with('/') {
            path.push(trimmed);
            path.push("index.html");
        } else {
            path.push(trimmed);
        }
        path
    }
}

/// What the site is called and where it lives: the few facts every page
/// and feed wants at hand.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SiteMeta {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub language: String,
    #[serde(default, alias = "baseURL")]
    pub base_url: String,
}

/// A plan is every target of one build, in the order they will be
/// rendered: the items dated newest first, then the site's own pages.
#[derive(Clone, Debug, Default)]
pub struct Plan {
    pub targets: Vec<Target>,
}

/// One finished page: the address and the bytes. What pre-rendering makes
/// is a bag of these, complete before the first reader arrives.
#[derive(Clone, Debug)]
pub struct Page {
    pub url: String,
    pub status: u16,
    pub html: String,
}

/// How a build went, in numbers a human reads.
#[derive(Clone, Debug, Default)]
pub struct BuildReport {
    pub items: usize,
    pub drafts: usize,
    pub rendered: usize,
    pub written: usize,
    pub took: std::time::Duration,
}
