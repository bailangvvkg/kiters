//! Running a build: read, plan, pre-render everything, write it out.

use std::fs;
use std::path::Path;
use std::time::Instant;

use kiters_core::{BuildReport, Hooks, Page, Plan, SiteMeta, Target, TargetKind};
use kiters_parse::{read_config, walk_content};
use kiters_render::{markdown_to_html, Layout, RenderOptions};

use crate::plan::{plan_site, PlanOptions};

/// Everything one build of one site is told: where the site is, what
/// walks with it, how markdown renders, what the plan takes into
/// account, and where the finished site lands when it should land
/// anywhere at all.
pub struct SiteBuilder<'a> {
    pub root: &'a Path,
    pub hooks: Hooks,
    pub layout: Layout,
    pub markdown: RenderOptions,
    pub plan: PlanOptions,
    pub out: Option<&'a Path>,
}

impl<'a> SiteBuilder<'a> {
    /// The colors, shared by every page of every build of the process:
    /// looked up once, worn everywhere.
    pub fn highlighter(&self) -> &'static kiters_render::Highlighter {
        static HIGHLIGHTER: std::sync::OnceLock<kiters_render::Highlighter> = std::sync::OnceLock::new();
        HIGHLIGHTER.get_or_init(kiters_render::Highlighter::new)
    }

    /// A build of the site at `root` with no hooks and the built-in
    /// layout — the ordinary build of an ordinary site. Nothing is
    /// written until [`SiteBuilder::out`] says where.
    pub fn new(root: &'a Path) -> Self {
        let mut builder = SiteBuilder {
            root,
            hooks: Hooks::new(),
            layout: Layout::builtin(),
            markdown: RenderOptions::default(),
            plan: PlanOptions { with_drafts: false, with_not_found: true, with_taxonomies: true },
            out: None,
        };
        // What the site's yaml says about raw HTML is the site's own
        // call; a build that says --unsafe only ever widens it.
        if let Ok(meta) = kiters_parse::read_config(root) {
            builder.markdown.unsafe_html |= meta.markdown_unsafe;
        }
        builder
    }

    /// Writes the finished site under `out` (a build's default is
    /// `<root>/dist`).
    pub fn out(mut self, out: &'a Path) -> Self {
        self.out = Some(out);
        self
    }

    /// Wears a theme of the site's own: `themes/<name>/` says the
    /// pages it says, and the built-ins answer the rest.
    pub fn theme(mut self, name: &str) -> Self {
        self.layout = kiters_render::Layout::theme(self.root.join("themes").join(name));
        self
    }

    /// Renders in memory only; nothing is written.
    pub fn no_write(mut self) -> Self {
        self.out = None;
        self
    }

    /// Adds a hook to the build's chain.
    pub fn hook(mut self, hook: impl kiters_core::Hook + 'static) -> Self {
        self.hooks = std::mem::take(&mut self.hooks).add(hook);
        self
    }

    /// Adds a hook already shared to the build's chain — a plugin,
    /// loaded once and run on every build, wants this.
    pub fn hook_shared(mut self, hook: std::sync::Arc<dyn kiters_core::Hook>) -> Self {
        self.hooks = std::mem::take(&mut self.hooks).add_shared(hook);
        self
    }
}

/// Reads a site, plans it, and pre-renders every page of the plan.
///
/// Pre-rendering is the point: every page exists, whole, before the
/// first reader arrives — a server in front of the result has nothing
/// left to do but hand over bytes.
pub fn build_site(builder: &SiteBuilder<'_>) -> anyhow::Result<(SiteMeta, Plan, Vec<Page>, BuildReport)> {
    let started = Instant::now();

    let site = read_config(builder.root)?;
    let (items, errors) = walk_content(builder.root, &builder.hooks);
    for err in &errors {
        eprintln!("跳过 {}", err);
    }
    let now = chrono::Utc::now();
    let drafts = items.iter().filter(|i| i.status() == kiters_core::Status::Draft).count();
    let public: Vec<_> = items
        .iter()
        .filter(|i| i.status().stands(i.dated(now), now))
        .cloned()
        .collect();

    let plan = plan_site(&items, &builder.hooks, builder.plan)?;

    let mut pages = Vec::with_capacity(plan.targets.len());
    for target in &plan.targets {
        let html = render_target(builder, &site, target, &public)?;
        let mut page = Page {
            url: target.url.clone(),
            status: match target.kind {
                TargetKind::NotFound => 404,
                _ => 200,
            },
            html,
        };
        builder.hooks.page(&mut page)?;
        pages.push(page);
    }

    let mut report = BuildReport {
        items: items.len(),
        drafts,
        rendered: pages.len(),
        written: 0,
        took: started.elapsed(),
    };
    builder.hooks.build_complete(&site, &plan, &pages, &report)?;

    if let Some(out) = builder.out {
        for page in &pages {
            let path = out.join(Target::address_to_path(&page.url));
            if let Some(dir) = path.parent() {
                fs::create_dir_all(dir)?;
            }
            fs::write(&path, &page.html)?;
            report.written += 1;
        }
        // The theme's own static files land with the pages, at their own
        // addresses — the site's clothes are the site's to serve.
        if let Some(static_dir) = builder.layout.static_dir() {
            // What sits under static/ is the site's own root here.
            for file in walk_files(static_dir) {
                if let Ok(under) = file.strip_prefix(static_dir) {
                    let to = out.join(under);
                    if let Some(dir) = to.parent() {
                        fs::create_dir_all(dir)?;
                    }
                    if fs::copy(&file, &to).is_ok() {
                        report.written += 1;
                    }
                }
            }
        }
    }
    report.took = started.elapsed();
    Ok((site, plan, pages, report))
}

