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
fn linkcard_lite_folds_a_bare_link_and_nothing_else() {
    let engine = engine();
    let plugin = WasmPlugin::load(&engine, &guest("linkcard-lite")).unwrap();

    // The same cases kite's linkcard folds, said here for the parity
    // both engines owe each other: a bare link alone in a paragraph
    // becomes one card; prose around links, and links inside prose,
    // stay as they are.
    let prose = "<p>开头</p>\n<p><a href=\"https://kite.zipimg.cn/\">https://kite.zipimg.cn/</a></p>\n<p>看看<a href=\"https://example.com/x\">这个</a>链接</p>\n<p><a href=\"https://dmoe.cc/random.php\">https://dmoe.cc/random.php</a></p>";
    let mut html = prose.to_string();
    let url = "/posts/test/";
    plugin
        .on_html(kiters_core::HtmlEvent { url: &url, html: &mut html })
        .unwrap();

    assert!(
        html.contains("<div class=\"linkcard\"><a class=\"lc-card\" href=\"https://kite.zipimg.cn/\" target=\"_blank\" rel=\"noopener\"><span class=\"lc-host\">kite.zipimg.cn</span></a></div>"),
        "the bare link did not become a card: {html}"
    );
    assert!(html.contains("<span class=\"lc-host\">dmoe.cc</span>"), "the second card missing: {html}");
    assert!(html.contains("<p>开头</p>"), "prose around was touched: {html}");
    assert!(html.contains("看看<a href=\"https://example.com/x\">这个</a>链接"), "a link inside prose was folded: {html}");
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
