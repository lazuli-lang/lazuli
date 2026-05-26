//! T5 — Watch mode for `lazuli test`.
//!
//! Per the proposal §T5 watch matrix:
//!
//! | Layer | Trigger | Debounce |
//! |---|---|---|
//! | spec    | `*.lzi`, `*.lzx`                                       | 500ms |
//! | view    | same as spec                                           | 500ms |
//! | handler | `*.go` under `<app_dir>/features/*/handlers/`          | 400ms |
//! | ts      | `*.ts`, `*.tsx` under `[testing.ts].discovery_root`    | 300ms |
//! | e2e     | `*.spec.ts` under `e2e/`                               | 600ms |
//!
//! Implementation strategy: per-layer recursive watcher, each owning
//! its own debounce window. When a layer's debounce elapses with
//! pending changes, the watch loop dispatches the runner for that
//! layer only. Other layers continue watching in parallel.
//!
//! The watch surface is deliberately minimal — the orchestrator (see
//! `cmd_test::run_watch`) drives the actual layer execution; this
//! module provides the building blocks (path classification, debounce
//! policy, channel-to-layer routing).

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use notify::{Event, EventKind, RecursiveMode, Watcher};

use crate::cmd_test_types::Layer;

/// Per-layer debounce window. Different layers need different windows
/// because their runners have wildly different cycle times.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_cli::cmd_test_watch::debounce_for;
/// use lazuli_cli::cmd_test_types::Layer;
/// use std::time::Duration;
///
/// assert_eq!(debounce_for(Layer::Handler), Duration::from_millis(400));
/// ```
pub fn debounce_for(layer: Layer) -> Duration {
    match layer {
        Layer::Spec | Layer::View => Duration::from_millis(500),
        Layer::Handler => Duration::from_millis(400),
        Layer::Ts => Duration::from_millis(300),
        Layer::E2e => Duration::from_millis(600),
    }
}

/// Classify a filesystem path into the layer(s) it affects.
///
/// Returns an empty vec for paths that no layer cares about (this is
/// the common case for editor swap files, IDE caches, `target/`, etc.).
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_cli::cmd_test_watch::layers_for_path;
/// use lazuli_cli::cmd_test_types::Layer;
///
/// let layers = layers_for_path(Path::new("/p/app.lzi"), Path::new("/p"));
/// assert!(layers.contains(&Layer::Spec));
/// ```
pub fn layers_for_path(path: &Path, project_root: &Path) -> Vec<Layer> {
    if is_ignored(path) {
        return Vec::new();
    }
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let mut layers = Vec::new();
    match ext {
        "lzi" => {
            layers.push(Layer::Spec);
            layers.push(Layer::View); // Wave 4 views are authored under .lzi too
        }
        "lzx" => {
            layers.push(Layer::View);
            layers.push(Layer::Spec);
        }
        "go" => {
            // Only Go files under `<root>/.../features/*/handlers/` are
            // handler tests; outside that, the change still affects
            // handler because Go test compilation is per-package.
            if path_under_features(path, project_root) {
                layers.push(Layer::Handler);
            }
        }
        "ts" | "tsx" => {
            if name.ends_with(".spec.ts") || path_under_e2e(path, project_root) {
                layers.push(Layer::E2e);
            } else {
                layers.push(Layer::Ts);
            }
        }
        _ => {}
    }
    layers
}

fn path_under_features(path: &Path, _project_root: &Path) -> bool {
    let s = path.to_string_lossy();
    s.contains("/features/") || s.contains("\\features\\")
}

fn path_under_e2e(path: &Path, _project_root: &Path) -> bool {
    let s = path.to_string_lossy();
    s.contains("/e2e/") || s.contains("\\e2e\\")
}

fn is_ignored(path: &Path) -> bool {
    path.components().any(|comp| {
        let name = match comp {
            std::path::Component::Normal(n) => n.to_str(),
            _ => return false,
        };
        matches!(
            name,
            Some(".git" | "target" | "node_modules" | ".claude" | "dist" | "coverage")
        )
    })
}

/// Watch driver. Spawns a `notify` watcher on `project_root` and
/// dispatches debounced layer-tagged events via `tx`. Returns when
/// the channel is closed (so the orchestrator can stop the loop).
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_cli::cmd_test_watch::{spawn_watcher, watch_channel};
///
/// let (tx, _rx) = watch_channel();
/// // let _watcher = spawn_watcher(Path::new("."), tx)?;
/// ```
pub fn spawn_watcher(
    project_root: &Path,
    tx: Sender<WatchEvent>,
) -> Result<notify::RecommendedWatcher> {
    let root = project_root.to_path_buf();
    let event_tx = tx.clone();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
        let Ok(event) = res else { return };
        if matches!(event.kind, EventKind::Access(_)) {
            return;
        }
        for path in event.paths {
            for layer in layers_for_path(&path, &root) {
                let _ = event_tx.send(WatchEvent {
                    layer,
                    path: path.clone(),
                });
            }
        }
    })
    .context("creating watch::recommended_watcher")?;
    watcher
        .watch(project_root, RecursiveMode::Recursive)
        .with_context(|| format!("watching {}", project_root.display()))?;
    Ok(watcher)
}

