//! The hooks fire, in order, and what they change changes the build.

use std::path::Path;

use kiters_build::{build_site, SiteBuilder};
use kiters_core::{Hook, Page, Target};

fn fixture() -> &'static Path {
    Box::leak(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixture")
            .canonicalize()
            .unwrap()
            .into_boxed_path(),
    )
}

/// A hook that rewrites markdown: the prose a reader sees is the prose
/// the hook left.
struct Marker(&'static str);

impl Hook for Marker {
    fn name(&self) -> &str {
        "marker"
    }
    fn on_markdown(&self, event: kiters_core::MarkdownEvent<'_>) -> anyhow::Result<()> {
        event.content.body.push_str(&format!("\n\n{}", self.0));
        Ok(())
    }
}

/// A hook that takes a target out of the site.
struct NoAbout;

impl Hook for NoAbout {
    fn name(&self) -> &str {
        "no-about"
    }
    fn on_target(&self, event: kiters_core::TargetEvent<'_>) -> anyhow::Result<()> {
        if event.target.url.contains("about") {
            *event.skip = true;
        }
        Ok(())
    }
}

/// A hook that stamps every finished page.
struct Stamp;

impl Hook for Stamp {
    fn name(&self) -> &str {
        "stamp"
    }
    fn on_page(&self, event: kiters_core::PageEvent<'_>) -> anyhow::Result<()> {
        event.page.html.push_str("<!-- stamped -->");
        Ok(())
    }
}

#[test]
fn hooks_rewrite_markdown_veto_targets_and_stain_pages() {
    let builder = SiteBuilder::new(fixture())
        .no_write()
        .hook(Marker("MARKED-BY-HOOK"))
        .hook(NoAbout)
        .hook(Stamp);
    let (_site, plan, pages, _report) = build_site(&builder).unwrap();

    // The markdown hook's words reached the reader.
    let hello = pages.iter().find(|p| p.url == "/posts/hello-world/").unwrap();
    assert!(hello.html.contains("MARKED-BY-HOOK"), "markdown hook left no trace");

    // The veto took the about page out of the plan whole.
    assert!(!plan.targets.iter().any(|t| t.url.contains("about")), "vetoed target stood");
    assert!(!pages.iter().any(|p| p.url.contains("about")));

    // The page hook stained every page, the vetoed one least of all.
    assert!(pages.iter().all(|p| p.html.contains("<!-- stamped -->")));
}

/// A hook that fails takes the build with it, named.
struct Rebellious;

impl Hook for Rebellious {
    fn name(&self) -> &str {
        "rebel"
    }
    fn on_target(&self, _event: kiters_core::TargetEvent<'_>) -> anyhow::Result<()> {
        anyhow::bail!("no")
    }
}

#[test]
fn a_failing_hook_fails_the_build_by_name() {
    let builder = SiteBuilder::new(fixture()).no_write().hook(Rebellious);
    let err = build_site(&builder).unwrap_err();
    let said = err.to_string();
    assert!(said.contains("rebel"), "error did not name the hook: {said}");
    assert!(said.contains("target"), "error did not name the moment: {said}");
}

/// The events give what they promise: a target carries its item, a page
/// its address.
#[test]
fn events_carry_their_subjects() {
    use std::sync::Mutex;

    static SEEN: Mutex<Vec<String>> = Mutex::new(Vec::new());

    struct Watch;
    impl Hook for Watch {
        fn on_page(&self, event: kiters_core::PageEvent<'_>) -> anyhow::Result<()> {
            let Page { url, status, .. } = &*event.page;
            SEEN.lock().unwrap().push(format!("{status} {url}"));
            Ok(())
        }
        fn on_target(&self, event: kiters_core::TargetEvent<'_>) -> anyhow::Result<()> {
            let Target { url, content, .. } = &*event.target;
            let has_item = content.is_some();
            SEEN.lock().unwrap().push(format!("{url} item={has_item}"));
            Ok(())
        }
    }

    let builder = SiteBuilder::new(fixture()).no_write().hook(Watch);
    build_site(&builder).unwrap();

    let seen = SEEN.lock().unwrap().clone();
    assert!(seen.iter().any(|s| s.starts_with("/posts/hello-world/ item=true")));
    assert!(seen.iter().any(|s| s.contains("200 /posts/hello-world/")));
    assert!(seen.iter().any(|s| s.contains("404 /404.html")));
}
