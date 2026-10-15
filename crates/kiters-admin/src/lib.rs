//! kiters-admin — the API a studio speaks, on top of a file tree.
//!
//! One password; content items addressed `kind:slug` under `content/`;
//! every write gated by the revision the writer believes in (If-Match),
//! the second writer answered 409 with what is true now. Previews
//! render one body with no store at all. Export and import carry the
//! whole content tree as one zip.

pub mod session;
pub mod store;

use std::sync::Arc;

use axum::body::Body;
use axum::extract::{FromRequestParts, Multipart, Path, Query, State};
use axum::http::{header, HeaderMap, Request, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;

use session::Sessions;
use store::{ContentStore, WriteError};

/// The whole admin, held once: the site's root and the password's key.
pub struct Admin {
    pub store: ContentStore,
    pub sessions: Sessions,
}

/// The admin's router, mounted under `/api` (and media at `/media`).
pub fn router(root: impl Into<std::path::PathBuf>, password: &str) -> Router {
    let state = Arc::new(Admin { store: ContentStore::new(root), sessions: Sessions::new(password) });
    Router::new()
        .route("/api/login", post(login))
        .route("/api/logout", post(logout))
        .route("/api/session", get(session_info))
        .route("/api/contents", get(list).post(create))
        .route("/api/contents/{id}", get(read_one).put(update).delete(remove))
        .route("/api/media", post(upload))
        .route("/api/preview", post(preview))
        .route("/api/export", get(export))
        .route("/api/import", post(import))
        .route("/media/{*path}", get(media_file))
        .route("/admin", get(studio))
        .with_state(state)
}

/// The studio's one page: a login, a list, an editor, a preview. It is
/// the API wearing a face, nothing more — every action it offers is one
/// the API already speaks.
async fn studio() -> Response {
    (
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        [(header::CACHE_CONTROL, "no-store")],
        STUDIO_HTML,
    )
        .into_response()
}

const STUDIO_HTML: &str = include_str!("studio.html");

/// A request that carried a standing session. Everything past login
/// requires one; none of it is anyone's without it.
struct Authed;

impl<S> FromRequestParts<S> for Authed
where
    Arc<Admin>: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut axum::http::request::Parts, state: &S) -> Result<Self, Self::Rejection> {
        let admin = Arc::<Admin>::from_ref(state);
        let cookies = parts.headers.get(header::COOKIE).and_then(|c| c.to_str().ok()).unwrap_or("");
        if admin.sessions.verify(cookies) {
            Ok(Authed)
        } else {
            Err((StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error": "not logged in"}))).into_response())
        }
    }
}

use axum::extract::FromRef;

fn json_status(code: StatusCode, value: serde_json::Value) -> Response {
    (code, Json(value)).into_response()
}

// --- the moments ---

#[derive(Deserialize)]
struct Login {
    password: String,
}

async fn login(State(admin): State<Arc<Admin>>, Json(ask): Json<Login>) -> Response {
    if admin.sessions.password_matches(&ask.password) {
        let mut response = json_status(StatusCode::OK, serde_json::json!({"ok": true}));
        response
            .headers_mut()
            .insert(header::SET_COOKIE, admin.sessions.cookie().parse().unwrap());
        return response;
    }
    json_status(StatusCode::UNAUTHORIZED, serde_json::json!({"error": "wrong password"}))
}

async fn logout(State(admin): State<Arc<Admin>>) -> Response {
    let mut response = json_status(StatusCode::OK, serde_json::json!({"ok": true}));
    response
        .headers_mut()
        .insert(header::SET_COOKIE, admin.sessions.gone_cookie().parse().unwrap());
    response
}

async fn session_info(_auth: Authed) -> Response {
    json_status(StatusCode::OK, serde_json::json!({"who": "admin"}))
}

#[derive(Deserialize)]
struct ListQuery {
    kind: Option<String>,
    cursor: Option<String>,
    limit: Option<usize>,
}

async fn list(_auth: Authed, State(admin): State<Arc<Admin>>, Query(q): Query<ListQuery>) -> Response {
    match admin.store.list(q.kind.as_deref(), q.cursor.as_deref(), q.limit.unwrap_or(20).max(1)) {
        Ok((items, next)) => json_status(
            StatusCode::OK,
            serde_json::json!({"items": items, "next_cursor": next}),
        ),
        Err(err) => json_status(StatusCode::INTERNAL_SERVER_ERROR, serde_json::json!({"error": err.to_string()})),
    }
}

async fn read_one(_auth: Authed, State(admin): State<Arc<Admin>>, Path(id): Path<String>) -> Response {
    match admin.store.read_raw(&id) {
        Some((raw, revision)) => {
            let item = store::parse_raw(&raw);
            json_status(
                StatusCode::OK,
                serde_json::json!({
                    "id": id,
                    "raw": raw,
                    "revision": revision,
                    "front": {
                        "title": item.front.title,
                        "status": item.front.status.as_str(),
                        "date": item.front.date.map(|d| d.to_rfc3339()),
                        "cover": item.front.cover,
                        "tags": item.front.taxonomies.get("tags").cloned().unwrap_or_default(),
                    "aliases": item.front.aliases,
                    },
                    "body": item.body,
                }),
            )
        }
        None => json_status(StatusCode::NOT_FOUND, serde_json::json!({"error": "no such item"})),
    }
}

#[derive(Deserialize)]
struct WriteItem {
    /// The kind is not optional: a PUT without it is a 400, the same
    /// contract the kite studio learned the hard way.
    kind: Option<String>,
    slug: Option<String>,
    #[serde(default)]
    title: String,
    #[serde(default = "default_status")]
    status: String,
    #[serde(default)]
    date: Option<String>,
    #[serde(default)]
    cover: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    /// The addresses an item answered under before — a save that does
    /// not carry them takes them away, and every old link dies.
    #[serde(default)]
    aliases: Vec<String>,
    #[serde(default)]
    body: String,
}

fn default_status() -> String {
    "draft".into()
}

/// A date as the studio says it: whole, or a plain `2026-01-01` that
/// means the midnight it starts at — the same reading a file gets.
fn parse_a_date(said: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    if let Ok(whole) = chrono::DateTime::parse_from_rfc3339(said.trim()) {
        return Some(whole.with_timezone(&chrono::Utc));
    }
    chrono::NaiveDate::parse_from_str(said.trim(), "%Y-%m-%d")
        .ok()
        .and_then(|day| day.and_hms_opt(0, 0, 0))
        .map(|midnight| chrono::DateTime::from_naive_utc_and_offset(midnight, chrono::Utc))
}

async fn create(_auth: Authed, State(admin): State<Arc<Admin>>, Json(item): Json<WriteItem>) -> Response {
    let Some(kind) = item.kind.clone() else {
        return json_status(StatusCode::BAD_REQUEST, serde_json::json!({"error": "kind is required"}));
    };
    let slug = item.slug.clone().unwrap_or_else(|| format!("{}-{}", slug_word(&item.title), chrono::Utc::now().timestamp() % 100000));
    match write(&admin, &kind, &slug, &item, None) {
        Ok(revision) => json_status(
            StatusCode::CREATED,
            serde_json::json!({"id": format!("{kind}:{slug}"), "revision": revision}),
        ),
        Err(response) => response,
    }
}

async fn update(
    _auth: Authed,
    State(admin): State<Arc<Admin>>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(item): Json<WriteItem>,
) -> Response {
    // An update must say what it believes it is writing over.
    let Some(expect) = headers.get("if-match").and_then(|v| v.to_str().ok()) else {
        return json_status(StatusCode::PRECONDITION_REQUIRED, serde_json::json!({"error": "if-match is required"}));
    };
    let expect = expect.trim().trim_matches('"');
    let (kind, slug) = match ContentStore::split_id(&id) {
        Ok(split) => split,
        Err(WriteError::BadName(name)) => {
            return json_status(StatusCode::BAD_REQUEST, serde_json::json!({"error": format!("bad id: {name}")}))
        }
        Err(_) => unreachable!("split_id only says BadName"),
    };
    // The kind must be said, and it must be the kind the item is: the
    // tree is the address, and an update does not move it.
    match item.kind.as_deref() {
        None => return json_status(StatusCode::BAD_REQUEST, serde_json::json!({"error": "kind is required"})),
        Some(said) if said != kind => {
            return json_status(
                StatusCode::BAD_REQUEST,
                serde_json::json!({"error": "kind does not match the item's address"}),
            )
        }
        Some(_) => {}
    }
    match write(&admin, &kind, &slug, &item, Some(expect)) {
        Ok(revision) => json_status(StatusCode::OK, serde_json::json!({"id": id, "revision": revision})),
        Err(response) => response,
    }
}

fn write(admin: &Arc<Admin>, kind: &str, slug: &str, item: &WriteItem, expect: Option<&str>) -> Result<String, Response> {
    let mut front = kiters_core::FrontMatter {
        title: item.title.clone(),
        status: match item.status.as_str() {
            "published" => kiters_core::Status::Published,
            "scheduled" => kiters_core::Status::Scheduled,
            "archived" => kiters_core::Status::Archived,
            _ => kiters_core::Status::Draft,
        },
        date: item.date.as_deref().and_then(parse_a_date).map(|d| d.with_timezone(&chrono::Utc)),
        aliases: item.aliases.clone(),
        cover: item.cover.clone(),
        ..Default::default()
    };
    if !item.tags.is_empty() {
        front.taxonomies.insert("tags".to_string(), item.tags.clone());
    }
    match admin.store.write(kind, slug, &front, &item.body, expect) {
        Ok(revision) => Ok(revision),
        Err(WriteError::Conflict(now)) => Err(json_status(
            StatusCode::CONFLICT,
            serde_json::json!({"error": "someone wrote first", "current_revision": now}),
        )),
        Err(WriteError::Exists) => Err(json_status(StatusCode::CONFLICT, serde_json::json!({"error": "already exists"}))),
        Err(WriteError::Missing) => Err(json_status(StatusCode::NOT_FOUND, serde_json::json!({"error": "no such item"}))),
        Err(WriteError::BadName(name)) => Err(json_status(StatusCode::BAD_REQUEST, serde_json::json!({"error": format!("bad name: {name}")}))),
        Err(WriteError::Io(err)) => Err(json_status(StatusCode::INTERNAL_SERVER_ERROR, serde_json::json!({"error": err.to_string()}))),
    }
}

async fn remove(_auth: Authed, State(admin): State<Arc<Admin>>, Path(id): Path<String>, headers: HeaderMap) -> Response {
    let Some(expect) = headers.get("if-match").and_then(|v| v.to_str().ok()).map(|v| v.trim().trim_matches('"')) else {
        return json_status(StatusCode::PRECONDITION_REQUIRED, serde_json::json!({"error": "if-match is required"}));
    };
    match admin.store.delete(&id, expect) {
        Ok(()) => (StatusCode::NO_CONTENT, ()).into_response(),
        Err(WriteError::Conflict(now)) => json_status(
            StatusCode::CONFLICT,
            serde_json::json!({"error": "someone wrote first", "current_revision": now}),
        ),
        Err(WriteError::Missing) => json_status(StatusCode::NOT_FOUND, serde_json::json!({"error": "no such item"})),
        Err(WriteError::BadName(name)) => json_status(StatusCode::BAD_REQUEST, serde_json::json!({"error": format!("bad name: {name}")})),
        Err(WriteError::Io(err)) => json_status(StatusCode::INTERNAL_SERVER_ERROR, serde_json::json!({"error": err.to_string()})),
        Err(WriteError::Exists) => unreachable!("delete never says exists"),
    }
}

// --- media ---

async fn upload(_auth: Authed, State(admin): State<Arc<Admin>>, mut multipart: Multipart) -> Response {
    while let Ok(Some(field)) = multipart.next_field().await {
        let Some(name) = field.file_name().map(|n| n.to_string()) else { continue };
        let Ok(bytes) = field.bytes().await else { continue };
        if bytes.is_empty() {
            continue;
        }
        let clean = media_name(&name);
        let year = chrono::Utc::now().format("%Y");
        let stamp = chrono::Utc::now().timestamp_millis();
        let file = admin.store.root().join("content/media").join(year.to_string()).join(format!("{stamp}-{clean}"));
        if let Some(dir) = file.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Err(err) = std::fs::write(&file, &bytes) {
            return json_status(StatusCode::INTERNAL_SERVER_ERROR, serde_json::json!({"error": err.to_string()}));
        }
        let url = format!("/media/{year}/{stamp}-{clean}");
        return json_status(StatusCode::CREATED, serde_json::json!({"url": url}));
    }
    json_status(StatusCode::BAD_REQUEST, serde_json::json!({"error": "no file in the request"}))
}

async fn media_file(_auth: Authed, State(admin): State<Arc<Admin>>, Path(path): Path<String>) -> Response {
    // The media tree is its own: only names the tree would hold, no
    // traveling anywhere.
    if path.split('/').any(|part| !is_media_part(part)) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let file = admin.store.root().join("content/media").join(&path);
    match std::fs::read(&file) {
        Ok(bytes) => {
            let mut response = Response::new(Body::from(bytes));
            response
                .headers_mut()
                .insert(header::CONTENT_TYPE, media_type(&path).parse().unwrap());
            response
                .headers_mut()
                .insert(header::CACHE_CONTROL, "public, max-age=31536000, immutable".parse().unwrap());
            response
        }
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

fn is_media_part(part: &str) -> bool {
    !part.is_empty()
        && part != "."
        && part != ".."
        && part.bytes().all(|b| matches!(b, b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'%'))
}

fn media_name(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or("file");
    let cleaned: String = base
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') { c } else { '-' })
        .collect();
    let trimmed = cleaned.trim_matches('-');
    if trimmed.is_empty() { "file".into() } else { trimmed.chars().take(80).collect() }
}

fn media_type(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("") {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "avif" => "image/avif",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "mp3" => "audio/mpeg",
        "pdf" => "application/pdf",
        _ => "application/octet-stream",
    }
}

// --- preview ---

#[derive(Deserialize)]
struct Preview {
    body: String,
}

async fn preview(_auth: Authed, Json(ask): Json<Preview>) -> Response {
    // Rendered, said, forgotten: a preview is of the moment.
    let html = kiters_render::markdown_to_html(
        &kiters_render::fold_shortcodes(&ask.body),
        kiters_render::RenderOptions::default(),
    );
    let mut response = json_status(StatusCode::OK, serde_json::json!({"html": html}));
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    response
}

// --- export and import ---

async fn export(_auth: Authed, State(admin): State<Arc<Admin>>) -> Response {
    let mut zip_bytes = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut zip_bytes);
        let options: zip::write::SimpleFileOptions = zip::write::SimpleFileOptions::default();
        let content = admin.store.root().join("content");
        let mut files = Vec::new();
        collect_files(&content, &content, &mut files);
        for (name, bytes) in files {
            let _ = zip.start_file(name, options);
            let _ = std::io::Write::write_all(&mut zip, &bytes);
        }
        let _ = zip.finish();
    }
    let bytes = zip_bytes.into_inner();
    let mut response = Response::new(Body::from(bytes));
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, "application/zip".parse().unwrap());
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    response
}

