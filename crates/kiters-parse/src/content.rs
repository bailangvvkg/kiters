//! Reading a site's content: the tree walk, the front matter split, and
//! the item a file becomes.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::Utc;
use kiters_core::{Content, FrontMatter, Hooks, Kind, SiteMeta};

/// Reads a site's configuration — the same `kite.yaml` a kite site
/// already carries, or a `kiters.yaml` if the site would rather not.
///
/// Only the `site` table is read; the rest of the file is none of this
/// crate's business.
pub fn read_config(root: &Path) -> anyhow::Result<SiteMeta> {
    for name in ["kite.yaml", "kiters.yaml"] {
        let path = root.join(name);
        if !path.is_file() {
            continue;
        }
        let text = fs::read_to_string(&path)?;
        let raw: serde_yaml::Value = serde_yaml::from_str(&text)?;
        let site = raw.get("site").cloned().unwrap_or_default();
        return Ok(serde_yaml::from_value(site)?);
    }
    Ok(SiteMeta::default())
}

/// A walk refused to go on. The path is the file that did it.
#[derive(Debug)]
pub struct WalkError {
    pub path: PathBuf,
    pub cause: String,
}

impl std::fmt::Display for WalkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.path.display(), self.cause)
    }
}
impl std::error::Error for WalkError {}

/// Splits an item's file into its front matter and its prose.
///
/// The matter is the leading `---` block; a file without one is all
/// prose, which a site may well mean.
pub fn split_front_matter(text: &str) -> (FrontMatter, String) {
    let trimmed = text.trim_start_matches('\u{feff}');
    let after = trimmed.trim_start_matches(['\r', '\n']);
    if let Some(rest) = after.strip_prefix("---") {
        // The matter runs to the next line that is only dashes.
        if let Some(end) = rest.find("\n---") {
            let tail = &rest[end + 4..];
            let matter = &rest[..end];
            let mut front: FrontMatter = match serde_yaml::from_str(matter) {
                Ok(front) => front,
                Err(err) => {
                    // A matter that will not parse does not sink the
                    // item: it stands with every field unsaid, and the
                    // builder hears about it.
                    eprintln!("front matter 解析失败,字段全部按缺省处理: {err}");
                    FrontMatter::default()
                }
            };
            // A file may carry fields we have no name for; those went to
            // `extra` already. A matter that failed to parse entirely is
            // empty — the walk reports it, and the prose still renders.
            // Kite's posts say tags and categories at the top of the
            // matter, and date the moment they published; read both
            // habits as the words kiters speaks.
            adopt_kite_fields(&mut front);
            if front.title.is_empty() {
                front.title = matter
                    .lines()
                    .find_map(|line| line.trim().strip_prefix("title:"))
                    .map(|said| said.trim().trim_matches('"').to_string())
                    .or_else(|| {
                        matter.lines().map(str::trim).find(|line| !line.is_empty()).map(str::to_string)
                    })
                    .unwrap_or_default();
            }
            return (front, tail.trim_start_matches(['\r', '\n']).to_string());
        }
    }
    (FrontMatter::default(), trimmed.to_string())
}

/// The fields a kite post carries at the top of its matter, moved to
/// where kiters keeps them: `tags` and `categories` into the taxonomies
/// they already are, and a missing `date` taken from `published_at`.
fn adopt_kite_fields(front: &mut FrontMatter) {
    for (top, taxonomy) in [("tags", "tags"), ("categories", "categories")] {
        let carried = front.extra.get(serde_yaml::Value::String(top.to_string())).cloned();
        if let Some(serde_yaml::Value::Sequence(terms)) = carried {
            if !front.taxonomies.contains_key(taxonomy) {
                let said: Vec<String> = terms
                    .into_iter()
                    .filter_map(|term| term.as_str().map(str::to_string))
                    .collect();
                if !said.is_empty() {
                    front.taxonomies.insert(taxonomy.to_string(), said);
                    front.extra.remove(serde_yaml::Value::String(top.to_string()));
                }
            }
        }
    }
    if front.date.is_none() {
        if let Some(serde_yaml::Value::String(published)) =
            front.extra.get(serde_yaml::Value::String("published_at".to_string())).cloned()
        {
            if let Ok(date) = chrono::DateTime::parse_from_rfc3339(published.trim()) {
                front.date = Some(date.with_timezone(&chrono::Utc()));
                front.extra.remove(serde_yaml::Value::String("published_at".to_string()));
            }
        }
    }
}

