//! Doctor rule `feature-orphan-component`.
//!
//! Flags user-authored `.tsx` / `.ts` files in non-canonical paths under a
//! Lazurite product repo. The canonical frontend layout is defined by L0 #1
//! (`docs/proposals/lazurite-frontend-folder-canon.md` §4), the Wave H
//! corrective (`[[client_src_canonical_architecture_2026-05-17]]` §3), and
//! the singular-vs-plural decision
//! (`[[frontend_layout_default_and_slicing_2026-05-16]]`).
//!
//! Canonical locations (post-Wave-H 7+6 closed catalog):
//!
//!   Plural client topology (`app/clients/<name>/src/...`):
//!     - `app/clients/<name>/src/shell/**/*.{ts,tsx}`
//!     - `app/clients/<name>/src/routes/**/*.{ts,tsx}`
//!     - `app/clients/<name>/src/cells/<feature>/**/*.{ts,tsx}`
//!     - `app/clients/<name>/src/ui/{forms,feedback,navigation,display,overlays,layout}/**/*.{ts,tsx}`
//!     - `app/clients/<name>/src/theme/**/*.{ts,tsx}`
//!     - `app/clients/<name>/src/state/**/*.{ts,tsx}`
//!     - `app/clients/<name>/src/assets/**/*.{ts,tsx}`
//!     - `app/clients/<name>/src/{index,main,vite-env}.{ts,tsx}` (wrapper-level files)
//!     - `app/clients/<name>/{main,vite.config,tailwind.config,playwright.config,nativewind-env}.ts(x)` (client config)
//!
//!   Singular client topology (`app/web/...` — same shape, different wrapper):
//!     - `app/web/shell/**/*.{ts,tsx}`
//!     - `app/web/routes/**/*.{ts,tsx}`
//!     - `app/web/cells/<feature>/**/*.{ts,tsx}`
//!     - `app/web/ui/{forms,feedback,navigation,display,overlays,layout}/**/*.{ts,tsx}`
//!     - `app/web/theme/**/*.{ts,tsx}`
//!     - `app/web/state/**/*.{ts,tsx}`
//!     - `app/web/assets/**/*.{ts,tsx}`
//!     - `app/web/{index,main,vite-env}.{ts,tsx}`
//!
//!   Legacy DSL feature shape (still recognised — generators emit here):
//!     - `app/features/<feat>/web/{cells,views/<audience>}/*.{ts,tsx}`
//!     - `app/features/<feat>/mobile/{cells,views/<audience>}/*.{ts,tsx}`
//!     - `features/<feat>/...` (test-fixture form, identical shape)
//!
//!   Legacy frontends wrapper (pre-Wave-H pilots):
//!     - `frontends/<target>/{shell,theme,ui,hooks,lib}/**/*.{ts,tsx}`
//!     - `frontends/<target>/{main,vite.config,tailwind.config}.ts(x)`
//!
//! Anything else inside the Lazuli-owned tree, such as `src/components/`,
//! `app/components/`, or `lib/components/`, is an orphan component.
//!
//! ## Scope discipline (polyglot monorepo)
//!
//! Per `CLAUDE.md` and the polyglot-monorepo canon, a Lazurite repo's root
//! may host non-Lazuli siblings (`apps/<frontend>/`, `packages/<pkg>/`,
//! `brand/`, `scripts/`, `docs/`, etc.) that are owned by the surrounding
//! pnpm/turbo/Astro/Vite workspace and NOT by the Lazuli compiler.
//!
//! This rule therefore only descends into the Lazuli-owned top-level
//! entries — `app/`, `features/` (legacy fixture form), and `frontends/`.
//! Anything else at the root is invisible to this rule. The Lazuli-owned
//! tree's contents are still walked recursively from those entry points,
//! so orphans inside `app/shared/ui/` or `app/components/` still fire.
//!
//! One subtree under `app/` is explicitly skipped: `app/clients/external/`
//! is the polyglot escape-hatch per
//! `[[lazurite_monorepo_shape_2026-05-17]]` §2.2 (revised 2026-05-18) —
//! non-Lazuli frontends (Astro marketing sites, Rails dashboards, etc.)
//! live grouped with Lazuli-native clients but follow their own
//! conventions. Doctor does not descend into `app/clients/external/**`.

