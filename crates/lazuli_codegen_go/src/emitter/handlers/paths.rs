//! Path + identifier helpers for handler stub emission.
//!
//! Centralises every string-level shaping that lands in either the
//! filesystem path (`app/features/<f>/handlers/<name>.go`), the Go
//! package name (`<feature>handlers`), or the exported Go function
//! identifier. Keeps the entry + emit modules free of low-level
//! sanitisation so the boundary stays "this is where IR names become
//! disk-safe strings."
//!
//! `path_exists` lives here too because the legacy-location lookup is
//! the inverse of `handler_path` — both encode the same canonical
//! layout described at the top of the module.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const DIST_GO_PREFIX: &str = "dist/go";

/// Canonical handler stub location:
/// `app/features/<feature>/handlers/<name>.go`.
///
/// Lives in the portable kernel (Tier 1 per `docs/project-structure.md`)
/// inside a dedicated `handlers/` sub-folder so the feature directory
/// itself stays focused on the DSL surface (`<f>.lzi`, `<f>.lzx`,
/// `<f>.<target>.lzx`, `templates/`, `queries/`, `handlers/`).
///
/// Returned paths are project-root relative; the orchestrator detects
/// the `app/features/` prefix and writes to project root instead of
/// the codegen `out_dir`.
///
/// Sub-folder package name is `<feature>handlers` (snake_case
/// concatenated). The import cycle that motivated the earlier flat
/// layout is now broken by the runtime registry — gen never imports
/// user handler packages directly; user handlers register themselves
/// at init via `lazuli.RegisterFn(...)` and the generated `Effect:
/// lazuli.ReturnsFromRegistry[I, O]("<feature>.<name>")` resolves
/// them at dispatch time.
///
/// Idempotency: once a file exists at this path, regeneration skips it
/// (the user has either authored or edited the stub). See `path_exists`
/// for the lookup that drives the skip decision.
pub(crate) const APP_FEATURES_PREFIX: &str = "app/features";

pub(super) fn handler_path(feature: &str, name: &str) -> String {
    format!(
        "{APP_FEATURES_PREFIX}/{feature}/handlers/{}.go",
        path_name_for(name)
    )
}

/// Go package name for the handler sub-folder. Matches the directory
/// name (`<feature>handlers`) so Go's package-name=dir-name convention
/// holds; user code references its own siblings without a qualifier
/// and references generated types via the `<feature>gen` alias.
pub(super) fn handlers_package_name(feature: &str) -> String {
    format!("{feature}handlers")
}

pub(super) fn path_exists(existing_files: &BTreeSet<PathBuf>, relative_path: &str) -> bool {
    if existing_files.is_empty() {
        return false;
    }

    let rel = normalize_path_string(relative_path);
    let rel_suffix = format!("/{rel}");

    // Legacy stub locations — pre-pivot scaffolds may have handlers at:
    //  - `dist/go/<f>/<name>.go` (the first failed pivot)
    //  - `app/features/<f>/<name>.go` (the second flat pivot)
    // Translate the canonical `app/features/<f>/handlers/<name>.go`
    // path back to those two shapes so the regen doesn't double-stub
    // handlers that already exist at the older locations.
    let legacy_alternatives: Vec<String> =
        if let Some(tail) = rel.strip_prefix(&format!("{APP_FEATURES_PREFIX}/")) {
            // `tail` is `<feature>/handlers/<name>.go`. Strip the
            // `handlers/` segment to derive `<feature>/<name>.go` —
            // matches the flat-pivot layout.
            let mut alts = Vec::with_capacity(2);
            if let Some((feature, after)) = tail.split_once('/')
                && let Some(name) = after.strip_prefix("handlers/")
            {
                let flat_app = format!("{APP_FEATURES_PREFIX}/{feature}/{name}");
                let flat_dist = format!("{DIST_GO_PREFIX}/{feature}/{name}");
                alts.push(flat_app);
                alts.push(flat_dist);
            }
            alts
        } else {
            Vec::new()
        };

    existing_files.iter().any(|path| {
        let existing = normalize_path(path);
        if existing == rel || existing.ends_with(&rel_suffix) {
            return true;
        }
        for legacy in &legacy_alternatives {
            let suffix = format!("/{legacy}");
            if existing == *legacy || existing.ends_with(&suffix) {
                return true;
            }
        }
        false
    })
}

fn normalize_path(path: &Path) -> String {
    normalize_path_string(&path.to_string_lossy())
}

fn normalize_path_string(path: &str) -> String {
    path.replace('\\', "/").trim_start_matches("./").to_owned()
}

pub(super) fn path_name_for(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    out.trim_matches('_').to_owned()
}

pub(super) fn exported_func_name(name: &str) -> String {
    let pascal = pascal_case(name);
    if pascal.is_empty() {
        return "Handler".to_owned();
    }
    if pascal
        .chars()
        .next()
        .map(|ch| ch.is_ascii_digit())
        .unwrap_or(false)
    {
        return format!("Handler{pascal}");
    }
    pascal
}

pub(super) fn pascal_case(s: &str) -> String {
    super::super::casing::pascal_case(s)
}

pub(super) fn escape_string(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(ch),
        }
    }
    out
}

pub(super) fn escape_comment(raw: &str) -> String {
    raw.replace(['\n', '\r'], " ")
}
