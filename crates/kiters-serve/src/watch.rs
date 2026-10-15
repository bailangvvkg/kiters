//! Watching the site, rebuilding it whole when it changes.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use notify::{Event, RecursiveMode, Watcher};

use crate::Snapshot;
use kiters_build::SiteBuilder;
use kiters_wasm::WasmPlugin;

/// Watches the site and rebuilds it whole on change, handing each whole
/// rebuild to `swap`. Events land quietly: a burst of saves is one
/// rebuild, for the site is rebuilt when the typing stops, not while it
/// goes on.
///
/// A scheduled item waiting in the future is watched by the clock: the
/// nearest due time is waited out and the site rebuilt when it passes,
/// so a scheduled page stands without anyone touching a thing. An idle
/// minute with nothing due rebuilds nothing.
///
/// Plugins ride along: loaded once before the first build, run on every
/// page of every rebuild.
pub fn watch_and_rebuild(
    root: PathBuf,
    options: crate::ServeOptions,
    swap: impl Fn(Snapshot) + Send + 'static,
) -> anyhow::Result<()> {
    let plugins = crate::load_plugins(&options.plugins)?;
    let (tx, rx) = std::sync::mpsc::channel::<Event>();
    let mut watcher = notify::recommended_watcher(move |result: Result<Event, _>| {
        if let Ok(event) = result {
            let _ = tx.send(event);
        }
    })?;
    // The theme is watched too, whole: a template touched is the site
    // re-said, without a restart asked.
    for dir in [root.join("content"), root.join("themes"), root.clone()] {
        if dir.is_dir() {
            let mode = if dir.ends_with("themes") { RecursiveMode::Recursive } else { RecursiveMode::NonRecursive };
            watcher.watch(&dir, mode)?;
        }
    }

    // One builder config, many builds: the plugins live in it, shared
    // whole from rebuild to rebuild.
    let builder = Mutex::new(build_config(&root, &options.theme, options.highlight, &plugins));
    let root_for_tick = root.clone();

    thread::spawn(move || {
        // The watcher lives as long as the thread: dropped here, it is
        // dropped when the watching ends.
        let _watcher = watcher;
        let mut waiting_for: Option<chrono::DateTime<chrono::Utc>> = None;
        let mut quiet_until: Option<Instant> = None;
        let mut last_theme = theme_stamp(&root_for_tick);
        loop {
            // The rebuild arm: the site says what it now is, and the
            // next due time is taken from what it said.
            match rebuild(&builder, options.with_drafts) {
                Ok(snapshot) => {
                    waiting_for = snapshot.next_due();
                    swap(snapshot);
                }
                Err(err) => eprintln!("重建失败: {err}"),
            }

            // The clock arm: wait for a change, or for the nearest due
            // time to pass — whichever comes first, and never longer
            // than the clock is willing to be checked at.
            let until_due = waiting_for
                .and_then(|at| (at + chrono::Duration::seconds(1)).signed_duration_since(chrono::Utc::now()).to_std().ok())
                .unwrap_or(Duration::from_secs(3600));
            let timeout = until_due.min(Duration::from_secs(20));

            let mut changed = false;
            match rx.recv_timeout(timeout) {
                Ok(event) if relevant(&event) => changed = true,
                Ok(_) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    let due_passed = waiting_for.is_some_and(|at| at <= chrono::Utc::now());
                    if !due_passed {
                        // The clock ticked on a quiet site — but a theme
                        // replaced under a standing watch (a directory
                        // removed and unpacked again) leaves inotify
                        // pointing at nothing. The stamp catches what
                        // the events miss.
                        let seen = theme_stamp(&root_for_tick);
                        if seen != last_theme {
                            last_theme = seen;
                            changed = true;
                        } else {
                            continue;
                        }
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
            }
            if changed {
                quiet_until = Some(Instant::now() + Duration::from_millis(150));
            }
            // A burst of saves is one rebuild: wait the typing out.
            while let Some(until) = quiet_until {
                let now = Instant::now();
                if now < until {
                    let _ = rx.recv_timeout(until - now);
                    quiet_until = Some(Instant::now() + Duration::from_millis(150));
                } else {
                    quiet_until = None;
                }
            }
        }
    });
    Ok(())
}

/// The builder every rebuild runs with: the site's root and its
/// plugins, nothing else — drafts and output are each build's own.
fn build_config(root: &PathBuf, theme: &Option<String>, highlight: bool, plugins: &[Arc<WasmPlugin>]) -> SiteBuilder<'static> {
    let root: &'static std::path::Path = Box::leak(root.clone().into_boxed_path());
    let mut builder = SiteBuilder::new(root);
    builder.markdown.highlight = highlight;
    if let Some(name) = theme {
        builder = builder.theme(name);
    }
    for plugin in plugins {
        builder = builder.hook_shared(plugin.clone());
    }
    builder
}

/// One whole rebuild through the shared builder config.
fn rebuild(builder: &Mutex<SiteBuilder<'static>>, with_drafts: bool) -> anyhow::Result<Snapshot> {
    let mut builder = builder.lock().expect("builder poisoned");
    // The theme's templates are read once and cached; a rebuild wants
    // what the files say now, so the cache is emptied first.
    builder.layout.forget_templates();
    Snapshot::from_builder(&mut builder, with_drafts)
}

/// Whether an event is the site saying it changed — not a file merely
/// being read.
fn relevant(event: &Event) -> bool {
    use notify::EventKind::*;
    matches!(event.kind, Modify(_) | Create(_) | Remove(_))
}

/// The newest mtime under the themes tree — the watch's backstop,
/// compared on the clock. Cheap to ask: a few hundred files stated.
fn theme_stamp(root: &std::path::Path) -> Option<std::time::SystemTime> {
    let mut newest = None;
    let mut stack = vec![root.join("themes")];
    while let Some(dir) = stack.pop() {
        let Ok(read) = std::fs::read_dir(&dir) else { continue };
        for entry in read.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if let Ok(seen) = entry.metadata().and_then(|m| m.modified()) {
                if newest.map_or(true, |have| seen > have) {
                    newest = Some(seen);
                }
            }
        }
    }
    newest
}