fn collect_files(dir: &std::path::Path, base: &std::path::Path, out: &mut Vec<(String, Vec<u8>)>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, base, out);
        } else if let Ok(rel) = path.strip_prefix(base) {
            let name = rel.to_string_lossy().replace('\\', "/");
            if let Ok(bytes) = std::fs::read(&path) {
                out.push((name, bytes));
            }
        }
    }
}

async fn import(_auth: Authed, State(admin): State<Arc<Admin>>, request: Request<Body>) -> Response {
    let Ok(bytes) = axum::body::to_bytes(request.into_body(), 64 * 1024 * 1024).await else {
        return json_status(StatusCode::BAD_REQUEST, serde_json::json!({"error": "no body"}));
    };
    let cursor = std::io::Cursor::new(bytes.to_vec());
    let mut archive = match zip::ZipArchive::new(cursor) {
        Ok(archive) => archive,
        Err(err) => return json_status(StatusCode::BAD_REQUEST, serde_json::json!({"error": format!("not a zip: {err}")})),
    };
    let content = admin.store.root().join("content");
    let mut written = 0usize;
    for i in 0..archive.len() {
        let Ok(mut file) = archive.by_index(i) else { continue };
        let Some(name) = file.enclosed_name().map(|p| p.to_string_lossy().replace('\\', "/")) else { continue };
        if name.ends_with('/') {
            continue;
        }
        if !file.is_file() {
            continue;
        }
        let mut bytes = Vec::new();
        if std::io::Read::read_to_end(&mut file, &mut bytes).is_err() {
            continue;
        }
        let out = content.join(&name);
        if !out.starts_with(&content) {
            continue;
        }
        if let Some(dir) = out.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if std::fs::write(&out, bytes).is_ok() {
            written += 1;
        }
    }
    json_status(StatusCode::OK, serde_json::json!({"written": written}))
}

fn slug_word(title: &str) -> String {
    let cleaned: String = title
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect();
    let trimmed = cleaned.trim_matches('-');
    if trimmed.is_empty() { "item".into() } else { trimmed.chars().take(40).collect() }
}
