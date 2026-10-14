//! A build of the fixture site, checked end to end.

use std::path::Path;

use kiters_build::{build_site, SiteBuilder};

fn fixture() -> &'static Path {
    Box::leak(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixture")
            .canonicalize()
            .unwrap()
            .into_boxed_path(),
    )
}

#[test]
fn builds_the_whole_site_with_drafts_and_scheduled_left_out() {
    let out = tempfile::tempdir().unwrap();
    let builder = SiteBuilder::new(fixture()).out(out.path());
    let (site, _plan, pages, report) = build_site(&builder).unwrap();

    assert_eq!(site.title, "我的站");
    assert_eq!(report.items, 5); // hello, long, about, draft, scheduled
    assert_eq!(report.drafts, 1);

    let urls: Vec<&str> = pages.iter().map(|p| p.url.as_str()).collect();
    for want in [
        "/posts/hello-world/",
        "/posts/long-post/",
        "/pages/about/",
        "/",
        "/sitemap.xml",
        "/rss.xml",
        "/404.html",
        "/posts/old-hello/",           // the alias
        "/tags/",                       // the taxonomy index
        "/tags/%E9%9A%8F%E7%AC%94/",    // a term page, encoded
    ] {
        assert!(urls.contains(&want), "missing {want}: {urls:?}");
    }
    assert!(!urls.iter().any(|u| u.contains("miyue")), "a draft stood in: {urls:?}");
    assert!(!urls.iter().any(|u| u.contains("scheduled")), "the future stood in: {urls:?}");

    // The markdown, the shortcode, the table, the code class.
    let hello = std::fs::read_to_string(out.path().join("posts/hello-world/index.html")).unwrap();
    assert!(hello.contains("<strong>加粗</strong>"), "bold did not render");
    assert!(hello.contains("language-rust"), "code block carries no language class");
    assert!(hello.contains("<table>"), "table did not render");
    assert!(hello.contains("<strong>Note: 入场</strong>"), "shortcode did not fold");

    // The alias page says where the item lives now.
    let alias = std::fs::read_to_string(out.path().join("posts/old-hello/index.html")).unwrap();
    assert!(alias.contains("/posts/hello-world/"), "alias does not point at the item");
    assert!(alias.contains("http-equiv=\"refresh\""), "alias carries no refresh");

    // The term page lists what carries the term.
    let term = std::fs::read_to_string(out.path().join("tags/%E9%9A%8F%E7%AC%94/index.html")).unwrap();
    assert!(term.contains("/posts/hello-world/"), "term page does not list the post");
    let index = std::fs::read_to_string(out.path().join("tags/index.html")).unwrap();
    assert!(index.contains("随笔"), "taxonomy index lacks its term");

    // Nowhere the draft or the scheduled item.
    let sitemap = std::fs::read_to_string(out.path().join("sitemap.xml")).unwrap();
    assert!(!sitemap.contains("miyue") && !sitemap.contains("scheduled"), "sitemap leaks");
    let home = std::fs::read_to_string(out.path().join("index.html")).unwrap();
    assert!(home.contains("/posts/hello-world/"), "home does not list the post");
    assert!(!home.contains("miyue"), "draft on the home page");

    // The 404 answers with a page, not a bare status.
    let nf = std::fs::read_to_string(out.path().join("404.html")).unwrap();
    assert!(nf.contains("页面不存在"));
}

#[test]
fn a_theme_wears_the_site_its_own_way() {
    let out = tempfile::tempdir().unwrap();
    let builder = SiteBuilder::new(fixture()).out(out.path()).theme("demo");
    let (_site, _plan, _pages, _report) = build_site(&builder).unwrap();

    // The theme's page skeleton answers.
    let hello = std::fs::read_to_string(out.path().join("posts/hello-world/index.html")).unwrap();
    assert!(hello.contains("demo-theme"), "the theme's skeleton is not worn: {hello}");
    assert!(hello.contains("Demo Theme"), "the theme's own header is missing");

    // The theme says the whole list page, with every item.
    let term = std::fs::read_to_string(out.path().join("tags/%E9%9A%8F%E7%AC%94/index.html")).unwrap();
    assert!(term.contains("demo-list"), "the theme's list is not worn");
    assert!(term.contains("/posts/hello-world/"), "the theme's list lost the item");

    // The not-found page is the theme's too.
    let nf = std::fs::read_to_string(out.path().join("404.html")).unwrap();
    assert!(nf.contains("demo-notfound"), "the theme's 404 is not worn");

    // What the theme does not say, the built-ins answer: the sitemap
    // still says the whole site.
    let sitemap = std::fs::read_to_string(out.path().join("sitemap.xml")).unwrap();
    assert!(sitemap.contains("<urlset"), "sitemap fell out from under the theme");
}

#[test]
fn with_drafts_the_draft_stands_for_a_preview() {
    let out = tempfile::tempdir().unwrap();
    let mut builder = SiteBuilder::new(fixture()).out(out.path());
    builder.plan.with_drafts = true;
    let (_site, _plan, _pages, report) = build_site(&builder).unwrap();
    assert!(out.path().join("posts/miyue-notes/index.html").is_file(), "draft not written");
    assert_eq!(report.rendered, 14); // the 12 standing pages, the draft, the scheduled — a preview shows all
}
