//! The content store: the file tree is the only truth.
//!
//! There is no database. An item is `content/<kind>/<slug>/index.md`,
//! addressed as `<kind>:<slug>`; its revision is a hash of its own
//! bytes, so two writers cannot both believe they won — the second to
//! write finds the first's revision in the way and answers 409.

use std::path::{Path, PathBuf};

use kiters_core::FrontMatter;
use serde::{Deserialize, Serialize};

/// One item, as the API speaks it.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Entry {
    pub id: String,
    pub kind: String,
    pub slug: String,
    pub title: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cover: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    /// The item's own bytes, hashed: what If-Match must say to write.
    pub revision: String,
    /// The front matter, as written, for an editor to take apart.
    pub raw_front_matter: String,
}

/// What a write ran into.
pub enum WriteError {
    /// The revision If-Match named is not the item's now: someone wrote
    /// first. The item's current revision rides along.
    Conflict(String),
    /// The address is taken (create) or gone (update, delete).
    Exists,
    Missing,
    /// A name the tree will not hold.
    BadName(String),
    Io(std::io::Error),
}

impl From<std::io::Error> for WriteError {
    fn from(err: std::io::Error) -> Self {
        WriteError::Io(err)
    }
}

/// The store of a site's content, rooted at the site's root.
pub struct ContentStore {
    root: PathBuf,
}

impl ContentStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        ContentStore { root: root.into() }
    }

    /// The site's root, for whoever walks it.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// An id (`kind:slug`) split, or refused.
    pub fn split_id(id: &str) -> Result<(String, String), WriteError> {
        let Some((kind, slug)) = id.split_once(':') else {
            return Err(WriteError::BadName(id.to_string()));
        };
        check_name(&kind)?;
        check_name(&slug)?;
        Ok((kind.to_string(), slug.to_string()))
    }

    fn file(&self, kind: &str, slug: &str) -> PathBuf {
        self.root.join("content").join(kind).join(slug).join("index.md")
    }

    /// Every item under a kind (or every kind), newest first, in pages:
    /// the cursor is the last id of the page before, and pages hold
    /// `limit` at most. `next` says whether there is more.
    pub fn list(&self, kind: Option<&str>, cursor: Option<&str>, limit: usize) -> anyhow::Result<(Vec<Entry>, Option<String>)> {
        let mut ids = Vec::new();
        let content = self.root.join("content");
        if !content.is_dir() {
            return Ok((Vec::new(), None));
        }
        for kind_dir in walk_kinds(&content) {
            if let Some(want) = kind {
                if kind_dir != want {
                    continue;
                }
            }
            let dir = content.join(&kind_dir);
            for slug in walk_slugs(&dir)? {
                ids.push(format!("{kind_dir}:{slug}"));
            }
        }
        ids.sort();
        let mut started = cursor.is_none();
        let mut page = Vec::new();
        let mut next = None;
        for id in &ids {
            if !started {
                started = id == cursor.unwrap_or("");
                continue;
            }
            if page.len() == limit {
                next = Some(id.clone());
                break;
            }
            if let Some(entry) = self.read(id)? {
                page.push(entry);
            }
        }
        Ok((page, next))
    }

    /// One item, whole: its entry, its front matter parsed, its prose.
    pub fn read(&self, id: &str) -> anyhow::Result<Option<Entry>> {
        let (kind, slug) = match Self::split_id(id) {
            Ok(split) => split,
            Err(_) => return Ok(None),
        };
        let path = self.file(&kind, &slug);
        let raw = match std::fs::read_to_string(&path) {
            Ok(raw) => raw,
            Err(_) => return Ok(None),
        };
        let (front, _body) = kiters_parse::split_front_matter(&raw);
        Ok(Some(entry_of(id, &kind, &slug, &front, &raw, &path)))
    }

    /// One item's full text, exactly as on disk.
    pub fn read_raw(&self, id: &str) -> Option<(String, String)> {
        let (kind, slug) = Self::split_id(id).ok()?;
        let raw = std::fs::read_to_string(self.file(&kind, &slug)).ok()?;
        let revision = revision_of(&raw);
        Some((raw, revision))
    }

    /// Writes an item whole: front matter serialized fresh, the prose as
    /// given. `expect` is the revision the writer believes in; on a
    /// mismatch the write does not happen and the current revision is
    /// returned in the conflict.
    pub fn write(&self, kind: &str, slug: &str, front: &FrontMatter, body: &str, expect: Option<&str>) -> Result<String, WriteError> {
        check_name(kind)?;
        check_name(slug)?;
        let path = self.file(kind, slug);
        let existed = path.is_file();
        match (existed, expect) {
            (true, Some(want)) => {
                let (_, now) = self.read_raw(&format!("{kind}:{slug}")).ok_or(WriteError::Missing)?;
                if now != want {
                    return Err(WriteError::Conflict(now));
                }
            }
            (true, None) => return Err(WriteError::Conflict(revision_of(&std::fs::read_to_string(&path)?))),
            (false, _) => {
                if expect.is_some() {
                    return Err(WriteError::Missing);
                }
            }
        }
        let raw = render_markdown_file(front, body);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&path, raw.as_bytes())?;
        Ok(revision_of(&raw))
    }

    /// Takes an item out of the tree, revision first.
    pub fn delete(&self, id: &str, expect: &str) -> Result<(), WriteError> {
        let (kind, slug) = Self::split_id(id)?;
        let path = self.file(&kind, &slug);
        if !path.is_file() {
            return Err(WriteError::Missing);
        }
        let raw = std::fs::read_to_string(&path)?;
        if revision_of(&raw) != expect {
            return Err(WriteError::Conflict(revision_of(&raw)));
        }
        let dir = path.parent().ok_or_else(|| WriteError::Missing)?.to_path_buf();
        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }
}

