//! kiters-wasm — a plugin, loaded whole, run in a sandbox, hooked in.
//!
//! A plugin is a WebAssembly component speaking the `kiters:plugin`
//! world (see `wit/kiters.wit`). It is compiled once and instantiated
//! once; every moment of the build walks across the boundary, fuel
//! metered — a plugin that runs in circles runs out of fuel and fails
//! the build by name, not by hanging it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use kiters_core::{
    BuildEvent, Content, DiscoverEvent, FrontMatterEvent, Hook, HtmlEvent, MarkdownEvent, PageEvent, Status,
    Target, TargetEvent, TargetKind,
};
use wasmtime::component::{bindgen, Component, Linker, ResourceTable};
use wasmtime::{Config, Engine, Store};
use wasmtime_wasi::{WasiCtx, WasiCtxView, WasiView};

bindgen!({ path: "wit", world: "plugin" });

/// What the plugin's store carries: WASI, and nothing in it — no
/// files, no clock, no randomness. A plugin is a transform, not a
/// process; the outside world is not its to touch.
pub struct PluginState {
    wasi: WasiCtx,
    table: ResourceTable,
}

impl WasiView for PluginState {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView { ctx: &mut self.wasi, table: &mut self.table }
    }
}

/// How much fuel one call may burn: enough for any honest transform,
/// not enough for a loop that never ends.
const CALL_FUEL: u64 = 2_000_000_000;

/// The plugin, loaded and instantiated. Safe to share: one call at a
/// time, in the order the build walks the chain.
pub struct WasmPlugin {
    name: String,
    /// One engine for every plugin of the process: the caches in it
    /// are shared, the instantiations are not.
    engine: Arc<Engine>,
    /// The compiled component — shared so a rebuild reuses it whole.
    component: Arc<Component>,
    instance: Mutex<Option<Instance>>,
}

struct Instance {
    store: Store<PluginState>,
    bindings: Plugin,
}

impl WasmPlugin {
    /// Compiles a plugin from a `.wasm` component file. The name it
    /// errors under is the file's, without the end.
    pub fn load(engine: &Arc<Engine>, path: &Path) -> anyhow::Result<Self> {
        let component = Component::from_file(engine.as_ref(), path)
            .map_err(|err| anyhow::anyhow!("{}: {}", path.display(), err))?;
        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        Ok(WasmPlugin { name, engine: engine.clone(), component: Arc::new(component), instance: Mutex::new(None) })
    }

    /// An engine shared by every plugin of the process.
    pub fn engine() -> anyhow::Result<Arc<Engine>> {
        let mut config = Config::new();
        config.consume_fuel(true);
        Ok(Arc::new(Engine::new(&config)?))
    }

    /// The instance, made on first use and kept; every call refuels
    /// before it crosses the boundary.
    fn with_instance<T>(&self, call: impl FnOnce(&mut Store<PluginState>, &Plugin) -> Result<T, wasmtime::Error>) -> anyhow::Result<T> {
        let mut guard = self.instance.lock().expect("plugin instance poisoned");
        if guard.is_none() {
            // WASI answers what the guest's own runtime asks for, and
            // nothing more; the world itself imports nothing.
            let mut linker: Linker<PluginState> = Linker::new(&self.engine);
            wasmtime_wasi::p2::add_to_linker_sync(&mut linker)
                .map_err(|err| anyhow::anyhow!("link wasi: {err}"))?;
            let wasi = WasiCtx::builder().build();
            let mut store =
                Store::new(&self.engine, PluginState { wasi, table: ResourceTable::new() });
            // Instantiating burns fuel too — the meter runs from before
            // the first breath.
            store.set_fuel(CALL_FUEL)?;
            let bindings = Plugin::instantiate(&mut store, &self.component, &linker)
                .map_err(|err| anyhow::anyhow!("instantiate: {err}"))?;
            *guard = Some(Instance { store, bindings });
        }
        let instance = guard.as_mut().expect("instance just made");
        instance.store.set_fuel(CALL_FUEL).map_err(|err| anyhow::anyhow!("fuel: {err}"))?;
        call(&mut instance.store, &instance.bindings)
            .map_err(|err| anyhow::anyhow!("trapped or failed: {err}"))
    }
}

impl Hook for WasmPlugin {
    fn name(&self) -> &str {
        &self.name
    }

    fn on_discover(&self, event: DiscoverEvent<'_>) -> anyhow::Result<()> {
        let path = event.path.to_string_lossy().into_owned();
        let kind = format!("{:?}", event.kind).to_lowercase();
        if self.with_instance(|store, plugin| plugin.call_on_discover(store, &path, &kind))? {
            *event.skip = true;
        }
        Ok(())
    }

    fn on_front_matter(&self, event: FrontMatterEvent<'_>) -> anyhow::Result<()> {
        let before = item_of(event.content);
        let after = self.with_instance(|store, plugin| plugin.call_on_front_matter(store, &before))?;
        write_back(event.content, &after);
        Ok(())
    }

    fn on_markdown(&self, event: MarkdownEvent<'_>) -> anyhow::Result<()> {
        let url = event.content.url();
        let body = event.content.body.clone();
        let rewritten = self.with_instance(|store, plugin| plugin.call_on_markdown(store, &url, &body))?;
        event.content.body = rewritten;
        Ok(())
    }

