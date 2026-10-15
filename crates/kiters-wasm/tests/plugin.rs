//! Plugins across the boundary, proven end to end: load a component,
//! hook it into a build, and check what the build says because of it.

use std::path::{Path, PathBuf};

use kiters_build::{build_site, SiteBuilder};
use kiters_core::Hook;
use kiters_wasm::WasmPlugin;

/// Where the CI leaves the compiled guests — `target/wasm32-wasip2/
/// debug/<name>.wasm`, next to the workspace root.
fn guest(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/wasm32-wasip2/debug")
        .join(format!("{}.wasm", name.replace('-', "_")))
        .canonicalize()
        .unwrap_or_else(|_| panic!("{name}.wasm not built — run: cargo build -p {name} --target wasm32-wasip2"))
}

fn fixture() -> &'static Path {
    Box::leak(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixture")
            .canonicalize()
            .unwrap()
            .into_boxed_path(),
    )
}

fn engine() -> std::sync::Arc<wasmtime::Engine> {
    WasmPlugin::engine().unwrap()
}

#[test]
fn stamp_marks_every_page() {
    let engine = engine();
    let plugin = WasmPlugin::load(&engine, &guest("stamp-wasm")).unwrap();
    let out = tempfile::tempdir().unwrap();
    let builder = SiteBuilder::new(fixture()).out(out.path()).hook(plugin);
    let (_site, _plan, pages, _report) = build_site(&builder).unwrap();
    assert!(!pages.is_empty());
    assert!(pages.iter().all(|p| p.html.contains("stamped by stamp-wasm")), "some page went unstamped");
}

#[test]
fn skipper_takes_its_targets_out() {
    let engine = engine();
    let plugin = WasmPlugin::load(&engine, &guest("skipper-wasm")).unwrap();
    let out = tempfile::tempdir().unwrap();
    // A site with a page the skipper cannot abide.
    let site = tempfile::tempdir().unwrap();
    for from in ["hello-world", "byebye-post"] {
        let dir = site.path().join("content/posts").join(from);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("index.md"), format!("---\ntitle: {from}\nstatus: published\n---\n\n正文\n")).unwrap();
    }
    std::fs::write(site.path().join("kite.yaml"), "site:\n  title: 插件站\n  language: zh-CN\n").unwrap();

    let builder = SiteBuilder::new(site.path()).out(out.path()).hook(plugin);
    let (_site, _plan, pages, _report) = build_site(&builder).unwrap();
    assert!(pages.iter().any(|p| p.url.contains("hello-world")), "the kept page vanished");
    assert!(!pages.iter().any(|p| p.url.contains("byebye")), "the vetoed page stood");
}

#[test]
fn linkcard_draws_a_card_from_a_code_block() {
    let engine = engine();
    let plugin = WasmPlugin::load(&engine, &guest("linkcard-lite")).unwrap();

    // What a card's block says the card wears; what it leaves out the
    // card asks for with data-need, for card.js to fill in.
    let html = "<html><head><title>x</title></head><body><main><pre><code class=\"language-card\">url: https://kite.zipimg.cn/\n标题: 风筝\ndesc: a small engine\n</code></pre></main></body></html>";
    let mut said = html.to_string();
    let url = "/posts/test/";
    plugin
        .on_html(kiters_core::HtmlEvent { url: &url, html: &mut said })
        .unwrap();

    assert!(said.contains("<a class=\"kite-card\" data-size=\"regular\""), "no card: {said}");
    assert!(said.contains("href=\"https://kite.zipimg.cn/\""), "the address lost: {said}");
    assert!(said.contains("data-need=\"icon image\""), "needs said wrong: {said}");
    assert!(said.contains(">风筝</strong>"), "the Chinese title lost: {said}");
    assert!(said.contains("<span class=\"kc-desc\">a small engine</span>"), "desc lost: {said}");
    assert!(said.contains("<span class=\"kc-host\">kite.zipimg.cn</span>"), "host lost: {said}");
    assert!(!said.contains("language-card"), "the block stayed: {said}");
    // The page dresses: one stylesheet at the head, one script at the
    // body's end.
    assert_eq!(said.matches("<style>").count(), 1, "style said more than once");
    assert_eq!(said.matches("Linkcard: fills in").count(), 1, "script said more than once");
    assert!(said.find("<style>").unwrap() < said.find("</head>").unwrap(), "style not at head");
    assert!(said.find("window.KiteLinkCard").unwrap() > said.find("</main>").unwrap(), "script not at body's end");
}

#[test]
fn linkcard_draws_a_moved_halo_tag() {
    let engine = engine();
    let plugin = WasmPlugin::load(&engine, &guest("linkcard-lite")).unwrap();

    // A post moved from Halo carries its cards as hyperlink-card
    // tags; a renderer that scrubs unknown HTML leaves them escaped,
    // and they still stand for cards. The inline one is the small
    // card.
    let url = "/posts/test/";
    let html = "<html><body><p>&lt;hyperlink-card href=\"https://blog.zipimg.cn/archives/1\" custom-title=\"一篇文章\"&gt;&lt;a&gt;x&lt;/a&gt;&lt;/hyperlink-card&gt;</p></body></html>";
    let mut said = html.to_string();
    plugin
        .on_html(kiters_core::HtmlEvent { url: &url, html: &mut said })
        .unwrap();

    assert!(said.contains("data-size=\"regular\""), "not the regular card: {said}");
    assert!(said.contains("href=\"https://blog.zipimg.cn/archives/1\""), "address lost: {said}");
    assert!(said.contains(">一篇文章</strong>"), "custom title lost: {said}");
    assert!(!said.contains("hyperlink-card"), "the tag stayed: {said}");

    let inline = "<html><body>&lt;hyperlink-inline-card href=\"https://example.com/\"&gt;x&lt;/hyperlink-inline-card&gt;</body></html>";
    let mut said = inline.to_string();
    plugin
        .on_html(kiters_core::HtmlEvent { url: &url, html: &mut said })
        .unwrap();
    assert!(said.contains("data-size=\"small\""), "not the small card: {said}");
}

