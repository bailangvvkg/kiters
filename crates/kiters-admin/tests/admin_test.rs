//! The admin API, said end to end: a login, a write, a conflict, a
//! preview, a zip — and nothing for anyone without the password.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use tower::ServiceExt;

/// The headers of a logged-in writer.
type Authed = Vec<(&'static str, String)>;

fn make_site() -> tempfile::TempDir {
    let site = tempfile::tempdir().unwrap();
    std::fs::write(site.path().join("kite.yaml"), "site:\n  title: 管理站\n  language: zh-CN\n").unwrap();
    site
}

fn make_app(of: &tempfile::TempDir) -> Router {
    kiters_admin::router(of.path(), "sekrit")
}

async fn call(app: &Router, method: &str, uri: &str, headers: &[(&str, String)], body: Option<Vec<u8>>) -> (StatusCode, String, Vec<u8>) {
    let mut builder = Request::builder().method(method).uri(uri);
    for (name, value) in headers {
        builder = builder.header(*name, value);
    }
    let body = body.map(Body::from).unwrap_or_else(|| Body::empty());
    let response = app.clone().oneshot(builder.body(body).unwrap()).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes().to_vec();
    (status, String::from_utf8_lossy(&bytes).into_owned(), bytes)
}

/// A wrong password refused, the right one answered with its cookie.
async fn login(app: &Router) -> Authed {
    let (status, _, _) = call(app, "POST", "/api/login", &[("content-type", "application/json".to_string())], Some(r#"{"password":"wrong"}"#.as_bytes().to_vec())).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "a wrong password logged in");

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/login")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"password":"sekrit"}"#.as_bytes().to_vec()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let cookie = response
        .headers()
        .get("set-cookie")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(';').next())
        .expect("login set no cookie")
        .to_string();
    vec![("cookie", cookie), ("content-type", "application/json".to_string())]
}

fn with_if_match(authed: &[(&'static str, String)], revision: &str) -> Vec<(&'static str, String)> {
    let mut headers: Vec<(&'static str, String)> = authed.to_vec();
    headers.push(("if-match", revision.to_string()));
    headers
}

async fn create(app: &Router, authed: &[(&'static str, String)], slug: &str) -> String {
    let item = format!(r#"{{"kind":"posts","slug":"{slug}","title":"{slug}","status":"draft","body":"一"}}"#);
    let (status, body, _) = call(app, "POST", "/api/contents", authed, Some(item.into_bytes())).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    serde_json::from_str::<serde_json::Value>(&body).unwrap()["revision"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn login_gates_everything() {
    let site = make_site();
    let app = make_app(&site);
    let authed = login(&app).await;

    let (status, body, _) = call(&app, "GET", "/api/session", &authed, None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("admin"), "{body}");

    // Without the cookie: nothing. The tree keeps to itself.
    for (method, uri) in [("GET", "/api/contents"), ("GET", "/api/contents/posts:x"), ("GET", "/api/export")] {
        let (status, _, _) = call(&app, method, uri, &[], None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{method} {uri} answered without a session");
    }
}

#[tokio::test]
async fn the_life_of_an_item() {
    let site = make_site();
    let app = make_app(&site);
    let authed = login(&app).await;

    // Create: kind said, the file is the truth.
    let revision = create(&app, &authed, "first").await;
    let raw = std::fs::read_to_string(site.path().join("content/posts/first/index.md")).unwrap();
    assert!(raw.contains("title: first"), "{raw}");

    // Read it back.
    let (status, body, _) = call(&app, "GET", "/api/contents/posts:first", &authed, None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body.contains(&revision), "{body}");

    // List sees it.
    let (status, body, _) = call(&app, "GET", "/api/contents?kind=posts", &authed, None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body.contains("posts:first"), "{body}");

    // An update must say what it believes it is writing over.
    let item = r#"{"kind":"posts","title":"first","status":"published","body":"二"}"#;
    let (status, _, _) = call(&app, "PUT", "/api/contents/posts:first", &authed, Some(item.as_bytes().to_vec())).await;
    assert_eq!(status, StatusCode::PRECONDITION_REQUIRED);

    // The honest update: If-Match says the revision we read.
    let honest = with_if_match(&authed, &revision);
    let (status, body, _) = call(&app, "PUT", "/api/contents/posts:first", &honest, Some(item.as_bytes().to_vec())).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let next = serde_json::from_str::<serde_json::Value>(&body).unwrap()["revision"].as_str().unwrap().to_string();
    assert_ne!(next, revision, "the revision did not move");

    // The file says what the winner wrote.
    let raw = std::fs::read_to_string(site.path().join("content/posts/first/index.md")).unwrap();
    assert!(raw.contains("二") && raw.contains("published"), "{raw}");

    // The old revision no longer opens the door.
    let (status, body, _) = call(&app, "PUT", "/api/contents/posts:first", &honest, Some(item.as_bytes().to_vec())).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(body.contains("current_revision"), "{body}");
}

#[tokio::test]
async fn if_match_gates_the_second_writer() {
    let site = make_site();
    let app = make_app(&site);
    let authed = login(&app).await;
    let revision = create(&app, &authed, "race").await;

    // Two writers, both believing the same revision. Exactly one wins;
    // the other is told what is true now.
    let one_headers = with_if_match(&authed, &revision);
    let two_headers = with_if_match(&authed, &revision);
    let app_one = app.clone();
    let app_two = app.clone();
    let first = tokio::spawn(async move {
        call(&app_one, "PUT", "/api/contents/posts:race", &one_headers, Some(r#"{"kind":"posts","title":"race","status":"draft","body":"乙"}"#.as_bytes().to_vec())).await
    });
    let second = tokio::spawn(async move {
        call(&app_two, "PUT", "/api/contents/posts:race", &two_headers, Some(r#"{"kind":"posts","title":"race","status":"draft","body":"丙"}"#.as_bytes().to_vec())).await
    });
    let one = first.await.unwrap();
    let two = second.await.unwrap();
    let outcomes = [one.0, two.0];
    assert!(
        outcomes.iter().filter(|s| **s == StatusCode::OK).count() == 1
            && outcomes.iter().filter(|s| **s == StatusCode::CONFLICT).count() == 1,
        "the race did not resolve: {outcomes:?}"
    );

    // The tree holds the winner's body, and only it.
    let raw = std::fs::read_to_string(site.path().join("content/posts/race/index.md")).unwrap();
    assert!(raw.contains("乙") ^ raw.contains("丙"), "{raw}");
}

#[tokio::test]
async fn preview_renders_and_forgets() {
    let site = make_site();
    let app = make_app(&site);
    let authed = login(&app).await;
    let (status, body, _) = call(
        &app,
        "POST",
        "/api/preview",
        &authed,
        Some(r##"{"body":"# 标题\n\n{{< note 注意 >}}短代码也算{{< /note >}}"}"##.as_bytes().to_vec()),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body.contains("<h1>"), "{body}");
    assert!(body.contains("Note: 注意"), "{body}");
}

#[tokio::test]
async fn export_then_import_round_trip() {
    let site = make_site();
    std::fs::create_dir_all(site.path().join("content/posts/orig")).unwrap();
    std::fs::write(
        site.path().join("content/posts/orig/index.md"),
        "---\ntitle: 原件\nstatus: published\n---\n\n原文\n",
    )
    .unwrap();

    let app = make_app(&site);
    let authed = login(&app).await;
    let (status, _, zip) = call(&app, "GET", "/api/export", &authed, None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(zip.starts_with(b"PK"), "not a zip");

    // Into a second site: the zip says what the first site said.
    let other = make_site();
    let app2 = make_app(&other);
    let authed2 = login(&app2).await;
    let mut headers = authed2.clone();
    headers.push(("content-type", "application/zip".to_string()));
    let (status, body, _) = call(&app2, "POST", "/api/import", &headers, Some(zip)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let value: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert!(value["written"].as_u64().unwrap() >= 1, "{value}");
    let raw = std::fs::read_to_string(other.path().join("content/posts/orig/index.md")).unwrap();
    assert!(raw.contains("原件"), "{raw}");
}

#[tokio::test]
async fn delete_takes_the_item_out_revision_first() {
    let site = make_site();
    let app = make_app(&site);
    let authed = login(&app).await;
    let revision = create(&app, &authed, "gone").await;

    // A stale belief does not delete anything.
    let stale = with_if_match(&authed, "0000000000000000");
    let (status, _, _) = call(&app, "DELETE", "/api/contents/posts:gone", &stale, None).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(site.path().join("content/posts/gone").exists());

    let honest = with_if_match(&authed, &revision);
    let (status, _, _) = call(&app, "DELETE", "/api/contents/posts:gone", &honest, None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(!site.path().join("content/posts/gone").exists());
}

#[tokio::test]
async fn media_round_trip_and_no_traveling() {
    let site = make_site();
    let app = make_app(&site);
    let authed = login(&app).await;

    let boundary = "k1t3rs";
    let png: &str = "\u{89}PNG-not-really";
    let body = format!(
        "--{boundary}\r\ncontent-disposition: form-data; name=\"file\"; filename=\"tu.png\"\r\ncontent-type: image/png\r\n\r\n{png}\r\n--{boundary}--\r\n"
    );
    let mut headers: Vec<(&'static str, String)> = authed
        .iter()
        .filter(|(name, _)| *name == "cookie")
        .map(|(name, value)| (*name, value.clone()))
        .collect();
    headers.push(("content-type", format!("multipart/form-data; boundary={boundary}")));
    let (status, body, _) = call(&app, "POST", "/api/media", &headers, Some(body.into_bytes())).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let url = serde_json::from_str::<serde_json::Value>(&body).unwrap()["url"].as_str().unwrap().to_string();
    assert!(url.ends_with(".png"), "{url}");

    // The media answers, as itself.
    let (status, _, bytes) = call(&app, "GET", &url, &authed, None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(!bytes.is_empty(), "the bytes did not come back");

    // And no one travels anywhere.
    let (status, _, _) = call(&app, "GET", "/media/..%2f..%2fkite.yaml", &authed, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "traveled out of the media tree");
}

#[tokio::test]
async fn kind_is_not_optional_on_create() {
    let site = make_site();
    let app = make_app(&site);
    let authed = login(&app).await;
    let (status, body, _) = call(
        &app,
        "POST",
        "/api/contents",
        &authed,
        Some(r#"{"slug":"nokind","title":"无类","body":"x"}"#.as_bytes().to_vec()),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(body.contains("kind"), "{body}");
}
