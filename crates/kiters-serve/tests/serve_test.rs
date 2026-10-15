//! A served site: the snapshot answers, whole, and swaps whole.

use std::path::Path;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use kiters_serve::{app, Served, Snapshot};
use tower::ServiceExt;

fn fixture() -> &'static Path {
    Box::leak(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixture")
            .canonicalize()
            .unwrap()
            .into_boxed_path(),
    )
}

async fn get(served: &std::sync::Arc<Served>, uri: &str, accept: Option<&str>) -> (StatusCode, Vec<u8>, axum::http::HeaderMap) {
    let mut request = Request::builder().uri(uri);
    if let Some(enc) = accept {
        request = request.header("accept-encoding", enc);
    }
    let response = app(served.clone()).oneshot(request.body(Body::empty()).unwrap()).await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let body = response.into_body().collect().await.unwrap().to_bytes().to_vec();
    (status, body, headers)
}

fn served() -> std::sync::Arc<Served> {
    std::sync::Arc::new(Served {
        snapshot: std::sync::Arc::new(arc_swap::ArcSwap::from_pointee(
            Snapshot::build(fixture(), false).unwrap(),
        )),
    })
}

#[tokio::test]
async fn a_page_answers_whole_with_the_snapshot_headers() {
    let served = served();
    let (status, body, headers) = get(&served, "/posts/hello-world/", None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.windows(b"<strong>".len()).any(|w| w == b"<strong>") || String::from_utf8_lossy(&body).contains("加粗"));
    assert_eq!(headers.get("x-kiters").unwrap(), "snapshot");
    assert_eq!(headers.get("cache-control").unwrap(), "no-store");
    assert!(headers.get("content-type").unwrap().to_str().unwrap().starts_with("text/html"));
}

#[tokio::test]
async fn an_unknown_address_lands_on_the_rendered_not_found_page() {
    let served = served();
    let (status, body, headers) = get(&served, "/no/such/page/", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(String::from_utf8_lossy(&body).contains("页面不存在"));
    assert_eq!(headers.get("x-kiters").unwrap(), "snapshot");
}

#[tokio::test]
async fn a_query_is_not_a_page_every_flood_hits_the_same_whole() {
    let served = served();
    for uri in ["/posts/hello-world/?x=1", "/posts/hello-world/?utm=whatever", "/posts/hello-world/"] {
        let (status, body, _) = get(&served, uri, None).await;
        assert_eq!(status, StatusCode::OK, "{uri}");
        assert!(String::from_utf8_lossy(&body).contains("加粗"), "{uri} served the wrong page");
    }
}

#[tokio::test]
async fn an_alias_answers_as_a_page_that_says_where_the_item_lives() {
    let served = served();
    let (status, body, _) = get(&served, "/posts/old-hello/", None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(String::from_utf8_lossy(&body).contains("/posts/hello-world/"));
}

#[tokio::test]
async fn a_term_page_lists_what_carries_the_term() {
    let served = served();
    let (status, body, _) = get(&served, "/tags/%E9%9A%8F%E7%AC%94/", None).await;
    assert_eq!(status, StatusCode::OK);
    let body = String::from_utf8_lossy(&body);
    assert!(body.contains("/posts/hello-world/"), "term page does not list the item");
}

#[tokio::test]
async fn a_big_page_is_carried_compressed_a_small_one_is_not() {
    let served = served();
    let (status, _, headers) = get(&served, "/posts/long-post/", Some("gzip")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers.get("content-encoding").unwrap(), "gzip", "big page not compressed");
    assert_eq!(headers.get("vary").map(|v| v.to_str().unwrap()), Some("accept-encoding"));

    let (status, _, headers) = get(&served, "/pages/about/", Some("gzip")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(headers.get("content-encoding").is_none(), "small page compressed anyway");
}

#[tokio::test]
async fn the_sitemap_is_served_as_xml() {
    let served = served();
    let (status, body, headers) = get(&served, "/sitemap.xml", None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(String::from_utf8_lossy(&body).contains("<urlset"));
    assert!(headers.get("content-type").unwrap().to_str().unwrap().contains("xml"));
}

#[tokio::test]
async fn the_themes_static_answers_immutable() {
    let served = std::sync::Arc::new(Served {
        snapshot: std::sync::Arc::new(arc_swap::ArcSwap::from_pointee(
            Snapshot::from_builder(
                &mut kiters_build::SiteBuilder::new(fixture()).no_write().theme("demo"),
                false,
            )
            .unwrap(),
        )),
    });
    let (status, body, headers) = get(&served, "/demo/hat.css", None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(String::from_utf8_lossy(&body).contains("teal"));
    assert!(headers.get("content-type").unwrap().to_str().unwrap().contains("text/css"));
    assert!(headers.get("cache-control").unwrap().to_str().unwrap().contains("immutable"));
}

#[tokio::test]
async fn the_search_index_answers_as_json() {
    let served = served();
    let (status, body, headers) = get(&served, "/search.json", None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(String::from_utf8_lossy(&body).contains("/posts/hello-world/"));
    assert!(headers.get("content-type").unwrap().to_str().unwrap().contains("json"));
}

#[tokio::test]
async fn a_change_swaps_the_whole_site() {
    let site = tempfile::tempdir().unwrap();
    let content = site.path().join("content/posts/live");
    std::fs::create_dir_all(&content).unwrap();
    std::fs::write(site.path().join("kite.yaml"), "site:\n  title: 直播站\n  language: zh-CN\n").unwrap();
    std::fs::write(content.join("index.md"), "---\ntitle: 第一版\nstatus: published\n---\n\n第一版正文\n").unwrap();

    let snapshot = std::sync::Arc::new(arc_swap::ArcSwap::from_pointee(Snapshot::build(site.path(), false).unwrap()));
    let served = std::sync::Arc::new(Served { snapshot: snapshot.clone() });
    let (_, body, _) = get(&served, "/posts/live/", None).await;
    assert!(String::from_utf8_lossy(&body).contains("第一版正文"));

    // The watcher takes the site from here: every change rebuilds the
    // whole site and swaps it in whole.
    kiters_serve::watch_and_rebuild(site.path().to_path_buf(), Default::default(), {
        let snapshot = snapshot.clone();
        move |rebuilt| snapshot.store(std::sync::Arc::new(rebuilt))
    })
    .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(400)).await; // the watcher settles

    // Change the file; the watcher rebuilds and swaps whole.
    std::fs::write(content.join("index.md"), "---\ntitle: 第二版\nstatus: published\n---\n\n第二版正文\n").unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let (_, body, _) = get(&served, "/posts/live/", None).await;
        if String::from_utf8_lossy(&body).contains("第二版正文") {
            break;
        }
        if std::time::Instant::now() > deadline {
            panic!("the change never swapped in");
        }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
}
