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
fn builds_the_whole_site_with_drafts_left_out() {
    let out = tempfile::tempdir().unwrap();
    let builder = SiteBuilder::new(fixture()).out(out.path());
    let (site, _plan, pages, report) = build_site(&builder).unwrap();

    assert_eq!(site.title, "我的站");
    assert_eq!(report.items, 3);
    assert_eq!(report.drafts, 1);

    // The public pages, the home, the feed, the sitemap, the 404.
    let urls: Vec<&str> = pages.iter().map(|p| p.url.as_str()).collect();
    assert!(urls.contains(&"/posts/hello-world/"), "urls: {urls:?}");
    assert!(urls.contains(&"/pages/about/"), "urls: {urls:?}");
    assert!(urls.contains(&"/"), "urls: {urls:?}");
    assert!(urls.contains(&"/sitemap.xml"), "urls: {urls:?}");
    assert!(urls.contains(&"/rss.xml"), "urls: {urls:?}");
    assert!(!urls.iter().any(|u| u.contains("miyue")), "a draft stood in the public site: {urls:?}");

    // Files on disk where the addresses say.
    let hello = std::fs::read_to_string(out.path().join("posts/hello-world/index.html")).unwrap();
    assert!(hello.contains("<strong>加粗</strong>"), "bold did not render");
    assert!(hello.contains("language-rust"), "code block carries no language class");
    assert!(hello.contains("<table>"), "table did not render");
    assert!(!out.path().join("posts/miyue-notes").exists(), "draft was written out");

    // The draft is in neither the sitemap nor the feed nor the home.
    let sitemap = std::fs::read_to_string(out.path().join("sitemap.xml")).unwrap();
    assert!(!sitemap.contains("miyue"), "draft in sitemap");
    let home = std::fs::read_to_string(out.path().join("index.html")).unwrap();
    assert!(home.contains("/posts/hello-world/"), "home does not list the post");
    assert!(!home.contains("miyue"), "draft on the home page");

    // The 404 answers with a page, not a bare status.
    let nf = std::fs::read_to_string(out.path().join("404.html")).unwrap();
    assert!(nf.contains("页面不存在"));
}