/// Reads one item: front matter, prose, the slug from the directory the
/// file lives in when the matter does not say one.
pub fn read_item(path: &Path, kind: &Kind, hooks: &Hooks) -> Result<Option<Content>, WalkError> {
    let mut skip = false;
    if let Err(err) = hooks.discover(path, kind, &mut skip) {
        return Err(WalkError { path: path.to_path_buf(), cause: err.to_string() });
    }
    if skip {
        return Ok(None);
    }

    let text = fs::read_to_string(path).map_err(|e| WalkError { path: path.to_path_buf(), cause: e.to_string() })?;
    let (front, body) = split_front_matter(&text);

    let slug = front
        .slug
        .clone()
        .unwrap_or_else(|| slug_from_dir(path).unwrap_or_default());

    let mut content = Content {
        kind: kind.clone(),
        slug,
        front,
        body,
        source: path.to_path_buf(),
    };
    if let Err(err) = hooks.front_matter(&mut content) {
        return Err(WalkError { path: path.to_path_buf(), cause: err.to_string() });
    }
    if let Err(err) = hooks.markdown(&mut content) {
        return Err(WalkError { path: path.to_path_buf(), cause: err.to_string() });
    }
    Ok(Some(content))
}

/// The directory an item's file lives in is the item's slug: the file is
/// `index.md` inside a directory named after the item.
fn slug_from_dir(path: &Path) -> Option<String> {
    path.parent()?.file_name()?.to_str().map(str::to_owned)
}

/// Walks a site's `content` tree and reads every item in it.
///
/// The layout is the one kite sites already have: `content/<kind dir>/
/// <slug>/index.md`. Errors are collected, not thrown: one unreadable
/// file takes itself out of the build, and the site builds around it.
pub fn walk_content(root: &Path, hooks: &Hooks) -> (Vec<Content>, Vec<WalkError>) {
    let mut items = Vec::new();
    let mut errors = Vec::new();

    let content = root.join("content");
    let Ok(entries) = fs::read_dir(&content) else {
        return (items, errors);
    };
    for kind_dir in entries.flatten() {
        let kind_path = kind_dir.path();
        if !kind_path.is_dir() {
            continue;
        }
        let kind_name = kind_path.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_owned();
        let kind = Kind::new(kind_name.trim_end_matches('s'));

        for file in walkdir::WalkDir::new(&kind_path).sort_by_file_name() {
            let entry = match file {
                Ok(e) => e,
                Err(e) => {
                    errors.push(WalkError { path: kind_path.clone(), cause: e.to_string() });
                    continue;
                }
            };
            if !entry.file_type().is_file() || entry.file_name() != "index.md" {
                continue;
            }
            match read_item(entry.path(), &kind, hooks) {
                Ok(Some(item)) => items.push(item),
                Ok(None) => {}
                Err(e) => errors.push(e),
            }
        }
    }

    // The newest first, the way a home page wants them.
    let now = Utc::now();
    items.sort_by(|a, b| b.dated(now).cmp(&a.dated(now)));
    (items, errors)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_date_reads_as_its_midnight() {
        let (front, body) = split_front_matter("---
title: 一篇
date: 2026-01-05
---

正文
");
        assert_eq!(front.title, "一篇");
        assert_eq!(front.date.map(|d| d.to_rfc3339()), Some("2026-01-05T00:00:00+00:00".to_string()));
        assert_eq!(body, "正文
");
    }

    #[test]
    fn a_whole_timestamp_reads_whole() {
        let (front, _) = split_front_matter("---
date: 2026-01-02T10:00:00Z
---
");
        assert_eq!(front.date.map(|d| d.to_rfc3339()), Some("2026-01-02T10:00:00+00:00".to_string()));
    }

    #[test]
    fn a_kite_post_reads_in_kiters_words() {
        let (front, _) = split_front_matter(
            "---
title: 一篇
status: published
published_at: 2024-09-08T08:19:00Z
tags: [随笔, 开工]
categories: [技术]
---
",
        );
        assert_eq!(front.taxonomies.get("tags").map(|t| t.len()), Some(2));
        assert_eq!(front.taxonomies.get("categories").map(|t| t.len()), Some(1));
        assert_eq!(front.date.map(|d| d.to_rfc3339()), Some("2024-09-08T08:19:00+00:00".to_string()));
        assert!(!front.extra.contains_key(&serde_yaml::Value::String("tags".to_string())));
    }

    #[test]
    fn a_said_date_wins_over_the_published() {
        let (front, _) = split_front_matter(
            "---
date: 2026-01-01
published_at: 2024-09-08T08:19:00Z
---
",
        );
        assert_eq!(front.date.map(|d| d.to_rfc3339()), Some("2026-01-01T00:00:00+00:00".to_string()));
    }

    #[test]
    fn no_date_is_no_date() {
        let (front, _) = split_front_matter("---
title: x
---
");
        assert!(front.date.is_none());
    }
}
