use std::path::{Component, Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{RecvTimeoutError, channel};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow};
use notify::{Event, EventKind, RecursiveMode, Watcher};

/// Typed options for [`run_dev`]. Each field maps 1:1 to a flag on
/// the `lazuli dev` clap arm.
pub struct DevOptions {
    /// Directory the watcher monitors (project root by default).
    pub source_root: PathBuf,
    /// Where regen writes its output (`dist/go` by default).
    pub out: PathBuf,
    /// When `true`, the loop regenerates but never starts the Go
    /// server child.
    pub no_run: bool,
    /// Debounce window between filesystem events and a regen cycle.
    pub debounce: Duration,
}

impl Default for DevOptions {
    fn default() -> Self {
        Self {
            source_root: PathBuf::from("."),
            out: PathBuf::from("dist/go"),
            no_run: false,
            debounce: Duration::from_millis(300),
        }
    }
}

/// Drive the `lazuli dev` loop — watch `source_root`, regenerate on
/// change (with a [`DevOptions::debounce`] window), and (unless
/// `no_run`) spawn the Go server child. Blocks until Ctrl-C.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_cli::dev::{run_dev, DevOptions};
/// // run_dev(DevOptions::default())?;
/// ```
pub fn run_dev(opts: DevOptions) -> Result<()> {
    validate_source(&opts.source_root)?;

    let watch_path = watch_path(&opts.source_root)?;
    let out_dir = effective_out_dir(&opts);
    eprintln!("lazuli dev: watching {}", watch_path.display());

    regen(&opts)?;
    eprintln!("regen ok");

    let mut child = spawn_go(&opts)?;
    let (tx, rx) = channel();
    let mut watcher = notify::recommended_watcher(move |event| {
        let _ = tx.send(event);
    })
    .context("creating file watcher")?;
    watcher
        .watch(&watch_path, RecursiveMode::Recursive)
        .with_context(|| format!("watching {}", watch_path.display()))?;

    loop {
        let mut changed_paths = match rx.recv() {
            Ok(event) => interesting_paths(event, &out_dir),
            Err(_) => break,
        };

        loop {
            match rx.recv_timeout(opts.debounce) {
                Ok(event) => changed_paths.extend(interesting_paths(event, &out_dir)),
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => return Ok(()),
            }
        }

        changed_paths.sort();
        changed_paths.dedup();
        let Some(first_changed) = changed_paths.first() else {
            continue;
        };

        eprintln!("change detected: {}", first_changed.display());
        stop_child(&mut child);

        eprintln!("regen...");
        let started = Instant::now();
        match regen(&opts) {
            Ok(()) => {
                eprintln!("regen ok ({}ms)", started.elapsed().as_millis());
                if !opts.no_run {
                    eprintln!("restarting server...");
                }
                child = spawn_go(&opts)?;
            }
            Err(err) => {
                eprintln!("regen failed: {err:#}");
                child = None;
            }
        }
    }

    stop_child(&mut child);
    Ok(())
}

pub(crate) fn regen(opts: &DevOptions) -> Result<()> {
    validate_source(&opts.source_root)?;
    let out_dir = effective_out_dir(opts);
    // `dev` regenerates on every save — `--allow-drops` is opt-in on
    // the explicit `lazuli generate go` invocation, so the watch loop
    // never escalates to destructive ALTERs implicitly.
    crate::generate_go(
        &opts.source_root,
        Some(&out_dir),
        None,
        None,
        false,
        false,
        false,
    )
}

fn spawn_go(opts: &DevOptions) -> Result<Option<Child>> {
    if opts.no_run {
        return Ok(None);
    }

    let out_dir = effective_out_dir(opts);
    let child = Command::new("go")
        .arg("run")
        .arg("./...")
        .current_dir(&out_dir)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .with_context(|| format!("spawning `go run ./...` in {}", out_dir.display()))?;

    Ok(Some(child))
}

fn stop_child(child: &mut Option<Child>) {
    let Some(mut running) = child.take() else {
        return;
    };

    match running.try_wait() {
        Ok(Some(_)) => return,
        Ok(None) => {}
        Err(err) => {
            eprintln!("checking server status failed: {err}");
        }
    }

    eprintln!("shutting down server...");
    if let Err(err) = running.kill() {
        eprintln!("stopping server failed: {err}");
    }
    if let Err(err) = running.wait() {
        eprintln!("waiting for server exit failed: {err}");
    }
}

fn interesting_paths(event: notify::Result<Event>, out_dir: &Path) -> Vec<PathBuf> {
    let Ok(event) = event else {
        return Vec::new();
    };

    if matches!(event.kind, EventKind::Access(_)) {
        return Vec::new();
    }

    event
        .paths
        .into_iter()
        .filter(|path| is_lazuli_source(path))
        .filter(|path| !is_ignored_path(path, out_dir))
        .collect()
}

fn is_lazuli_source(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|ext| ext.to_str()),
        Some("lzi" | "lzx")
    )
}

fn is_ignored_path(path: &Path, out_dir: &Path) -> bool {
    if absolute_path(path).starts_with(absolute_path(out_dir)) {
        return true;
    }

    path.components().any(|component| {
        let Component::Normal(name) = component else {
            return false;
        };
        matches!(
            name.to_str(),
            Some(".git" | "target" | "node_modules" | ".claude")
        ) || name.to_str().is_some_and(|name| name.starts_with('_'))
    })
}

fn absolute_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    }
}

fn watch_path(source_root: &Path) -> Result<PathBuf> {
    if source_root.is_dir() {
        return Ok(source_root.to_path_buf());
    }

    source_root
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| anyhow!("{} has no parent directory to watch", source_root.display()))
}

fn effective_out_dir(opts: &DevOptions) -> PathBuf {
    if opts.out.is_absolute() {
        return opts.out.clone();
    }

    project_root(&opts.source_root).join(&opts.out)
}

fn project_root(source_root: &Path) -> PathBuf {
    if source_root.is_file() {
        source_root
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."))
    } else {
        source_root.to_path_buf()
    }
}

fn validate_source(source_root: &Path) -> Result<()> {
    if source_root.exists() {
        Ok(())
    } else {
        Err(anyhow!(
            "source path does not exist: {}",
            source_root.display()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_options_construct() {
        let opts = DevOptions::default();
        assert_eq!(opts.source_root, PathBuf::from("."));
        assert_eq!(opts.out, PathBuf::from("dist/go"));
        assert!(!opts.no_run);
        assert_eq!(opts.debounce, Duration::from_millis(300));
    }

    #[test]
    fn regen_errors_when_source_path_is_missing() {
        let opts = DevOptions {
            source_root: PathBuf::from("__missing_lazuli_dev_source__"),
            ..DevOptions::default()
        };

        assert!(regen(&opts).is_err());
    }
}