    fn on_target(&self, event: TargetEvent<'_>) -> anyhow::Result<()> {
        let before = planned_of(event.target);
        match self.with_instance(|store, plugin| plugin.call_on_target(store, &before))? {
            Outcome::Skip => *event.skip = true,
            Outcome::Keep(kept) => {
                event.target.url = kept.url;
                event.target.out = PathBuf::from(kept.out);
                if let Some(kind) = kind_from(&kept.kind) {
                    event.target.kind = kind;
                }
            }
        }
        Ok(())
    }

    fn on_html(&self, event: HtmlEvent<'_>) -> anyhow::Result<()> {
        let html = event.html.clone();
        let rewritten = self.with_instance(|store, plugin| plugin.call_on_html(store, event.url, &html))?;
        *event.html = rewritten;
        Ok(())
    }

    fn on_page(&self, event: PageEvent<'_>) -> anyhow::Result<()> {
        let before = Page {
            url: event.page.url.clone(),
            status: event.page.status,
            html: event.page.html.clone(),
        };
        let after = self.with_instance(|store, plugin| plugin.call_on_page(store, &before))?;
        event.page.url = after.url;
        event.page.status = after.status;
        event.page.html = after.html;
        Ok(())
    }

    fn on_build_complete(&self, event: BuildEvent<'_>) -> anyhow::Result<()> {
        let report = Report {
            items: event.report.items as u32,
            drafts: event.report.drafts as u32,
            rendered: event.report.rendered as u32,
            written: event.report.written as u32,
            took_ms: event.report.took.as_millis() as u32,
        };
        self.with_instance(|store, plugin| plugin.call_on_build_complete(store, report))
    }
}

/// Everything the event hands out, in the order the world speaks it.
fn item_of(content: &Content) -> Item {
    Item {
        id: content.slug.clone(),
        title: content.front.title.clone(),
        url: content.url(),
        body: content.body.clone(),
        status: content.status().as_str().to_string(),
        date: content.front.date.map(|d| d.to_rfc3339()),
        aliases: content.front.aliases.clone(),
        taxonomies: content
            .front
            .taxonomies
            .iter()
            .map(|(name, terms)| kiters::plugin::types::Taxonomy { name: name.clone(), terms: terms.clone() })
            .collect(),
    }
}

/// The item the plugin returned, written back where it came from. Only
/// the fields the world carries are written; the rest are the build's.
fn write_back(content: &mut Content, item: &Item) {
    content.front.title = item.title.clone();
    content.body = item.body.clone();
    content.front.date = item.date.as_deref().and_then(|d| chrono::DateTime::parse_from_rfc3339(d).ok()).map(|d| d.with_timezone(&chrono::Utc));
    content.front.aliases = item.aliases.clone();
    let mut taxonomies = BTreeMap::new();
    for tax in &item.taxonomies {
        taxonomies.insert(tax.name.clone(), tax.terms.clone());
    }
    content.front.taxonomies = taxonomies;
    let parsed = match item.status.as_str() {
        "published" => Some(Status::Published),
        "draft" => Some(Status::Draft),
        "scheduled" => Some(Status::Scheduled),
        "archived" => Some(Status::Archived),
        _ => None,
    };
    // A status the world does not speak leaves the item as it was.
    if let Some(status) = parsed {
        content.front.status = status;
    }
}

fn planned_of(target: &Target) -> Planned {
    Planned {
        url: target.url.clone(),
        out: target.out.display().to_string(),
        kind: kind_to(&target.kind),
    }
}

fn kind_to(kind: &TargetKind) -> String {
    match kind {
        TargetKind::Item => "item".into(),
        TargetKind::Home => "home".into(),
        TargetKind::NotFound => "not-found".into(),
        TargetKind::Alias { .. } => "alias".into(),
        TargetKind::Taxonomy(name) => format!("taxonomy:{name}"),
        TargetKind::Term { taxonomy, term } => format!("term:{taxonomy}/{term}"),
        TargetKind::Extra(name) => format!("extra:{name}"),
    }
}

fn kind_from(kind: &str) -> Option<TargetKind> {
    match kind {
        "item" => Some(TargetKind::Item),
        "home" => Some(TargetKind::Home),
        "not-found" => Some(TargetKind::NotFound),
        "alias" => Some(TargetKind::Alias { to: String::new() }),
        _ => {
            if let Some(name) = kind.strip_prefix("taxonomy:") {
                Some(TargetKind::Taxonomy(name.to_string()))
            } else if let Some(rest) = kind.strip_prefix("term:") {
                let (taxonomy, term) = rest.split_once('/')?;
                Some(TargetKind::Term { taxonomy: taxonomy.to_string(), term: term.to_string() })
            } else {
                // `extra` targets are the site's own (the sitemap, the
                // feed), fixed names, not a plugin's to invent.
                None
            }
        }
    }
}

/// Loads every plugin in a directory (`.wasm` files, sorted by name),
/// one engine shared among them.
pub fn load_dir(engine: &Arc<Engine>, dir: &Path) -> anyhow::Result<Vec<Arc<WasmPlugin>>> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "wasm"))
        .collect();
    paths.sort();
    paths.iter().map(|p| WasmPlugin::load(engine, p).map(Arc::new)).collect()
}