/// An entry said from what is on disk.
fn entry_of(id: &str, kind: &str, slug: &str, front: &FrontMatter, raw: &str, path: &Path) -> Entry {
    let mut tags = Vec::new();
    for name in ["tags", "categories"] {
        if let Some(terms) = front.taxonomies.get(name) {
            tags.extend(terms.iter().cloned());
        }
    }
    Entry {
        id: id.to_string(),
        kind: kind.to_string(),
        slug: slug.to_string(),
        title: front.title.clone(),
        status: front.status.as_str().to_string(),
        date: front.date.map(|d| d.to_rfc3339()),
        cover: front.cover.clone(),
        tags,
        updated_at: std::fs::metadata(path).ok().and_then(|m| m.modified().ok()).map(|t| {
            let dt: chrono::DateTime<chrono::Utc> = t.into();
            dt.to_rfc3339()
        }),
        revision: revision_of(raw),
        raw_front_matter: String::new(),
    }
}

/// Front matter and prose, said as a file says them.
fn render_markdown_file(front: &FrontMatter, body: &str) -> String {
    let mut yaml = serde_yaml::to_string(front).unwrap_or_default();
    // serde_yaml writes `---\n` … `---`? It writes a bare document; give
    // it the fences a kiters file speaks.
    yaml = yaml.trim_end().to_string();
    format!("---\n{yaml}\n---\n\n{body}")
}

/// A name the tree will hold: lowercase letters, digits, dashes, no
/// traveling anywhere.
fn check_name(name: &str) -> Result<(), WriteError> {
    let honest = !name.is_empty()
        && name.len() <= 64
        && name != "."
        && name != ".."
        && name
            .bytes()
            .all(|b| matches!(b, b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_'));
    if honest {
        Ok(())
    } else {
        Err(WriteError::BadName(name.to_string()))
    }
}

/// The revision of a file's own bytes: FNV-1a, 64 bits, said in hex.
/// Not a secret — a witness: any two writers of one item name different
/// witnesses, and the second to arrive is told the first's.
pub fn revision_of(raw: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in raw.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn walk_kinds(content: &Path) -> Vec<String> {
    let mut kinds = Vec::new();
    if let Ok(entries) = std::fs::read_dir(content) {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                if let Some(name) = entry.file_name().to_str() {
                    if check_name(name).is_ok() {
                        kinds.push(name.to_string());
                    }
                }
            }
        }
    }
    kinds.sort();
    kinds
}

fn walk_slugs(dir: &Path) -> anyhow::Result<Vec<String>> {
    let mut slugs = Vec::new();
    if !dir.is_dir() {
        return Ok(slugs);
    }
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        if entry.path().join("index.md").is_file() {
            if let Some(name) = entry.file_name().to_str() {
                slugs.push(name.to_string());
            }
        }
    }
    slugs.sort();
    Ok(slugs)
}

/// An item's parts, for the API to carry.
#[derive(Clone, Debug)]
pub struct ItemBody {
    pub front: FrontMatter,
    pub body: String,
}

/// Parses a raw file into its parts.
pub fn parse_raw(raw: &str) -> ItemBody {
    let (front, body) = kiters_parse::split_front_matter(raw);
    ItemBody { front, body }
}
