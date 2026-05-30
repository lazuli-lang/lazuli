//! Indent-2 block-kind catalogs and typo diagnostics for the four
//! top-level container blocks: `app`, `registry`, `view`, `surface`.
//!
//! Every diagnostic in this module shares the same skeleton: detect
//! "inside the context" at indent-0 (or by header scan with body-indent
//! tracking), at the appropriate sub-indent check the first token
//! against a closed `*_BODY_KINDS` catalog, skip
//! decorator/field/assignment lines, and emit ERROR with a
//! `closest_kind` suggestion when distance ≤ 2.

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use super::super::closest_kind;
use crate::{leading_spaces, simple_canonical_diagnostic};

/// Closed catalog of indent-2 child kinds inside `app <name>`.
/// Mirrors `app_child_block` (block headers) ∪ `is_app_scalar_child`
/// (scalar one-liners). Kept manually in sync with both helpers; if
/// either grows a new keyword, add it here too.
/// Sorted alphabetically for diff hygiene.
pub(crate) const APP_BODY_KINDS: &[&str] = &[
    "architecture",
    "actor_query",
    "auth_failed_redirect",
    "bindings",
    "capabilities",
    "communication",
    "cookie",
    "cors",
    "default_locale",
    "default_timezone",
    "deploy",
    "encryption",
    "env",
    "environments",
    "error_page",
    "headers",
    "integrations",
    "lazuli_version",
    "limits",
    "locale",
    "logging",
    "not_found",
    "packs",
    "proxy",
    "route_guard",
    "runtime",
    "services",
    "targets",
    "title",
    "tracing",
    "urls",
    "uses",
    "version",
];

/// Closed catalog of indent-2 child kinds inside `registry`.
/// Mirrors `registry_child` in `lazuli_cli/src/app_manifest.rs` plus
/// the LSP's own `registry_contract_diagnostics` allow-list. Sorted.
///
/// B1 (W3-blockers) — `bindings` is registry-level sugar over
/// `integrations`. Same IR, same codegen, but the indent-6
/// child grammar additionally accepts `endpoint env.X` and
/// `auth keys env.A env.B` so authors can write the
/// roadmap §3.5 shape directly.
pub(crate) const REGISTRY_BODY_KINDS: &[&str] = &[
    "bindings",
    "capabilities",
    "env",
    "integrations",
    "packs",
    "secret_rotation",
    "tools",
    "webhook_event",
    "webhook_events",
];

/// Closed catalog of view-body keywords. Mirrors `view_body_handlers`
/// in `lazuli_syntax/src/parser.rs` plus the standalone block handlers
/// (`drawer`, `filters`, `search`, `sort`, `selection`, `bulk_actions`,
/// `settings`, `on_success`) and the route/extends/anchor/audience
/// scaffolding from the L0 #6 grammar. Surface-sync WT-2 added the
/// Wave-W6 / GAP-UX primitives (`wizard_steps`, `tab_group`,
/// `view_mode`, `view.inline_table`, `view.board`, `repeatable`,
/// `tabs`, `wizard`).
pub(crate) const VIEW_BODY_KINDS: &[&str] = &[
    "actions",
    "anchor",
    "audience",
    "block",
    "bulk_actions",
    "cells",
    "columns",
    "drawer",
    "extends",
    "extensible_by",
    "fields",
    "filter",
    "filters",
    "lazy",
    "on_success",
    "policy",
    "prerender",
    "repeatable",
    "route",
    "search",
    "sections",
    "selection",
    "settings",
    "slot",
    "sort",
    "source",
    "submit",
    "tab_group",
    "tabs",
    "view.board",
    "view.inline_table",
    "view_mode",
    "wizard",
    "wizard_steps",
];

/// Closed catalog of indent-2 child kinds inside `surface X <platform>`
/// (canonical `.lzi`). The `.lzi` surface accepts both the
/// audience-grouped form (`audience <name>` with nested views) and the
/// flat form (`view <name> <Component>` directly under surface). Plus
/// the `uses experience` declaration. The `uses` prefix-form is
/// matched on the head token alone.
pub(crate) const SURFACE_BODY_KINDS: &[&str] = &["audience", "uses", "view"];

/// 2026-05-15 — Indent-2 kind keywords inside `app <name>`. Without
/// this lint, a typo like `urls` → `urs` is silently dropped by the
/// parser, no diagnostic, and the regenerated app forgets every URL.
pub(crate) fn app_unknown_kind_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut inside_app = false;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let leading = leading_spaces(line);
        if leading == 0 {
            inside_app = trimmed.starts_with("app ");
            continue;
        }
        if !inside_app || leading != 2 {
            continue;
        }
        let Some(first) = trimmed.split_whitespace().next() else {
            continue;
        };
        if first.starts_with('@')
            || trimmed.contains('(')
            || trimmed.contains(':')
            || trimmed.contains('=')
        {
            continue;
        }
        if APP_BODY_KINDS.contains(&first) {
            continue;
        }
        let suggestion = closest_kind(first, APP_BODY_KINDS, 2);
        let message = match suggestion {
            Some(suggested) => {
                format!("unknown app block kind `{first}`. Did you mean `{suggested}`?")
            }
            None => format!(
                "unknown app block kind `{first}`. Valid kinds: title / version / lazuli_version / targets / bindings / packs / environments / urls / cors / headers / cookie / proxy / limits / env / integrations / capabilities / architecture / services / communication / runtime / deploy / logging / tracing / locale / encryption / error_page / uses / default_locale / default_timezone / auth_failed_redirect / route_guard / actor_query / not_found."
            ),
        };
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::ERROR,
            "app-unknown-kind",
            &message,
        ));
    }

    diagnostics
}