/// One filesystem event tagged with the layer it affects.
///
/// Produced by `spawn_watcher` and consumed by `DebounceBuffer::push`.
/// `path` is the absolute path of the touched file; `layer` is the
/// single layer classification it maps to. Files affecting multiple
/// layers produce one `WatchEvent` per layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchEvent {
    /// Which layer this change should re-run.
    pub layer: Layer,
    /// Absolute path of the changed file.
    pub path: PathBuf,
}

/// One pending-change set per layer; flushes a set when its debounce
/// elapses with no new events.
#[derive(Debug, Default)]
pub struct DebounceBuffer {
    buckets: Vec<DebounceBucket>,
}

#[derive(Debug)]
struct DebounceBucket {
    layer: Layer,
    paths: Vec<PathBuf>,
    last_seen: Instant,
}

impl DebounceBuffer {
    /// Construct an empty buffer. Equivalent to `Default::default()`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Stage one `WatchEvent` in its layer's bucket, resetting the
    /// debounce timer for that bucket. New layers append a fresh
    /// bucket.
    pub fn push(&mut self, event: WatchEvent) {
        let now = Instant::now();
        if let Some(b) = self.buckets.iter_mut().find(|b| b.layer == event.layer) {
            b.paths.push(event.path);
            b.last_seen = now;
        } else {
            self.buckets.push(DebounceBucket {
                layer: event.layer,
                paths: vec![event.path],
                last_seen: now,
            });
        }
    }

    /// Drains layers whose debounce window has elapsed. Returns one
    /// `(layer, paths)` per layer.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use lazuli_cli::cmd_test_watch::DebounceBuffer;
    ///
    /// let mut buf = DebounceBuffer::new();
    /// assert!(buf.drain_ready().is_empty());
    /// ```
    pub fn drain_ready(&mut self) -> Vec<(Layer, Vec<PathBuf>)> {
        let now = Instant::now();
        let mut ready = Vec::new();
        self.buckets.retain(|b| {
            if now.duration_since(b.last_seen) >= debounce_for(b.layer) {
                ready.push((b.layer, b.paths.clone()));
                false
            } else {
                true
            }
        });
        ready
    }

    /// Returns the smallest remaining debounce window — useful for
    /// `recv_timeout` so we wake up exactly when something is ready.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use lazuli_cli::cmd_test_watch::DebounceBuffer;
    ///
    /// let buf = DebounceBuffer::new();
    /// assert!(buf.next_tick().is_none());
    /// ```
    pub fn next_tick(&self) -> Option<Duration> {
        let now = Instant::now();
        self.buckets
            .iter()
            .map(|b| {
                let elapsed = now.duration_since(b.last_seen);
                let window = debounce_for(b.layer);
                window.saturating_sub(elapsed)
            })
            .min()
    }

    /// True when no layer has any pending changes staged.
    pub fn is_empty(&self) -> bool {
        self.buckets.is_empty()
    }
}

/// The watch loop calls back into this trait once per layer flush.
/// Tests pass a no-op impl; the orchestrator passes a closure that
/// invokes the real runner.
pub trait WatchDispatcher {
    fn dispatch(&mut self, layer: Layer, paths: &[PathBuf]) -> Result<()>;
}

impl<F: FnMut(Layer, &[PathBuf]) -> Result<()>> WatchDispatcher for F {
    fn dispatch(&mut self, layer: Layer, paths: &[PathBuf]) -> Result<()> {
        self(layer, paths)
    }
}

