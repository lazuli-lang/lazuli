//! `lazuli doctor --watch` — re-runs the pipeline on every `.lzi` / `.lzx`
//! change and streams findings as JSON / NDJSON. Wave 2.4 of the TDD/BDD-
//! first proposal.
//!
//! Scope (PARTIAL by design): the watch loop reuses the same `notify`
//! backend the rest of the CLI relies on (`crate::dev`). LSP-watch
//! unification is deferred to a follow-up cell.

use std::path::Path;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use lazuli_lsp::SecurityProfile;
use notify::{Event, RecursiveMode, Watcher};

use crate::doctor;
use crate::doctor_report::{DoctorFormat, FailOnSpec};

const DEBOUNCE_MS: u64 = 200;

pub fn doctor_watch_command(
    input: &Path,
    security_profile: SecurityProfile,
    allow_version_mismatch: bool,
    format: DoctorFormat,
    fail_on: Vec<FailOnSpec>,
) -> Result<()> {
    // Run once at startup so the agent sees the current state immediately.
    run_once(
        input,
        security_profile,
        allow_version_mismatch,
        format,
        &fail_on,
    );

    let (tx, rx) = mpsc::channel::<Result<Event, notify::Error>>();
    let mut watcher = notify::recommended_watcher(move |res| {
        let _ = tx.send(res);
    })
    .context("failed to create file watcher")?;
    let watch_root = if input.is_dir() {
        input.to_path_buf()
    } else {
        input
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| std::path::PathBuf::from("."))
    };
    watcher
        .watch(&watch_root, RecursiveMode::Recursive)
        .with_context(|| format!("failed to watch {}", watch_root.display()))?;

    let mut last_run = Instant::now();
    loop {
        match rx.recv_timeout(Duration::from_millis(500)) {
            Ok(Ok(event)) => {
                if !event_touches_lazuli_source(&event) {
                    continue;
                }
                // Debounce: editor saves often emit a burst of events.
                if last_run.elapsed() < Duration::from_millis(DEBOUNCE_MS) {
                    continue;
                }
                last_run = Instant::now();
                run_once(
                    input,
                    security_profile,
                    allow_version_mismatch,
                    format,
                    &fail_on,
                );
            }
            Ok(Err(err)) => {
                eprintln!("watch error: {err}");
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                // Periodic wake-up; lets us notice external termination.
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                break;
            }
        }
    }
    Ok(())
}

fn event_touches_lazuli_source(event: &Event) -> bool {
    event.paths.iter().any(|p| {
        let s = p.to_string_lossy();
        s.ends_with(".lzi") || s.ends_with(".lzx") || s.ends_with("Lazurite.toml")
    })
}

fn run_once(
    input: &Path,
    security_profile: SecurityProfile,
    allow_version_mismatch: bool,
    format: DoctorFormat,
    fail_on: &[FailOnSpec],
) {
    // Watch never bubbles up an Err — the caller expects a long-running
    // loop. Errors are printed to stderr so the agent sees them.
    let opts = doctor::DoctorRuntimeOptions {
        format: Some(
            match format {
                DoctorFormat::Auto => "auto",
                DoctorFormat::Text => "text",
                DoctorFormat::Json => "json",
                DoctorFormat::Ndjson => "ndjson",
            }
            .to_string(),
        ),
        coverage: false,
        fail_on: fail_on
            .iter()
            .map(|spec| match spec {
                FailOnSpec::Severity(s) => s.as_str().to_string(),
                FailOnSpec::Category(c) => format!("category:{}", c.as_str()),
                FailOnSpec::Rule(r) => format!("rule:{r}"),
            })
            .collect(),
        // Watch mode never self-audits; --watch + --self combo would
        // re-walk crates/ on every keystroke, pointless overhead.
        self_audit: false,
    };
    if let Err(err) = doctor::doctor_command_with_options(
        input,
        security_profile,
        false,
        allow_version_mismatch,
        opts,
    ) {
        eprintln!("doctor run failed: {err}");
    }
}