/// 2026-05-15 — Indent-2 kind keywords inside `registry`. A typo like
/// `webhook_evnts` (vs `webhook_events`) silently drops the registry
/// catalog, leaving downstream webhooks unable to resolve their typed
/// envelope shape.
pub(crate) fn registry_unknown_kind_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut inside_registry = false;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let leading = leading_spaces(line);
        if leading == 0 {
            inside_registry = trimmed == "registry" || trimmed.starts_with("registry ");
            continue;
        }
        if !inside_registry || leading != 2 {
            continue;
        }
        let Some(first) = trimmed.split_whitespace().next() else {
            continue;
        };
        if first.starts_with('@')
            || trimmed.contains('(')
            || trimmed.contains(':')
            || trimmed.contains('=')
        {
            continue;
        }
        if REGISTRY_BODY_KINDS.contains(&first) {
            continue;
        }
        let suggestion = closest_kind(first, REGISTRY_BODY_KINDS, 2);
        let message = match suggestion {
            Some(suggested) => {
                format!("unknown registry block kind `{first}`. Did you mean `{suggested}`?")
            }
            None => format!(
                "unknown registry block kind `{first}`. Valid kinds: env / capabilities / integrations / bindings / packs / tools / webhook_event / webhook_events / secret_rotation."
            ),
        };
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::ERROR,
            "registry-unknown-kind",
            &message,
        ));
    }

    diagnostics
}

/// 2026-05-15 — Body kinds inside `view <name> <Component>` (L0 #6
/// view body). A typo like `selecton` (vs `selection`) silently strips
/// row-selection from the rendered list view; `colums` (vs `columns`)
/// produces an empty grid.
pub(crate) fn view_unknown_kind_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    // Stack of (header_indent, body_indent) for the currently open view block.
    let mut current_view: Option<(usize, usize)> = None;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let leading = leading_spaces(line);

        // Close view scope when indentation returns to (or above) the header.
        if let Some((header_indent, _body_indent)) = current_view {
            if leading <= header_indent {
                current_view = None;
            }
        }

        if current_view.is_none() {
            // Detect a view header — `view list <name>`, `view detail <name>`,
            // `view create <name>`, or `view <name> <Component>` (legacy form).
            if trimmed.starts_with("view ") {
                current_view = Some((leading, leading + 2));
            }
            continue;
        }

        // `current_view.is_none()` continued the loop above, so this is Some.
        let Some((_header_indent, body_indent)) = current_view else {
            continue;
        };
        if leading != body_indent {
            continue;
        }
        let Some(first) = trimmed.split_whitespace().next() else {
            continue;
        };
        if first.starts_with('@')
            || trimmed.contains('(')
            || trimmed.contains(':')
            || trimmed.contains('=')
        {
            continue;
        }
        if VIEW_BODY_KINDS.contains(&first) {
            continue;
        }
        let suggestion = closest_kind(first, VIEW_BODY_KINDS, 2);
        let message = match suggestion {
            Some(suggested) => {
                format!("unknown view body kind `{first}`. Did you mean `{suggested}`?")
            }
            None => format!(
                "unknown view body kind `{first}`. Valid kinds: source / submit / on_success / columns / fields / sections / cells / route / actions / search / filter / filters / drawer / sort / selection / bulk_actions / settings / block / slot / extends / extensible_by / anchor / audience / lazy / prerender / wizard_steps / tab_group / tabs / wizard / view_mode / view.inline_table / view.board / repeatable."
            ),
        };
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::ERROR,
            "view-unknown-kind",
            &message,
        ));
    }

    diagnostics
}

/// 2026-05-15 — Indent-2 kind keywords inside `surface X <platform>`.
/// Only `uses experience` and `audience` are valid children. A typo
/// like `audeince` silently drops the entire audience subtree, taking
/// every view inside it with no diagnostic.
pub(crate) fn surface_unknown_kind_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    // Stack of (header_indent, body_indent) for the currently open surface.
    let mut current_surface: Option<(usize, usize)> = None;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let leading = leading_spaces(line);

        if let Some((header_indent, _body_indent)) = current_surface {
            if leading <= header_indent {
                current_surface = None;
            }
        }

        if current_surface.is_none() {
            if trimmed.starts_with("surface ") {
                current_surface = Some((leading, leading + 2));
            }
            continue;
        }

        // `current_surface.is_none()` continued the loop above, so this is Some.
        let Some((_header_indent, body_indent)) = current_surface else {
            continue;
        };
        if leading != body_indent {
            continue;
        }
        let Some(first) = trimmed.split_whitespace().next() else {
            continue;
        };
        if first.starts_with('@')
            || trimmed.contains('(')
            || trimmed.contains(':')
            || trimmed.contains('=')
        {
            continue;
        }
        if SURFACE_BODY_KINDS.contains(&first) {
            continue;
        }
        let suggestion = closest_kind(first, SURFACE_BODY_KINDS, 2);
        let message = match suggestion {
            Some(suggested) => {
                format!("unknown surface body kind `{first}`. Did you mean `{suggested}`?")
            }
            None => format!(
                "unknown surface body kind `{first}`. Valid children: `uses experience <name>`, `audience <name>`."
            ),
        };
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::ERROR,
            "surface-unknown-kind",
            &message,
        ));
    }

    diagnostics
}
