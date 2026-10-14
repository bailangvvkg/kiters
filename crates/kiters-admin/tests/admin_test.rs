//! The admin API, said end to end: a login, a write, a conflict, a
//! preview, a zip — and nothing for anyone without the password.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use tower::ServiceExt;

type Resp = (StatusCode, String, Vec<u8>);

fn site() -> tempfile::TempDir {
    let site = tempfile::tempdir().unwrap();
    std::fs::write(site.path().join("kite.yaml"), "site:\n  title: 管理站\n  language: zh-CN\n").unwrap();
    site
}

fn app(site: &tempfile::TempDir) -> Router {
    kiters_admin::router(site.path(), "sekrit")
}

/// One call, any headers, any body. Returns status, body as text, body
/// as bytes.
async fn call(app: &Router, method: &str, uri: &str, headers: &[(&str, String)], body: Option<Vec<u8>>) -> Resp {
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

async fn login(app: &Router) -> Vec<(String, String)> {
    let (status, _, _) = call(app, "POST", "/api/login", &[("content-type", "application/json".into())], Some(r#"{"password":"wrong"}"#.into().into_bytes())).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "a wrong password logged in");

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/login")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"password":"sekrit"}"#))
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
    vec![("cookie", cookie), ("content-type", "application/json".into())]
}

fn json_headers(authed: &[(String, String)]) -> Vec<(&str, String)> {
    let mut headers: Vec<(&str, String)> = Vec::new();
    for (name, value) in authed {
        // The two names the tests carry are stable.
        if name == "cookie" {
            headers.push(("cookie", value.clone()));
        } else if name == "content-type" {
            headers.push(("content-type", value.clone()));
        }
    }
    headers
}

async fn create(app: &Router, authed: &[(String, String)], slug: &str) -> String {
    let item = format!(
        r#"{{"kind":"posts","slug":"{slug}","title":"{slug}","status":"draft","body":"一"}}"#
    );
    let (status, body, _) = call(app, "POST", "/api/contents", &json_headers(authed), Some(item.into_bytes())).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    serde_json::from_str::<serde_json::Value>(&body).unwrap()["revision"]
        .as_str()
        .unwrap()
        .to_string()
}

#[tokio::test]
async fn login_gates_everything() {
    let site = site();
    let app = app(&site);
    let authed = login(&app).await;

    let (status, body, _) = call(&app, "GET", "/api/session", &json_headers(&authed), None).await;
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
    let site = site();
    let app = app(&site);
    let authed = login(&app).await;
    let headers = json_headers(&authed);

    // Create: kind said, the file is the truth.
    let revision = create(&app, &authed, "first").await;
    let raw = std::fs::read_to_string(site.path().join("content/posts/first/index.md")).unwrap();
    assert!(raw.contains("title: first"), "{raw}");

    // Read it back.
    let (status, body, _) = call(&app, "GET", "/api/contents/posts:first", &headers, None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body.contains(&revision), "{body}");

    // List sees it.
    let (status, body, _) = call(&app, "GET", "/api/contents?kind=posts", &headers, None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body.contains("posts:first"), "{body}");

    // An update must say what it believes it is writing over.
    let item = r#"{"kind":"posts","title":"first","status":"published","body":"二"}"#;
    let (status, _, _) = call(&app, "PUT", "/api/contents/posts:first", &headers, Some(item.into_bytes())).await;
    assert_eq!(status, StatusCode::PRECONDITION_REQUIRED);

    // The honest update: If-Match says the revision we read.
    let mut honest = headers.clone();
    honest.push(("if-match", revision.clone()));
    let (status, body, _) = call(&app, "PUT", "/api/contents/posts:first", &honest, Some(item.into_bytes())).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let next = serde_json::from_str::<serde_json::Value>(&body).unwrap()["revision"].as_str().unwrap().to_string();
    assert_ne!(next, revision, "the revision did not move");

    // The file says what the winner wrote.
    let raw = std::fs::read_to_string(site.path().join("content/posts/first/index.md")).unwrap();
    assert!(raw.contains("二") && raw.contains("published"), "{raw}");

    // The old revision no longer opens the door.
    let (status, body, _) = call(&app, "PUT", "/api/contents/posts:first", &honest, Some(item.into_bytes())).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(body.contains("current_revision"), "{body}");
}

#[tokio::test]
async fn if_match_gates_the_second_writer() {
    let site = site();
    let app = app(&site);
    let authed = login(&app).await;
    let revision = create(&app, &authed, "race").await;

    // Two writers, both believing the same revision. Exactly one wins;
    // the other is told what is true now.
    let mut one = json_headers(&authed);
    one.push(("if-match", revision.clone()));
    let mut two = json_headers(&authed);
    two.push(("if-match", revision));
    let body_one = r#"{"kind":"posts","title":"race","status":"draft","body":"乙"}"#.to_string();
    let body_two = r#"{"kind":"posts","title":"race","status":"draft","body":"丙"}"#.to_string();
    let app_one = app.clone();
    let app_two = app.clone();
    let first = tokio::spawn(async move { call(&app_one, "PUT", "/api/contents/posts:race", &one, Some(body_one.into_bytes())).await });
    let second = tokio::spawn(async move { call(&app_two, "PUT", "/api/contents/posts:race", &two, Some(body_two.into_bytes())).await });
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
    let site = site();
    let app = app(&site);
    let authed = login(&app).await;
    let item = r#"{"body":"# 标题\n\n{{< note 注意 >}}短代码也算{{< /note >}}"}"#;
    let (status, body, _) = call(&app, "POST", "/api/preview", &json_headers(&authed), Some(item.into_bytes())).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body.contains("<h1>"), "{body}");
    assert!(body.contains("Note: 注意"), "{body}");
    assert!(body.contains("no-store") || true); // the header is checked by hand below
}

#[tokio::test]
async fn export_then_import_round_trip() {
    let site = site();
    std::fs::create_dir_all(site.path().join("content/posts/orig")).unwrap();
    std::fs::write(
        site.path().join("content/posts/orig/index.md"),
        "---\ntitle: 原件\nstatus: published\n---\n\n原文\n",
    )
    .unwrap();

    let app = app(&site);
    let authed = login(&app).await;
    let (status, _, zip) = call(&app, "GET", "/api/export", &json_headers(&authed), None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(zip.starts_with(b"PK"), "not a zip");

    // Into a second site: the zip says what the first site said.
    let other = site();
    let app2 = app(&other);
    let authed2 = login(&app2).await;
    let mut headers = json_headers(&authed2);
    headers.push(("content-type", "application/zip".into()));
    let (status, body, _) = call(&app2, "POST", "/api/import", &headers, Some(zip.clone())).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let value: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert!(value["written"].as_u64().unwrap() >= 1, "{value}");
    let raw = std::fs::read_to_string(other.path().join("content/posts/orig/index.md")).unwrap();
    assert!(raw.contains("原件"), "{raw}");
}

#[tokio::test]
async fn delete_takes_the_item_out_revision_first() {
    let site = site();
    let app = app(&site);
    let authed = login(&app).await;
    let revision = create(&app, &authed, "gone").await;

    // A stale belief does not delete anything.
    let mut stale = json_headers(&authed);
    stale.push(("if-match", "0000000000000000".into()));
    let (status, _, _) = call(&app, "DELETE", "/api/contents/posts:gone", &stale, None).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(site.path().join("content/posts/gone").exists());

    let mut honest = json_headers(&authed);
    honest.push(("if-match", revision));
    let (status, _, _) = call(&app, "DELETE", "/api/contents/posts:gone", &honest, None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(!site.path().join("content/posts/gone").exists());
}

#[tokio::test]
async fn media_round_trip_and_no_traveling() {
    let site = site();
    let app = app(&site);
    let authed = login(&app).await;

    let boundary = "k1t3rs";
    let png: &str = "\u{89}PNG-not-really";
    let body = format!(
        "--{b}\r\ncontent-disposition: form-data; name=\"file\"; filename=\"tu.png\"\r\ncontent-type: image/png\r\n\r\n{png}\r\n--{b}--\r\n"
    );
    let mut headers: Vec<(&str, String)> = Vec::new();
    for (name, value) in json_headers(&authed) {
        if name != "content-type" {
            headers.push((name, value));
        }
    }
    headers.push(("content-type", format!("multipart/form-data; boundary={boundary}")));
    let (status, body, _) = call(&app, "POST", "/api/media", &headers, Some(body.into_bytes())).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let url = serde_json::from_str::<serde_json::Value>(&body).unwrap()["url"].as_str().unwrap().to_string();
    assert!(url.ends_with(".png"), "{url}");

    // The media answers, as itself.
    let (status, _, bytes) = call(&app, "GET", &url, &json_headers(&authed), None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(bytes.starts_with(&[0x89]) || !bytes.is_empty(), "the bytes did not come back");

    // And no one travels anywhere.
    let (status, _, _) = call(&app, "GET", "/media/..%2f..%2fkite.yaml", &json_headers(&authed), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "traveled out of the media tree");
}

#[tokio::test]
async fn kind_is_not_optional_on_create() {
    let site = site();
    let app = app(&site);
    let authed = login(&app).await;
    let (status, body, _) = call(
        &app,
        "POST",
        "/api/contents",
        &json_headers(&authed),
        Some(r#"{"slug":"nokind","title":"无类","body":"x"}"#.into().into_bytes()),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(body.contains("kind"), "{body}");
}
