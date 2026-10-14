//! The hooks: every place the pipeline does anything, one of these fires.
//!
//! A hook is a trait object added to a [`Hooks`] chain before a build
//! runs; each method defaults to doing nothing, so a hook overrides the
//! moments it cares about and ignores the rest. Events borrow what they
//! are about, mutably where changing it changes the build — and carry a
//! `skip` where vetoing is part of the point.
//!
//! The chain runs in the order hooks were added; the first error stops
//! the build, named after the hook that raised it. Hooks may not panic
//! the build away on purpose: results are the currency.
//!
//! ```text
//! on_discover        a content file is found, before it is read
//! on_front_matter    parsed; the matter may still be shaped
//! on_markdown        the prose, before it becomes HTML
//! on_target          planned; an address may still be moved or dropped
//! on_html            the prose as HTML, before the layout wraps it
//! on_page            the finished page, the last word on its bytes
//! on_build_complete  everything rendered, the report ready to read
//! ```

use std::path::Path;
use std::sync::Arc;

use crate::{BuildReport, Content, Page, Plan, SiteMeta, Target};

/// A hook into the making of a site. See the module for the shape of the
/// chain; the doc on each method says the moment it fires.
pub trait Hook: Send + Sync {
    /// The name errors and logs carry this hook under.
    fn name(&self) -> &str {
        std::any::type_name::<Self>()
    }

    /// A content file has been found. Fired before anything is read, so a
    /// hook that knows better may skip the file whole; `kind` is the kind
    /// the path says it is, before the front matter has its say.
    fn on_discover(&self, _event: DiscoverEvent<'_>) -> anyhow::Result<()> {
        Ok(())
    }

    /// An item's front matter is parsed. The matter is mutable here: a
    /// hook may fill a slug, date, or status a file left blank.
    fn on_front_matter(&self, _event: FrontMatterEvent<'_>) -> anyhow::Result<()> {
        Ok(())
    }

    /// The prose of an item, still markdown. A hook may rewrite it — this
    /// is where a link becomes a card and a diagram becomes a figure.
    fn on_markdown(&self, _event: MarkdownEvent<'_>) -> anyhow::Result<()> {
        Ok(())
    }

    /// A target has been planned. The address and the output path may
    /// still be moved, and the target may be skipped — the one place a
    /// hook takes a page out of the site.
    fn on_target(&self, _event: TargetEvent<'_>) -> anyhow::Result<()> {
        Ok(())
    }

    /// The item's prose as HTML, before the layout wraps it. Last look at
    /// the body alone.
    fn on_html(&self, _event: HtmlEvent<'_>) -> anyhow::Result<()> {
        Ok(())
    }

    /// A page is finished: layout, head, everything. The last word on the
    /// bytes; what this leaves is what the reader reads.
    fn on_page(&self, _event: PageEvent<'_>) -> anyhow::Result<()> {
        Ok(())
    }

    /// The build is complete: every page rendered, nothing written yet.
    /// The report is read-only; the plan is the shape of what was.
    fn on_build_complete(&self, _event: BuildEvent<'_>) -> anyhow::Result<()> {
        Ok(())
    }
}

/// A file of content, found but not yet read.
pub struct DiscoverEvent<'a> {
    pub path: &'a Path,
    pub kind: &'a crate::Kind,
    /// Set to take the file out of the build entirely.
    pub skip: &'a mut bool,
}

/// An item's front matter, parsed and open to shaping.
pub struct FrontMatterEvent<'a> {
    pub content: &'a mut Content,
}

/// The markdown of an item, open to rewriting.
pub struct MarkdownEvent<'a> {
    pub content: &'a mut Content,
}

/// A target of the plan, open to moving or dropping.
pub struct TargetEvent<'a> {
    pub target: &'a mut Target,
    /// Set to take the target out of the plan.
    pub skip: &'a mut bool,
}

/// The rendered body of an item, open to a last look.
pub struct HtmlEvent<'a> {
    pub url: &'a str,
    pub html: &'a mut String,
}

/// A finished page, the last word on its bytes.
pub struct PageEvent<'a> {
    pub page: &'a mut Page,
}

/// The whole build, done.
pub struct BuildEvent<'a> {
    pub site: &'a SiteMeta,
    pub plan: &'a Plan,
    pub pages: &'a [Page],
    pub report: &'a BuildReport,
}

/// The chain of hooks a build runs with, in the order they were added.
///
/// Every firing walks the chain once, in order, stopping at the first
/// error. An empty chain — the ordinary build of an ordinary site —
/// costs one call through a slice.
#[derive(Clone, Default)]
pub struct Hooks {
    chain: Vec<Arc<dyn Hook>>,
}

impl Hooks {
    /// An empty chain: a build with no one watching.
    pub fn new() -> Self {
        Hooks::default()
    }

    /// Adds a hook to the end of the chain.
    pub fn add(mut self, hook: impl Hook + 'static) -> Self {
        self.chain.push(Arc::new(hook));
        self
    }

    /// Adds a shared hook to the end of the chain.
    pub fn add_shared(mut self, hook: Arc<dyn Hook>) -> Self {
        self.chain.push(hook);
        self
    }

    /// How many hooks the chain carries.
    pub fn len(&self) -> usize {
        self.chain.len()
    }

    pub fn is_empty(&self) -> bool {
        self.chain.is_empty()
    }

    /// Runs a moment of the chain: each hook in order, the first error
    /// returned named after the hook that raised it.
    fn fire(&self, name: &str, mut each: impl FnMut(&dyn Hook) -> anyhow::Result<()>) -> anyhow::Result<()> {
        for hook in &self.chain {
            if let Err(err) = each(hook.as_ref()) {
                return Err(err.context(format!("hook {} failed at {}", hook.name(), name)));
            }
        }
        Ok(())
    }

    pub fn discover(&self, path: &Path, kind: &crate::Kind, skip: &mut bool) -> anyhow::Result<()> {
        self.fire("discover", |h| h.on_discover(DiscoverEvent { path, kind, skip }))
    }

    pub fn front_matter(&self, content: &mut Content) -> anyhow::Result<()> {
        self.fire("front matter", |h| h.on_front_matter(FrontMatterEvent { content }))
    }

    pub fn markdown(&self, content: &mut Content) -> anyhow::Result<()> {
        self.fire("markdown", |h| h.on_markdown(MarkdownEvent { content }))
    }

    pub fn target(&self, target: &mut Target, skip: &mut bool) -> anyhow::Result<()> {
        self.fire("target", |h| h.on_target(TargetEvent { target, skip }))
    }

    pub fn html(&self, url: &str, html: &mut String) -> anyhow::Result<()> {
        self.fire("html", |h| h.on_html(HtmlEvent { url, html }))
    }

    pub fn page(&self, page: &mut Page) -> anyhow::Result<()> {
        self.fire("page", |h| h.on_page(PageEvent { page }))
    }

    pub fn build_complete(&self, site: &SiteMeta, plan: &Plan, pages: &[Page], report: &BuildReport) -> anyhow::Result<()> {
        self.fire("build complete", |h| h.on_build_complete(BuildEvent { site, plan, pages, report }))
    }
}