/// Renders one target of the plan. The page hook has not fired yet —
/// the whole page is finished first, then offered once.
fn render_target(
    builder: &SiteBuilder<'_>,
    site: &SiteMeta,
    target: &Target,
    public: &[kiters_core::Content],
) -> anyhow::Result<String> {
    match &target.kind {
        TargetKind::Item => {
            let Some(content) = &target.content else {
                return Ok(String::new());
            };
            // Shortcodes fold first — they are markdown's own little
            // structures, and what they fold into renders like any prose.
            let folded = kiters_render::fold_shortcodes(&content.body);
            let mut html = markdown_to_html(&folded, builder.markdown);
            if builder.markdown.highlight {
                let highlighter = builder.highlighter();
                html = kiters_render::highlight_code_blocks(&html, &highlighter);
            }
            builder.hooks.html(&target.url, &mut html)?;
            Ok(builder.layout.post(site, content, &html))
        }
        TargetKind::Home => Ok(builder.layout.home(site, public)),
        TargetKind::NotFound => Ok(builder.layout.not_found(site)),
        TargetKind::Alias { to } => Ok(builder.layout.alias(site, to)),
        TargetKind::Taxonomy(name) => {
            let terms: Vec<(String, String)> = public
                .iter()
                .filter_map(|item| item.front.taxonomies.get(name.as_str()))
                .flatten()
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .map(|term| {
                    let url = format!("/{name}/{}/", crate::plan::term_encode(term));
                    (term.clone(), url)
                })
                .collect();
            Ok(builder.layout.taxonomy(site, name, &terms))
        }
        TargetKind::Term { taxonomy, term } => {
            let carrying: Vec<_> = public
                .iter()
                .filter(|item| {
                    item.front.taxonomies.get(taxonomy.as_str()).is_some_and(|terms| terms.iter().any(|t| t == term))
                })
                .collect();
            let items: Vec<kiters_core::Content> = carrying.into_iter().cloned().collect();
            Ok(builder.layout.list(site, term, &items))
        }
        TargetKind::Extra("search") => {
            // The search index is a build's product like any page: every
            // standing item, said plainly — title, address, date, and
            // the prose as text — for a theme's own reader to search
            // with, client-side, no server asked twice.
            let mut json = String::from("{\"items\":[");
            let mut first = true;
            for item in public {
                let text = plain_text(&item.body);
                let entry = serde_json::json!({
                    "title": item.front.title,
                    "url": item.url(),
                    "date": item.front.date.map(|d| d.date_naive().to_string()),
                    "text": text,
                });
                if !first {
                    json.push(',');
                }
                first = false;
                json.push_str(&entry.to_string());
            }
            json.push_str("]}");
            Ok(json)
        }
        TargetKind::Extra("sitemap") => {
            let urls: Vec<String> = public
                .iter()
                .map(|c| c.url())
                .chain(std::iter::once("/".to_string()))
                .collect();
            Ok(builder.layout.sitemap(site, &urls))
        }
        TargetKind::Extra("rss") => {
            // The feed says what a reader wants in a list: what it is
            // called, where it lives, when it stood, and a taste of it —
            // not the whole of every page, which is what the pages are
            // for. Its cover rides along as media.
            let items: Vec<_> = public
                .iter()
                .take(20)
                .map(|c| {
                    let id = c
                        .front
                        .extra
                        .get(serde_yaml::Value::String("id".to_string()))
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| c.url());
                    (
                        c.url(),
                        c.front.title.clone(),
                        c.front.date.map(|d| d.to_rfc2822()).unwrap_or_default(),
                        tasted(&plain_text(&c.body)),
                        c.front.cover.clone().unwrap_or_default(),
                        id,
                    )
                })
                .collect();
            Ok(builder.layout.rss(site, &items))
        }
        TargetKind::Extra(_) => Ok(String::new()),
    }
}

/// Markdown said as plain text: the marks gone, the words kept. A
/// search index does not rank formatting; it reads it.
fn plain_text(markdown: &str) -> String {
    let mut text = String::with_capacity(markdown.len());
    for line in markdown.lines() {
        let line = line.trim();
        if line.starts_with("```") || line.is_empty() {
            continue;
        }
        let said: String = line
            .trim_start_matches('#')
            .trim()
            .chars()
            .filter(|&c| !matches!(c, '*' | '`' | '[' | ']' | '!' | '>'))
            .collect();
        if !said.is_empty() {
            text.push_str(&said);
            text.push(' ');
        }
    }
    text.chars().take(500).collect::<String>().trim_end().to_string()
}

/// Every file under a directory, however deep.
fn walk_files(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(read) = fs::read_dir(&dir) else { continue };
        for entry in read.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                found.push(path);
            }
        }
    }
    found
}

/// A taste of a text: enough to know whether to click, with the cut
/// said plainly.
fn tasted(text: &str) -> String {
    let cut: String = text.chars().take(300).collect();
    if cut.chars().count() < text.chars().count() {
        format!("{cut}…")
    } else {
        cut
    }
}