/// Run the watch loop synchronously. Blocks until `rx` returns
/// disconnect (Ctrl-C, or the watcher dropped).
///
/// ## Examples
///
/// ```ignore
/// use lazuli_cli::cmd_test_watch::{run_watch_loop, watch_channel};
/// use lazuli_cli::cmd_test_types::Layer;
/// use std::path::PathBuf;
///
/// let (_tx, rx) = watch_channel();
/// // Dispatcher closure satisfies the WatchDispatcher trait.
/// // run_watch_loop(rx, |_layer: Layer, _paths: &[PathBuf]| Ok(()))?;
/// ```
pub fn run_watch_loop<D: WatchDispatcher>(rx: Receiver<WatchEvent>, mut dispatch: D) -> Result<()> {
    let mut buf = DebounceBuffer::new();
    loop {
        // Wait for either the next event or the next debounce flush
        // window, whichever comes first.
        let timeout = buf
            .next_tick()
            .unwrap_or_else(|| Duration::from_millis(1_000));
        match rx.recv_timeout(timeout) {
            Ok(event) => {
                buf.push(event);
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return Ok(()),
        }
        for (layer, paths) in buf.drain_ready() {
            dispatch.dispatch(layer, &paths)?;
        }
    }
}

/// Convenience constructor for the runtime channel; isolated so tests
/// can build their own.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_cli::cmd_test_watch::watch_channel;
///
/// let (_tx, _rx) = watch_channel();
/// ```
pub fn watch_channel() -> (Sender<WatchEvent>, Receiver<WatchEvent>) {
    channel()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::TryRecvError;

    #[test]
    fn debounce_for_each_layer_distinct() {
        // Spec/view share one window; the rest are unique.
        assert_eq!(debounce_for(Layer::Spec), Duration::from_millis(500));
        assert_eq!(debounce_for(Layer::View), Duration::from_millis(500));
        assert_eq!(debounce_for(Layer::Handler), Duration::from_millis(400));
        assert_eq!(debounce_for(Layer::Ts), Duration::from_millis(300));
        assert_eq!(debounce_for(Layer::E2e), Duration::from_millis(600));
    }

    #[test]
    fn classify_lzi_paths_as_spec_view() {
        let layers = layers_for_path(Path::new("/proj/app.lzi"), Path::new("/proj"));
        assert!(layers.contains(&Layer::Spec));
        assert!(layers.contains(&Layer::View));
    }

    #[test]
    fn classify_handler_go() {
        let layers = layers_for_path(
            Path::new("/proj/app/features/post/handlers/x.go"),
            Path::new("/proj"),
        );
        assert_eq!(layers, vec![Layer::Handler]);
    }

    #[test]
    fn classify_non_feature_go_ignored() {
        let layers = layers_for_path(Path::new("/proj/main.go"), Path::new("/proj"));
        assert!(layers.is_empty());
    }

    #[test]
    fn classify_e2e_spec_ts() {
        let layers = layers_for_path(
            Path::new("/proj/e2e/post/publish.spec.ts"),
            Path::new("/proj"),
        );
        assert_eq!(layers, vec![Layer::E2e]);
    }

    #[test]
    fn classify_component_ts_under_src_is_ts_layer() {
        let layers = layers_for_path(
            Path::new("/proj/app/clients/web/src/post.test.ts"),
            Path::new("/proj"),
        );
        assert_eq!(layers, vec![Layer::Ts]);
    }

    #[test]
    fn ignore_node_modules() {
        let layers = layers_for_path(
            Path::new("/proj/node_modules/foo/index.ts"),
            Path::new("/proj"),
        );
        assert!(layers.is_empty());
    }

    #[test]
    fn debounce_buffer_groups_paths_per_layer() {
        let mut buf = DebounceBuffer::new();
        buf.push(WatchEvent {
            layer: Layer::Handler,
            path: PathBuf::from("a.go"),
        });
        buf.push(WatchEvent {
            layer: Layer::Handler,
            path: PathBuf::from("b.go"),
        });
        buf.push(WatchEvent {
            layer: Layer::Ts,
            path: PathBuf::from("x.tsx"),
        });
        // Before debounce window: nothing ready.
        assert!(buf.drain_ready().is_empty());
        // Sleep past the longest window in the buffer (handler=400ms).
        std::thread::sleep(Duration::from_millis(700));
        let ready = buf.drain_ready();
        // After sleeping, all layers should be ready.
        assert_eq!(ready.len(), 2);
        let handler = ready.iter().find(|(l, _)| *l == Layer::Handler).unwrap();
        assert_eq!(handler.1.len(), 2);
    }

    #[test]
    fn dispatcher_closure_compiles() {
        // Smoke: verify the trait blanket impl works with closures.
        let mut called = false;
        let mut dispatch = |layer: Layer, _paths: &[PathBuf]| {
            called = true;
            assert_eq!(layer, Layer::Spec);
            Ok::<_, anyhow::Error>(())
        };
        dispatch.dispatch(Layer::Spec, &[]).unwrap();
        assert!(called);
    }

    #[test]
    fn channel_smoke() {
        let (tx, rx) = watch_channel();
        tx.send(WatchEvent {
            layer: Layer::Spec,
            path: PathBuf::from("app.lzi"),
        })
        .unwrap();
        drop(tx);
        let ev = rx.recv().unwrap();
        assert_eq!(ev.layer, Layer::Spec);
        // Channel closed after drop.
        assert!(matches!(rx.try_recv(), Err(TryRecvError::Disconnected)));
    }
}
