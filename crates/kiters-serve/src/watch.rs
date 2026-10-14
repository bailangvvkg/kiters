//! Watching the site, rebuilding it whole when it changes.

use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use notify::{Event, RecursiveMode, Watcher};

use crate::Snapshot;

/// Watches the site and rebuilds it whole on change, handing each whole
/// rebuild to `swap`. Events land quietly: a burst of saves is one
/// rebuild, for the site is rebuilt when the typing stops, not while it
/// goes on.
///
/// A scheduled item waiting in the future is watched by the clock: the
/// nearest due time is waited out and the site rebuilt when it passes,
/// so a scheduled page stands without anyone touching a thing. An idle
/// minute with nothing due rebuilds nothing.
pub fn watch_and_rebuild(
    root: PathBuf,
    with_drafts: bool,
    swap: impl Fn(Snapshot) + Send + 'static,
) -> anyhow::Result<()> {
    let (tx, rx) = mpsc::channel::<Event>();
    let mut watcher = notify::recommended_watcher(move |result: Result<Event, _>| {
        if let Ok(event) = result {
            let _ = tx.send(event);
        }
    })?;
    for dir in [root.join("content"), root.clone()] {
        if dir.is_dir() {
            watcher.watch(&dir, RecursiveMode::NonRecursive)?;
        }
    }

    thread::spawn(move || {
        // The watcher lives as long as the thread: dropped here, it is
        // dropped when the watching ends.
        let _watcher = watcher;
        let mut waiting_for: Option<chrono::DateTime<chrono::Utc>> = None;
        let mut quiet_until: Option<Instant> = None;
        loop {
            // The rebuild arm: the site says what it now is, and the
            // next due time is taken from what it said.
            let rebuilt = Snapshot::build(&root, with_drafts);
            match rebuilt {
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
            let timeout = until_due.min(Duration::from_secs(60));

            let mut changed = false;
            match rx.recv_timeout(timeout) {
                Ok(event) if relevant(&event) => changed = true,
                Ok(_) => {}
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    let due_passed = waiting_for.is_some_and(|at| at <= chrono::Utc::now());
                    if !due_passed {
                        continue; // the clock ticked on a quiet site
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
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

/// Whether an event is the site saying it changed — not a file merely
/// being read.
fn relevant(event: &Event) -> bool {
    use notify::EventKind::*;
    matches!(event.kind, Modify(_) | Create(_) | Remove(_))
}
