//! Doctor rule `VOCAB-CLIENT-SRC-001`.
//!
//! Closed-catalog enforcement for the Lazurite client `src/` layout per
//! `docs/decisions/client_src_canonical_architecture_2026-05-17.md` §3.
//!
//! The rule walks two possible client-tree shapes (per
//! `[[lazurite_monorepo_shape_2026-05-17]]` and
//! `[[frontend_layout_default_and_slicing_2026-05-16]]`):
//!
//! - **Singular** topology: `app/web/` — one web target, the 6-folder
//!   closed catalog applies at this level.
//! - **Plural** topology: `app/clients/<name>/src/` — every direct
//!   child of `app/clients/` is a client root; the 6-folder closed
//!   catalog applies under each `<name>/src/`.
//!
//! Two closed catalogs are enforced:
//!
//! 1. Top-level of the client tree (the wrapper itself): must be one of
//!    `{shell, routes, ui, theme, state, assets, cells}` (per the
//!    §3.1 amendment 2026-05-17 — was 6 folders; `cells/` added as the
//!    7th to resolve tsc module-resolution for handcrafted feature
//!    cells).
//! 2. Children of the `ui/` subfolder (when present): must be one of
//!    `{forms, feedback, navigation, display, overlays, layout}`.
//! 3. Children of the `cells/` subfolder (when present): each subdir
//!    MUST mirror an existing `app/features/<feature>/` directory.
//!    Orphan cells (no matching feature) fire a diagnostic with the
//!    cell sub-dir's path. NOTE: orphan-cell enforcement is a future
//!    enhancement; v0 only enforces the top-level + `ui/` closed
//!    catalogs.
//!
//! Any other directory at those two levels fires a finding. Files
//! (rather than directories) at those levels are NOT flagged by this
//! rule — the file layout discipline lives in sibling rules like
//! `feature-orphan-component`.
//!
//! ## Skip discipline
//!
//! Non-Lazuli sub-apps under `app/clients/external/<name>/` (per
//! `[[lazurite_monorepo_shape_2026-05-17]]` §2.2, revised 2026-05-18)
//! are invisible: when listing `app/clients/<name>/`, the walker
//! explicitly skips the `external` entry. The pre-revision top-level
//! `external/` location is also invisible — the walker never enters
//! it because it only descends into `app/web/` and
//! `app/clients/<name>/src/`.
//!
//! Inside walked trees, the rule does not descend further than the
//! checked levels — it inspects the immediate children of the client
//! root and the immediate children of `ui/` only. Build outputs like
//! `dist/` and `node_modules/` therefore can't appear in either
//! closed-catalog level since they would themselves be the
//! anti-pattern. To keep error noise low, however, the well-known
//! tooling dirs `dist`, `node_modules`, and `.lazuli` are explicitly
//! skipped at the top level (treated as out-of-scope rather than
//! diagnosed — the user typically can't avoid them and they aren't
//! source code).

use std::fs;
use std::path::{Path, PathBuf};

/// One `VOCAB-CLIENT-SRC-001` finding — a single anti-pattern
/// directory at one of the two enforced levels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Path of the offending directory (relative to the project root or
    /// absolute, as walked). Callers normalize via `doctor_rule_path`.
    pub path: PathBuf,
    /// Pre-rendered diagnostic message.
    pub message: String,
}

impl Finding {
    /// Stable doctor rule code surfaced to the user.
    pub const CODE: &'static str = "VOCAB-CLIENT-SRC-001";
}

/// Top-level closed catalog for the client tree (singular `app/web/`
/// or each plural `app/clients/<name>/src/`).
pub(super) const TOP_LEVEL_ALLOWED: &[&str] =
    &["shell", "routes", "ui", "theme", "state", "assets", "cells"];

/// Closed sub-catalog for `ui/` children inside the client tree.
pub(super) const UI_CHILDREN_ALLOWED: &[&str] = &[
    "forms",
    "feedback",
    "navigation",
    "display",
    "overlays",
    "layout",
];

/// Well-known tooling directories that are skipped at the top-level
/// without firing a diagnostic. They are not source-controlled vocab
/// choices the user makes, so flagging them is noise.
///
/// `__smoke__/` is a framework-emitted convention (spec 0027): `lazuli
/// new --frontends web` writes the render + generated-SDK compile smoke
/// tests there. It's a Lazuli-owned test dir, not a user catalog choice,
/// so it's out-of-scope for closed-catalog enforcement (same treatment
/// as `dist`/`node_modules`).
const TOP_LEVEL_SKIP: &[&str] =
    &["dist", "node_modules", ".lazuli", ".git", ".cache", "__smoke__"];

