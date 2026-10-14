//! Turning read items into the targets of one build.

use std::path::PathBuf;

use chrono::Utc;
use kiters_core::{Content, Hooks, Plan, Target, TargetKind};

/// What a plan takes into account.
#[derive(Clone, Copy, Default)]
pub struct PlanOptions {
    /// Drafts stand in the public site; a preview wants this, a
    /// deployment never does.
    pub with_drafts: bool,
    /// A not-found page is planned; serve wants it, a plain build too.
    pub with_not_found: bool,
    /// Taxonomy and term pages are planned. A site with no terms has
    /// none either way.
    pub with_taxonomies: bool,
}

/// Plans a site's targets from its items, `now` the judge of what stands.
///
/// Every standing item is a target; its aliases are targets saying where
/// it lives now; each taxonomy is an index, each of its terms a list; the
/// home page belongs to them all; the sitemap and the feed speak for the
/// whole site. Each target is offered to the hooks — moved, renamed, or
/// taken out by them — before it stands in the plan.
pub fn plan_site(items: &[Content], hooks: &Hooks, opts: PlanOptions) -> anyhow::Result<Plan> {
    let now = Utc::now();
    let mut targets = Vec::new();
    let mut public: Vec<&Content> = Vec::new();

    for item in items {
        let standing = item.status().stands(item.dated(now), now) || opts.with_drafts;
        if !standing {
            continue;
        }
        let url = item.url();
        let mut target = Target {
            kind: TargetKind::Item,
            url: url.clone(),
            out: Target::address_to_path(&url),
            content: Some(item.clone()),
        };
        if !offer(&mut target, hooks)? {
            continue;
        }
        if item.status().stands(item.dated(now), now) {
            public.push(item);
        }
        targets.push(target);

        // An alias is a page, not a redirect: it carries the new address
        // in it and lands even for the reader nothing runs for.
        for alias in &item.front.aliases {
            let url = alias_path(alias);
            let mut target = Target {
                kind: TargetKind::Alias { to: item.url() },
                url: url.clone(),
                out: Target::address_to_path(&url),
                content: Some(item.clone()),
            };
            if offer(&mut target, hooks)? {
                targets.push(target);
            }
        }
    }

    if opts.with_taxonomies {
        // The taxonomies a site has are the ones its items carry.
        let mut names: Vec<&String> = public
            .iter()
            .flat_map(|item| item.front.taxonomies.keys())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        names.sort();
        for name in names {
            let index_url = format!("/{}/", name);
            let mut index = Target {
                kind: TargetKind::Taxonomy(name.clone()),
                url: index_url.clone(),
                out: Target::address_to_path(&index_url),
                content: None,
            };
            if offer(&mut index, hooks)? {
                targets.push(index);
            }
            let mut terms: Vec<&String> = public
                .iter()
                .filter_map(|item| item.front.taxonomies.get(name))
                .flatten()
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect();
            terms.sort();
            for term in terms {
                let url = format!("/{}/{}/", name, term_encode(term));
                let mut target = Target {
                    kind: TargetKind::Term { taxonomy: name.clone(), term: term.clone() },
                    url: url.clone(),
                    out: Target::address_to_path(&url),
                    content: None,
                };
                if offer(&mut target, hooks)? {
                    targets.push(target);
                }
            }
        }
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
    for (kind, url, out) in [
        (TargetKind::Extra("sitemap"), "/sitemap.xml".to_string(), PathBuf::from("sitemap.xml")),
        (TargetKind::Extra("rss"), "/rss.xml".to_string(), PathBuf::from("rss.xml")),
        (TargetKind::Extra("search"), "/search.json".to_string(), PathBuf::from("search.json")),
    ] {
        let mut target = Target { kind, url: url.clone(), out, content: None };
        if offer(&mut target, hooks)? {
            targets.push(target);
        }
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

    Ok(Plan { targets })
}

/// Offers a target to the hooks; false means the target is out.
fn offer(target: &mut Target, hooks: &Hooks) -> anyhow::Result<bool> {
    let mut skip = false;
    hooks.target(target, &mut skip)?;
    Ok(!skip)
}

/// An alias address as the plan writes it: a leading slash, a trailing
/// slash — the shape the site speaks.
fn alias_path(alias: &str) -> String {
    let mut url = alias.to_string();
    if !url.starts_with('/') {
        url.insert(0, '/');
    }
    if !url.ends_with('/') && !url.contains('.') {
        url.push('/');
    }
    url
}

/// A term's address part, percent-encoded where the address line needs
/// it: a term may say anything; an address says what a URL may.
pub fn term_encode(term: &str) -> String {
    let mut out = String::new();
    for byte in term.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(byte as char),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}
