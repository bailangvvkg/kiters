//! A snapshot: the whole site, rendered, held in one piece.

use std::collections::HashMap;
use std::path::Path;

use kiters_build::{build_site, SiteBuilder};

/// Every page of one build, by address. Built whole before anyone reads
/// it, read whole by everyone after — the only cache whose emptiness is
/// the site changing.
pub struct Snapshot {
    site: kiters_core::SiteMeta,
    /// Each page as counted bytes: handing one to a reader adds a
    /// count, not a copy.
    pages: HashMap<String, (u16, bytes::Bytes)>,
    /// The theme's static files, by address, held whole with the pages
    /// they dress.
    assets: HashMap<String, bytes::Bytes>,
    /// The instants a scheduled item comes due; a watcher waits out the
    /// nearest and rebuilds when it passes.
    due: Vec<chrono::DateTime<chrono::Utc>>,
    built_in: std::time::Duration,
}

impl Snapshot {
    /// Builds the site and holds it whole. Nothing is written anywhere.
    pub fn build(root: &Path, with_drafts: bool) -> anyhow::Result<Self> {
        Self::from_builder(&mut SiteBuilder::new(root).no_write(), with_drafts)
    }

    /// Builds from a configured builder, drafts as asked.
    pub fn from_builder(builder: &mut SiteBuilder<'_>, with_drafts: bool) -> anyhow::Result<Self> {
        builder.plan.with_drafts = with_drafts;
        let (site, plan, pages, report) = build_site(builder)?;
        let due = plan
            .targets
            .iter()
            .filter_map(|t| t.content.as_ref())
            .filter(|c| c.status() == kiters_core::Status::Scheduled)
            .filter_map(|c| c.front.date)
            .collect();
        let assets = builder
            .layout
            .static_dir()
            .map(read_assets)
            .unwrap_or_default();
        Ok(Snapshot {
            site,
            pages: pages
                .into_iter()
                .map(|page| (page.url, (page.status, bytes::Bytes::from(page.html))))
                .collect(),
            assets,
            due,
            built_in: report.took,
        })
    }

    /// The page an address answers with.
    pub fn get(&self, path: &str) -> Option<(u16, bytes::Bytes)> {
        self.pages.get(path).cloned()
    }

    /// The not-found page, rendered like any other.
    pub fn not_found(&self) -> (u16, bytes::Bytes) {
        self.pages.get("/404.html").cloned().unwrap_or((404, bytes::Bytes::from_static(b"not found")))
    }

    /// How many pages the snapshot holds.
    pub fn pages_count(&self) -> usize {
        self.pages.len()
    }

    /// The site's own facts, for whoever asks.
    pub fn site(&self) -> &kiters_core::SiteMeta {
        &self.site
    }

    /// The nearest instant a scheduled item comes due, if one is
    /// waiting.
    pub fn next_due(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        self.due.iter().copied().min()
    }

    /// How long the snapshot took to build.
    pub fn built_in(&self) -> std::time::Duration {
        self.built_in
    }
}

/// A theme's static directory, read whole: every file under its own
/// address, `/almanac/almanac.css` and all its neighbours.
fn read_assets(root: &Path) -> HashMap<String, bytes::Bytes> {
    // What sits under static/ answers at its own address, from the root.
    let mut assets = HashMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(read) = std::fs::read_dir(&dir) else { continue };
        for entry in read.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if let Ok(bytes) = std::fs::read(&path) {
                if let Ok(under) = path.strip_prefix(root) {
                    let address = format!("/{}", under.to_string_lossy().replace('\\', "/"));
                    assets.insert(address, bytes::Bytes::from(bytes));
                }
            }
        }
    }
    assets
}

impl Snapshot {
    /// A static asset of the theme, by address.
    pub fn asset(&self, path: &str) -> Option<bytes::Bytes> {
        self.assets.get(path).cloned()
    }
}