use std::fs;
use std::path::{Path, PathBuf};

/// One `feature-orphan-component` finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub path: PathBuf,
    pub message: String,
}

impl Finding {
    /// Stable doctor rule code surfaced to the user.
    pub const CODE: &'static str = "feature-orphan-component";

    /// Render the remediation message naming the orphan path and the
    /// 7+6 closed catalog of canonical destinations.
    pub fn message(path: &Path) -> String {
        format!(
            "`{}` is outside the canonical Lazurite frontend layout. \
             Route into the 7+6 closed catalog per \
             `[[client_src_canonical_architecture_2026-05-17]]` §3: \
             `app/clients/<name>/src/routes/**` (page-level .tsx), \
             `app/clients/<name>/src/cells/<feature>/**` (handcrafted slot \
             widget), `app/clients/<name>/src/ui/{{forms,feedback,navigation,\
             display,overlays,layout}}/**` (shared primitive), or \
             `app/clients/<name>/src/{{shell,theme,state,assets}}/**`. \
             Singular topology uses `app/web/` with the same shape.",
            path.display()
        )
    }
}

/// Walk the Lazuli-owned subtrees of `root`, classify each `.tsx`/`.ts` file
/// as canonical or orphan, and return findings for the orphans.
///
/// Only the Lazuli-owned top-level entries are walked (`app/`, `features/`,
/// `frontends/`). Polyglot-sibling roots (`apps/<frontend>/`,
/// `packages/<pkg>/`, `brand/`, `scripts/`, etc.) are invisible to this
/// rule — they are owned by the surrounding workspace, not by the Lazuli
/// compiler. See module docs for the scope-discipline rationale.
///
/// Inside Lazuli-owned subtrees, skips directories: `node_modules`, `dist`,
/// `.lazuli`, `.git`, `target`, `.next`, `.expo`, `.cache`, `coverage`.
///
/// Also skips test/story files via extension: `*.test.tsx`, `*.spec.tsx`,
/// `*.stories.tsx`.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_cli::doctor::folder::feature_orphan::check;
///
/// // let findings = check(Path::new("."));
/// ```
pub fn check(root: &Path) -> Vec<Finding> {
    let mut findings = Vec::new();
    for entry in LAZULI_OWNED_ROOTS {
        let subtree = root.join(entry);
        if subtree.is_dir() {
            walk(root, &subtree, &mut findings);
        }
    }
    findings
}

/// Top-level entries under the project root that this rule descends into.
///
/// Anything else at the root is a polyglot sibling and out of scope per
/// `docs/scope-discipline.md` + the polyglot-monorepo canon. Adding a new
/// entry here is a language-canon decision, not a per-project config.
const LAZULI_OWNED_ROOTS: &[&str] = &["app", "features", "frontends"];

// --- internal helpers ---

fn walk(root: &Path, dir: &Path, findings: &mut Vec<Finding>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };

    let mut entries: Vec<_> = entries.filter_map(Result::ok).collect();
    entries.sort_by_key(|entry| entry.file_name());

    for entry in entries {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };

        if file_type.is_dir() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if should_skip_dir(&name) {
                continue;
            }
            if is_external_clients_subtree(root, &path) {
                continue;
            }
            walk(root, &path, findings);
            continue;
        }

        if !file_type.is_file() || !is_ts_or_tsx_file(&path) || is_test_or_story_file(&path) {
            continue;
        }

        let rel = path.strip_prefix(root).unwrap_or(path.as_path());
        if !is_canonical_path(rel) {
            findings.push(Finding {
                path: rel.to_path_buf(),
                message: Finding::message(rel),
            });
        }
    }
}

