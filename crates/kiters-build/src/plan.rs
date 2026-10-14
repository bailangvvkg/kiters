//! Turning read items into the targets of one build.

use std::path::PathBuf;

use kiters_core::{Hooks, Kind, Plan, Target, TargetKind};

/// What a plan takes into account.
#[derive(Clone, Copy, Default)]
pub struct PlanOptions {
    /// Drafts stand in the public site; a preview wants this, a
    /// deployment never does.
    pub with_drafts: bool,
    /// A not-found page is planned; serve wants it, a plain build too.
    pub with_not_found: bool,
}

/// Plans a site's targets from its items.
///
/// Every public item is a target; the home page belongs to them all;
/// the sitemap and the feed speak for the whole site. Each target is
/// offered to the hooks — moved, renamed, or taken out by them — before
/// it stands in the plan.
pub fn plan_site(items: &[kiters_core::Content], hooks: &Hooks, opts: PlanOptions) -> anyhow::Result<Plan> {
    let mut targets = Vec::new();
    let mut public = Vec::new();

    for item in items {
        if !item.status().is_public() && !opts.with_drafts {
            continue;
        }
        let url = item.url();
        let mut target = Target {
            kind: TargetKind::Item,
            url: url.clone(),
            out: kiters_core::Target::address_to_path(&url),
            content: Some(item.clone()),
        };
        if !offer(&mut target, hooks)? {
            continue;
        }
        if item.status().is_public() {
            public.push(item);
        }
        targets.push(target);
    }

    // The site's own pages, planned from what the items came to.
    let mut home = Target {
        kind: TargetKind::Home,
        url: "/".to_string(),
        out: PathBuf::from("index.html"),
        content: None,
    };
    if offer(&mut home, hooks)? {
        targets.push(home);
    }
    let mut sitemap = Target {
        kind: TargetKind::Extra("sitemap"),
        url: "/sitemap.xml".to_string(),
        out: PathBuf::from("sitemap.xml"),
        content: None,
    };
    if offer(&mut sitemap, hooks)? {
        targets.push(sitemap);
    }
    let mut rss = Target {
        kind: TargetKind::Extra("rss"),
        url: "/rss.xml".to_string(),
        out: PathBuf::from("rss.xml"),
        content: None,
    };
    if offer(&mut rss, hooks)? {
        targets.push(rss);
    }
    if opts.with_not_found {
        let mut nf = Target {
            kind: TargetKind::NotFound,
            url: "/404.html".to_string(),
            out: PathBuf::from("404.html"),
            content: None,
        };
        if offer(&mut nf, hooks)? {
            targets.push(nf);
        }
    }

    let _ = Kind::POST; // the vocabulary is the site's; nothing here names a kind
    Ok(Plan { targets })
}

/// Offers a target to the hooks; false means the target is out.
fn offer(target: &mut Target, hooks: &Hooks) -> anyhow::Result<bool> {
    let mut skip = false;
    hooks.target(target, &mut skip)?;
    Ok(!skip)
}
