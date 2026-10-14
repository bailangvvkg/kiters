//! kiters-serve — a site, served from a whole, swapped whole.

pub mod snapshot;
pub mod watch;

use std::net::SocketAddr;
use std::sync::Arc;

use arc_swap::ArcSwap;
use axum::body::Body;
use axum::extract::State;
use axum::http::{header, HeaderValue, Request, Response, StatusCode};
use axum::Router;
use kiters_build::SiteBuilder;
use tower_http::compression::CompressionLayer;
use tower_http::compression::predicate::SizeAbove;
use tower_http::set_header::SetResponseHeaderLayer;

pub use snapshot::Snapshot;
pub use watch::watch_and_rebuild;

/// The server's whole state: the snapshot every reader reads, swapped
/// whole by the watcher.
pub struct Served {
    pub snapshot: Arc<ArcSwap<Snapshot>>,
}

/// An app: any address in, the page the snapshot holds for it out, and
/// the snapshot's own not-found page for the rest. Pages are
/// pre-rendered whole — every answer is the same answer, so every
/// answer says where it came from: `x-kiters: snapshot`.
pub fn app(served: Arc<Served>) -> Router {
    Router::new()
        .fallback(route)
        .with_state(served)
        .layer(
            CompressionLayer::new()
                .br(true)
                .zstd(true)
                .gzip(true)
                // Nothing under a kilobyte is worth carrying compressed —
                // the smaller answer says more than the savings.
                .compress_when(SizeAbove::new(1024))
        )
        // A page is of the moment the snapshot is; the browser must
        // never be left holding what the site was. The snapshot is the
        // cache, and it empties the only way the site changes: by being
        // rebuilt.
        .layer(SetResponseHeaderLayer::overriding(header::CACHE_CONTROL, HeaderValue::from_static("no-store")))
}

async fn route(State(served): State<Arc<Served>>, request: Request<Body>) -> Response<Body> {
    // The path alone is the address: a query string is not a page, and
    // pages pre-rendered whole have no per-asking answer to vary — a
    // flood of ?x= is a flood of the same page.
    let snapshot = served.snapshot.load_full();
    let path = request.uri().path().to_string();
    let page = snapshot.get(&path);
    let (status, body) = match page {
        Some(page) => (StatusCode::OK, page.html.clone()),
        None => (StatusCode::NOT_FOUND, snapshot.not_found().html.clone()),
    };
    let mut response = Response::new(Body::from(body));
    *response.status_mut() = status;
    response
        .headers_mut()
        .insert("x-kiters", HeaderValue::from_static("snapshot"));
    if path.ends_with(".xml") {
        response
            .headers_mut()
            .insert(header::CONTENT_TYPE, HeaderValue::from_static("application/xml; charset=utf-8"));
    } else {
        response
            .headers_mut()
            .insert(header::CONTENT_TYPE, HeaderValue::from_static("text/html; charset=utf-8"));
    }
    response
}

/// Loads every plugin named, one engine shared among them. No paths,
/// no plugins — the ordinary build of an ordinary site.
pub fn load_plugins(paths: &[std::path::PathBuf]) -> anyhow::Result<Vec<Arc<kiters_wasm::WasmPlugin>>> {
    if paths.is_empty() {
        return Ok(Vec::new());
    }
    let engine = kiters_wasm::WasmPlugin::engine()?;
    paths.iter().map(|p| kiters_wasm::WasmPlugin::load(&engine, p).map(Arc::new)).collect()
}

/// Serves the site, rebuilding on change, until the process stops.
///
/// Every reader holds one snapshot whole for the length of one answer;
/// a rebuild swaps in a new one whole. There is no invalidation to get
/// wrong — the old snapshot dies with its last reader. Plugins ride
/// along through every rebuild, loaded once, run on every page.
pub async fn serve(
    root: std::path::PathBuf,
    addr: SocketAddr,
    with_watch: bool,
    with_drafts: bool,
    plugins: Vec<std::path::PathBuf>,
) -> anyhow::Result<()> {
    let hooks = load_plugins(&plugins)?;
    let mut builder = SiteBuilder::new(&root);
    for hook in &hooks {
        builder = builder.hook_shared(hook.clone());
    }
    let snapshot = Arc::new(ArcSwap::from_pointee(Snapshot::from_builder(&mut builder, with_drafts)?));
    let served = Arc::new(Served { snapshot: snapshot.clone() });

    let listener = tokio::net::TcpListener::bind(addr).await?;
    eprintln!(
        "kiters serve 在 {} ({} 页, watch {}, 插件 {})",
        addr,
        served.snapshot.load().pages_count(),
        if with_watch { "开" } else { "关" },
        hooks.len()
    );

    if with_watch {
        watch_and_rebuild(root.clone(), with_drafts, hooks, move |rebuilt| {
            snapshot.store(Arc::new(rebuilt));
        })?;
    }

    let app = app(served);
    axum::serve(listener, app).await?;
    Ok(())
}