/// Returns true if the relative-to-root path matches one of the canonical
/// locations.
fn is_canonical_path(rel: &Path) -> bool {
    let parts = path_parts(rel);

    match parts.as_slice() {
        // --- Wave H 7+6 canon, plural topology ---
        // app/clients/<name>/src/{shell,routes,theme,state,assets}/<...>
        ["app", "clients", _, "src", folder, rest @ ..]
            if is_client_src_top_level(folder) && !rest.is_empty() =>
        {
            true
        }
        // app/clients/<name>/src/cells/<feature>/<...>
        // (per Wave H amendment: subdirs MUST mirror app/features/<feature>/;
        // the mirror enforcement is sibling rule's job — here we just accept
        // any non-empty cells/<feature>/file shape).
        ["app", "clients", _, "src", "cells", _feature, rest @ ..] if !rest.is_empty() => true,
        // app/clients/<name>/src/ui/<sub-bucket>/<...>
        ["app", "clients", _, "src", "ui", sub, rest @ ..]
            if is_ui_sub_bucket(sub) && !rest.is_empty() =>
        {
            true
        }
        // app/clients/<name>/src/ui/index.{ts,tsx} — barrel file
        // re-exporting the 6 sub-bucket primitives. Common pilot
        // convention; not a sub-bucket so it has its own arm.
        ["app", "clients", _, "src", "ui", file] if is_barrel_file(file) => true,
        // app/clients/<name>/src/<wrapper-file>.{ts,tsx}
        ["app", "clients", _, "src", file] if is_client_src_wrapper_file(file) => true,
        // app/clients/<name>/<client-config>.{ts,tsx} — tooling configs that
        // live at the wrapper level (vite.config.ts, playwright.config.ts,
        // tailwind.config.ts, nativewind-env.d.ts, etc.).
        ["app", "clients", _, file] if is_client_wrapper_config_file(file) => true,

        // --- Wave H 7+6 canon, singular topology ---
        // app/web/{shell,routes,theme,state,assets}/<...>
        ["app", "web", folder, rest @ ..]
            if is_client_src_top_level(folder) && !rest.is_empty() =>
        {
            true
        }
        // app/web/cells/<feature>/<...>
        ["app", "web", "cells", _feature, rest @ ..] if !rest.is_empty() => true,
        // app/web/ui/<sub-bucket>/<...>
        ["app", "web", "ui", sub, rest @ ..] if is_ui_sub_bucket(sub) && !rest.is_empty() => true,
        // app/web/ui/index.{ts,tsx} — barrel file
        ["app", "web", "ui", file] if is_barrel_file(file) => true,
        // app/web/<wrapper-file>.{ts,tsx} (top-level shell entry)
        ["app", "web", file] if is_client_src_wrapper_file(file) => true,

        // --- Legacy DSL feature shape ---
        // app/features/<feat>/web/cells/<file>.tsx
        // app/features/<feat>/mobile/cells/<file>.tsx
        ["app", "features", _, platform, "cells", rest @ ..]
            if is_platform(platform) && !rest.is_empty() =>
        {
            true
        }

        // app/features/<feat>/web/views/<audience>/<file>.tsx
        // app/features/<feat>/mobile/views/<audience>/<file>.tsx
        ["app", "features", _, platform, "views", _, rest @ ..]
            if is_platform(platform) && !rest.is_empty() =>
        {
            true
        }

        // Backwards-compatible legacy fixture shape.
        ["features", _, platform, "cells", rest @ ..]
            if is_platform(platform) && !rest.is_empty() =>
        {
            true
        }
        ["features", _, platform, "views", _, rest @ ..]
            if is_platform(platform) && !rest.is_empty() =>
        {
            true
        }

        // --- Legacy frontends wrapper (pre-Wave-H pilots) ---
        // frontends/web/shell/<...>.tsx
        // frontends/mobile/theme/<...>.tsx
        ["frontends", platform, subdir, rest @ ..]
            if is_platform(platform)
                && matches!(*subdir, "shell" | "theme" | "ui" | "hooks" | "lib")
                && !rest.is_empty() =>
        {
            true
        }

        // frontends/web/main.tsx, vite.config.ts, tailwind.config.ts
        ["frontends", platform, file]
            if is_platform(platform)
                && matches!(
                    *file,
                    "main.tsx" | "main.ts" | "vite.config.ts" | "tailwind.config.ts"
                ) =>
        {
            true
        }

        _ => false,
    }
}