#[test]
fn linkcard_moves_a_halo_tag_into_a_card_block() {
    let engine = engine();
    let plugin = WasmPlugin::load(&engine, &guest("linkcard-lite")).unwrap();

    // A renderer that does not trust raw HTML omits the tag whole; so
    // the tag becomes a card code block while it is still writing, and
    // rides the block's own road to a card.
    let writing = "前文

<hyperlink-card href=\"https://blog.zipimg.cn/archives/1\" custom-title=\"一篇文章\"></hyperlink-card>

后文

<hyperlink-inline-card href=\"https://example.com/\"></hyperlink-inline-card>
";
    let mut content = kiters_core::Content {
        kind: kiters_core::Kind::new("post"),
        slug: "test".to_string(),
        front: Default::default(),
        body: writing.to_string(),
        source: std::path::PathBuf::from("index.md"),
    };
    plugin
        .on_markdown(kiters_core::MarkdownEvent { content: &mut content })
        .unwrap();
    let said = content.body;

    assert!(said.contains("```card
url: https://blog.zipimg.cn/archives/1
title: 一篇文章
```"), "the tag did not become a block: {said}");
    assert!(said.contains("url: https://example.com/
size: small"), "the inline tag is not the small card: {said}");
    assert!(!said.contains("hyperlink-card"), "a tag stayed in the writing: {said}");
    assert!(said.starts_with("前文"), "the writing around was touched: {said}");
    assert!(said.contains("

后文"), "the prose after was torn: {said}");
}

#[test]
fn linkcard_folds_a_bare_link_and_nothing_else() {
    let engine = engine();
    let plugin = WasmPlugin::load(&engine, &guest("linkcard-lite")).unwrap();

    // The same cases kite's linkcard folds, said here for the parity
    // both engines owe each other: a bare link alone in a paragraph
    // becomes one card; prose around links, and links inside prose,
    // stay as they are.
    let url = "/posts/test/";
    let prose = "<html><body><p>开头</p>\n<p><a href=\"https://kite.zipimg.cn/\">https://kite.zipimg.cn/</a></p>\n<p>看看<a href=\"https://example.com/x\">这个</a>链接</p>\n<p><a href=\"https://dmoe.cc/random.php\">https://dmoe.cc/random.php</a></p></body></html>";
    let mut html = prose.to_string();
    plugin
        .on_html(kiters_core::HtmlEvent { url: &url, html: &mut html })
        .unwrap();

    assert!(
        html.contains("<a class=\"kite-card\" data-size=\"regular\" href=\"https://kite.zipimg.cn/\""),
        "the bare link did not become a card: {html}"
    );
    assert!(html.contains("<span class=\"kc-host\">dmoe.cc</span>"), "the second card missing: {html}");
    assert!(html.contains("<p>开头</p>"), "prose around was touched: {html}");
    assert!(html.contains("看看<a href=\"https://example.com/x\">这个</a>链接"), "a link inside prose was folded: {html}");
    assert!(html.contains("<strong class=\"kc-title\" data-fill=\"title\">kite.zipimg.cn</strong>"), "fill slot missing: {html}");
    // The card a page does not have dresses nothing.
    let plain = "<html><body><p>只是文字</p></body></html>";
    let mut said = plain.to_string();
    plugin
        .on_html(kiters_core::HtmlEvent { url: &url, html: &mut said })
        .unwrap();
    assert_eq!(said, plain, "a page without cards was dressed anyway");
}

#[test]
fn a_kept_alias_keeps_its_target() {
    let engine = engine();
    // A plugin that keeps a target keeps it whole: the alias's own
    // address rides the wire beside the kind's name, and comes back.
    let plugin = WasmPlugin::load(&engine, &guest("linkcard-lite")).unwrap();
    let mut target = kiters_core::Target {
        url: "/posts/old/".to_string(),
        out: std::path::PathBuf::from("posts/old/index.html"),
        kind: kiters_core::TargetKind::Alias { to: "/posts/new-home/".to_string() },
        content: None,
    };
    let mut skip = false;
    plugin
        .on_target(kiters_core::TargetEvent { target: &mut target, skip: &mut skip })
        .unwrap();
    assert!(!skip, "a keep skipped");
    match target.kind {
        kiters_core::TargetKind::Alias { to } => assert_eq!(to, "/posts/new-home/", "the alias lost where it points"),
        other => panic!("the kind changed shape: {other:?}"),
    }
}

#[test]
fn a_broken_plugin_fails_the_build_by_name_not_by_hanging() {
    // Not a component at all: loading it fails, and the failure says
    // the file.
    let junk = tempfile::tempdir().unwrap();
    let file = junk.path().join("junk.wasm");
    std::fs::write(&file, b"not wasm at all").unwrap();
    let engine = engine();
    let err = match WasmPlugin::load(&engine, &file) {
        Err(err) => err,
        Ok(_) => panic!("junk wasm loaded as a plugin?"),
    };
    assert!(err.to_string().contains("junk.wasm"), "failure did not name the file: {err}");
}
