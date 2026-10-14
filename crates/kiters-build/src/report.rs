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
    /// A build of the site at `root` with no hooks and the built-in
    /// layout — the ordinary build of an ordinary site. Nothing is
    /// written until [`SiteBuilder::out`] says where.
    pub fn new(root: &'a Path) -> Self {
        SiteBuilder {
            root,
            hooks: Hooks::new(),
            layout: Layout::builtin(),
            markdown: RenderOptions::default(),
            plan: PlanOptions { with_drafts: false, with_not_found: true, with_taxonomies: true },
            out: None,
        }
    }

    /// Writes the finished site under `out` (a build's default is
    /// `<root>/dist`).
    pub fn out(mut self, out: &'a Path) -> Self {
        self.out = Some(out);
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
    let drafts = items.iter().filter(|i| *i.status() == kiters_core::Status::Draft).count();
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
            builder.hooks.html(&target.url, &mut html)?;
            Ok(builder.layout.page(site, &content.front.title, &html))
        }
        TargetKind::Home => Ok(builder.layout.home(site, public)),
        TargetKind::NotFound => Ok(builder.layout.not_found(site)),
        TargetKind::Alias { to } => Ok(builder.layout.alias(site, to)),
        TargetKind::Taxonomy(name) => {
            let mut terms: Vec<(String, String)> = public
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
            let mut body = String::from("<ul>\n");
            for (term, url) in &terms {
                body.push_str(&format!("<li><a href=\"{url}\">{term}</a></li>\n"));
            }
            body.push_str("</ul>\n");
            Ok(builder.layout.page(site, name, &body))
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
        TargetKind::Extra("sitemap") => {
            let urls: Vec<String> = public
                .iter()
                .map(|c| c.url())
                .chain(std::iter::once("/".to_string()))
                .collect();
            Ok(builder.layout.sitemap(site, &urls))
        }
        TargetKind::Extra("rss") => {
            let items: Vec<_> = public
                .iter()
                .take(20)
                .map(|c| {
                    (
                        c.url(),
                        c.front.title.clone(),
                        c.front.date.map(|d| d.to_rfc2822()).unwrap_or_default(),
                        markdown_to_html(&c.body, builder.markdown),
                    )
                })
                .collect();
            Ok(builder.layout.rss(site, &items))
        }
        TargetKind::Extra(_) => Ok(String::new()),
    }
}