/// Top-level folders of the post-Wave-H 7-folder closed catalog,
/// excluding `cells/` (which has its own match arm because it requires a
/// `<feature>` sub-bucket) and `ui/` (which has a closed sub-catalog).
///
/// Per `[[client_src_canonical_architecture_2026-05-17]]` §3.1 the 7 are
/// `{shell, routes, ui, theme, state, assets, cells}`. This helper covers
/// the 4 that accept any rest path (`shell`, `routes`, `theme`, `state`)
/// plus `assets` (static files).
fn is_client_src_top_level(name: &str) -> bool {
    matches!(name, "shell" | "routes" | "theme" | "state" | "assets")
}

/// Closed catalog for `ui/` sub-buckets per
/// `[[client_src_canonical_architecture_2026-05-17]]` §3.2.
fn is_ui_sub_bucket(name: &str) -> bool {
    matches!(
        name,
        "forms" | "feedback" | "navigation" | "display" | "overlays" | "layout"
    )
}

/// Barrel-file names allowed at a folder's top level (a single
/// re-export hub for the folder's primitives). Common pilot convention
/// for `ui/index.ts` etc.; not part of the closed catalog but harmless.
fn is_barrel_file(file: &str) -> bool {
    matches!(file, "index.ts" | "index.tsx")
}

/// File names that may live directly under `app/clients/<name>/src/` or
/// `app/web/` (the client wrapper root). These are the bootstrap entry
/// points + Vite/TS housekeeping files that scaffolds emit at the
/// wrapper level.
fn is_client_src_wrapper_file(file: &str) -> bool {
    matches!(
        file,
        "main.tsx" | "main.ts" | "index.ts" | "index.tsx" | "App.tsx" | "App.ts" | "vite-env.d.ts"
    )
}

/// File names that may live directly under `app/clients/<name>/` (one
/// level above `src/` — the client root). These are tooling configs
/// that have to sit next to `package.json` so Vite / Playwright /
/// Tailwind pick them up. Only `.ts(x)` entries are listed — sibling
/// `.cjs` / `.mjs` / `.js` configs are out of scope for this rule (it
/// only scans `.ts(x)` files; see `is_ts_or_tsx_file`).
fn is_client_wrapper_config_file(file: &str) -> bool {
    matches!(
        file,
        "vite.config.ts"
            | "vitest.config.ts"
            | "playwright.config.ts"
            | "tailwind.config.ts"
            | "postcss.config.ts"
            | "nativewind-env.d.ts"
            | "env.d.ts"
    )
}

fn is_ts_or_tsx_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("ts" | "tsx")
    )
}

fn is_test_or_story_file(path: &Path) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    name.ends_with(".test.tsx")
        || name.ends_with(".test.ts")
        || name.ends_with(".spec.tsx")
        || name.ends_with(".spec.ts")
        || name.ends_with(".stories.tsx")
        || name.ends_with(".stories.ts")
}

fn should_skip_dir(name: &str) -> bool {
    matches!(
        name,
        "node_modules"
            | "dist"
            | ".lazuli"
            | ".git"
            | "target"
            | ".next"
            | ".expo"
            | ".cache"
            | "coverage"
    )
}

fn path_parts(path: &Path) -> Vec<&str> {
    path.components()
        .filter_map(|component| component.as_os_str().to_str())
        .collect()
}

fn is_platform(part: &str) -> bool {
    matches!(part, "web" | "mobile")
}

/// True if `path` is the `app/clients/external/` subtree under `root` —
/// the polyglot escape-hatch per `[[lazurite_monorepo_shape_2026-05-17]]`
/// §2.2 (revised 2026-05-18). Non-Lazuli frontends (Astro marketing,
/// Rails dashboards, etc.) live here and are excluded from canon walks.
fn is_external_clients_subtree(root: &Path, path: &Path) -> bool {
    path == root.join("app").join("clients").join("external")
}

#[cfg(test)]
mod test_support;
#[cfg(test)]
mod tests_basic;
#[cfg(test)]
mod tests_wave_h;