/// Walk the project's client trees and return one finding per
/// closed-catalog violation. See module docs for scope.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_cli::doctor::folder::vocab_client_src_001::check;
///
/// // let findings = check(Path::new("."));
/// ```
pub fn check(root: &Path) -> Vec<Finding> {
    let mut findings = Vec::new();

    // Singular: app/web/
    let app_web = root.join("app").join("web");
    if app_web.is_dir() {
        check_client_tree(&app_web, &mut findings);
    }

    // Plural: app/clients/<name>/src/ — visit every direct child of
    // app/clients/ that has a src/ subdir. Children without a src/
    // (e.g., placeholder dirs) are out of scope; the
    // monorepo-exception convention puts non-Lazuli apps in
    // external/<name>/ instead.
    let app_clients = root.join("app").join("clients");
    if let Ok(entries) = fs::read_dir(&app_clients) {
        let mut client_dirs: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
            .collect();
        client_dirs.sort();

        for client_dir in client_dirs {
            // Skip the polyglot escape-hatch subtree per
            // `[[lazurite_monorepo_shape_2026-05-17]]` §2.2 (revised
            // 2026-05-18) — `app/clients/external/<name>/` houses
            // non-Lazuli frontends and is exempt from canon walks.
            if client_dir.file_name().and_then(|n| n.to_str()) == Some("external") {
                continue;
            }
            let client_src = client_dir.join("src");
            if client_src.is_dir() {
                check_client_tree(&client_src, &mut findings);
            }
        }
    }

    findings
}

fn check_client_tree(client_root: &Path, findings: &mut Vec<Finding>) {
    let Ok(entries) = fs::read_dir(client_root) else {
        return;
    };

    let mut children: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    children.sort();

    for child in children {
        let name = match child.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_owned(),
            None => continue,
        };

        if TOP_LEVEL_SKIP.contains(&name.as_str()) {
            continue;
        }

        if !TOP_LEVEL_ALLOWED.contains(&name.as_str()) {
            findings.push(Finding {
                message: top_level_message(&name, &child),
                path: child.clone(),
            });
            continue;
        }

        if name == "ui" {
            check_ui_children(&child, findings);
        }
    }
}

fn check_ui_children(ui_root: &Path, findings: &mut Vec<Finding>) {
    let Ok(entries) = fs::read_dir(ui_root) else {
        return;
    };

    let mut children: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    children.sort();

    for child in children {
        let name = match child.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_owned(),
            None => continue,
        };

        if !UI_CHILDREN_ALLOWED.contains(&name.as_str()) {
            findings.push(Finding {
                message: ui_child_message(&name, &child),
                path: child.clone(),
            });
        }
    }
}

fn top_level_message(name: &str, path: &Path) -> String {
    let hint = match name {
        "shared" => Some(
            "anti-pattern catch-all; route into one of \
             `{shell, routes, ui, theme, state, assets}` or \
             `app/features/<f>/cells/`",
        ),
        "presentation" => Some(
            "DDD/Clean-Architecture layer leaked; reject per \
             `[[client_src_canonical_architecture_2026-05-17]]` §2 — \
             route children into `ui/`, `shell/`, or `theme/`",
        ),
        "application" => Some(
            "DDD/Clean-Architecture layer leaked; reject per \
             `[[client_src_canonical_architecture_2026-05-17]]` §2 — \
             cross-feature state moves to `state/`",
        ),
        "features" => Some(
            "a feature has one home — `app/features/<f>/`; cells go to \
             `app/features/<f>/cells/`",
        ),
        "lib" | "utils" | "helpers" | "services" | "components" | "types" | "api" | "common"
        | "misc" => Some(
            "catch-all anti-pattern; each file has a real home in \
             `{shell, routes, ui, theme, state, assets}` or \
             `app/features/<f>/cells/`",
        ),
        _ => None,
    };

    let allowed = TOP_LEVEL_ALLOWED.join(", ");
    match hint {
        Some(hint) => format!(
            "`{}` is outside the client `src/` closed catalog `{{{}}}` — {}. \
             See `[[client_src_canonical_architecture_2026-05-17]]` §3.",
            path.display(),
            allowed,
            hint
        ),
        None => format!(
            "`{}` is outside the client `src/` closed catalog `{{{}}}`. \
             See `[[client_src_canonical_architecture_2026-05-17]]` §3 \
             for the destination of `{}/`.",
            path.display(),
            allowed,
            name
        ),
    }
}

fn ui_child_message(name: &str, path: &Path) -> String {
    let hint = match name {
        "actions" => Some(
            "consolidate into `ui/forms/` (buttons are form actions) \
             or `ui/navigation/` (links)",
        ),
        "branding" => Some(
            "consolidate into `ui/display/` (logos as display) or \
             `theme/` (brand tokens)",
        ),
        "cards" => Some("consolidate into `ui/display/` — Card is a display primitive"),
        "service" => Some("feature-specific widget; move to `app/features/<f>/cells/`"),
        "atoms" | "molecules" | "organisms" => Some(
            "Atomic Design rejected per \
             `[[client_src_canonical_architecture_2026-05-17]]` §3.2 — \
             closed-catalog by function (forms/feedback/navigation/display/overlays/layout), \
             not by size",
        ),
        _ => None,
    };

    let allowed = UI_CHILDREN_ALLOWED.join(", ");
    match hint {
        Some(hint) => format!(
            "`{}` is outside the `ui/` closed catalog `{{{}}}` — {}. \
             See `[[client_src_canonical_architecture_2026-05-17]]` §3.2.",
            path.display(),
            allowed,
            hint
        ),
        None => format!(
            "`{}` is outside the `ui/` closed catalog `{{{}}}`. \
             See `[[client_src_canonical_architecture_2026-05-17]]` §3.2 \
             for the destination of `ui/{}/`.",
            path.display(),
            allowed,
            name
        ),
    }
}

#[cfg(test)]
mod test_support;
#[cfg(test)]
mod tests_anti_patterns;
#[cfg(test)]
mod tests_canon;
