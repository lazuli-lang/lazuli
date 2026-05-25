//! Diagnostics for the canonical-block order and the per-context
//! closed-kind catalogs that catch typos at the keystroke.
//!
//! Two related families share this module because they both consume
//! the same set of closed-catalog constants (`FEATURE_BODY_KINDS`,
//! `APP_BODY_KINDS`, etc.) and the same Damerau-Levenshtein typo
//! suggestion infrastructure:
//!
//! | Producer | Concern |
//! |---|---|
//! | [`canonical_order_diagnostics`] | every feature block emits children in the canonical order `meta -> defaults -> uses -> refs -> domain -> policies -> errors -> auth -> command -> api -> workflow -> job -> webhook -> surface -> extensions -> escape_route`. |
//! | [`feature_unknown_kind_diagnostics`] | indent-2 children of `feature X` use `FEATURE_BODY_KINDS`. |
//! | [`app_unknown_kind_diagnostics`] | indent-2 children of `app X` use `APP_BODY_KINDS`. |
//! | [`registry_unknown_kind_diagnostics`] | indent-2 children of `registry` use `REGISTRY_BODY_KINDS`. |
//! | [`view_unknown_kind_diagnostics`] | indent-2 children of `view X` use `VIEW_BODY_KINDS`. |
//! | [`surface_unknown_kind_diagnostics`] | indent-2 children of `surface X` use `SURFACE_BODY_KINDS`. |
//! | [`command_statement_unknown_diagnostics`] | indent-4 statements inside `command X` use `COMMAND_STATEMENT_KINDS`. |
//! | [`query_statement_unknown_diagnostics`] | indent-6 statements inside `query.* X` use `QUERY_STATEMENT_KINDS`. |
//! | [`audience_unknown_kind_diagnostics`] | indent-4 children of `audience X` use `AUDIENCE_BODY_KINDS`. |
//!
//! Each producer emits an `ERROR` with the closest known kind via
//! `closest_kind` (Damerau-Levenshtein ≤ 2). The catalog constants
//! are exposed `pub(crate)` so this module can also feed the LSP
//! completion items in `crate::completion_items`.

use std::collections::HashSet;

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range};

use crate::{feature_name, leading_spaces, simple_canonical_diagnostic};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CanonicalBlockKind {
    Meta,
    Defaults,
    Uses,
    Refs,
    Domain,
    Policies,
    Errors,
    Auth,
    Command,
    Api,
    Workflow,
    Job,
    Webhook,
    Surface,
    Extensions,
    EscapeRoute,
}

impl CanonicalBlockKind {
    pub(crate) fn rank(self) -> u8 {
        match self {
            Self::Meta => 0,
            Self::Defaults => 1,
            Self::Uses => 2,
            Self::Refs => 3,
            Self::Domain => 4,
            Self::Policies => 5,
            Self::Errors => 6,
            Self::Auth => 7,
            Self::Command => 8,
            Self::Api => 9,
            Self::Workflow => 10,
            Self::Job => 11,
            Self::Webhook => 12,
            Self::Surface => 13,
            Self::Extensions => 14,
            Self::EscapeRoute => 15,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Meta => "meta",
            Self::Defaults => "defaults",
            Self::Uses => "uses",
            Self::Refs => "refs",
            Self::Domain => "domain",
            Self::Policies => "policies",
            Self::Errors => "errors",
            Self::Auth => "auth",
            Self::Command => "command",
            Self::Api => "api",
            Self::Workflow => "workflow",
            Self::Job => "job",
            Self::Webhook => "webhook",
            Self::Surface => "surface",
            Self::Extensions => "extensions",
            Self::EscapeRoute => "escape_route",
        }
    }
}

pub(crate) const CANONICAL_FEATURE_ORDER: &str = "meta -> defaults -> uses -> refs -> domain -> policies -> errors -> auth -> command -> api -> workflow -> job -> webhook -> surface -> extensions -> escape_route";

pub(crate) fn canonical_order_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut current_feature: Option<CanonicalFeatureOrder> = None;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if leading_spaces(line) == 0 && trimmed.starts_with("feature ") {
            current_feature = Some(CanonicalFeatureOrder::new(feature_name(trimmed)));
            continue;
        }

        let Some(feature) = current_feature.as_mut() else {
            continue;
        };

        if leading_spaces(line) != 2 {
            continue;
        }

        let Some(kind) = canonical_block_kind(trimmed) else {
            continue;
        };

        if let Some(previous) = feature.last_kind {
            if kind.rank() < previous.rank() {
                diagnostics.push(canonical_order_diagnostic(
                    line_index,
                    line,
                    &feature.name,
                    kind,
                    previous,
                ));
                continue;
            }
        }

        feature.last_kind = Some(kind);
    }

    diagnostics
}

#[derive(Debug)]
pub(crate) struct CanonicalFeatureOrder {
    name: String,
    last_kind: Option<CanonicalBlockKind>,
}

impl CanonicalFeatureOrder {
    fn new(name: String) -> Self {
        Self {
            name,
            last_kind: None,
        }
    }
}

pub(crate) fn canonical_order_diagnostic(
    line_index: usize,
    line: &str,
    feature_name: &str,
    found: CanonicalBlockKind,
    previous: CanonicalBlockKind,
) -> Diagnostic {
    Diagnostic {
        range: Range {
            start: Position {
                line: line_index as u32,
                character: leading_spaces(line) as u32,
            },
            end: Position {
                line: line_index as u32,
                character: line.len().max(leading_spaces(line) + 1) as u32,
            },
        },
        severity: Some(DiagnosticSeverity::ERROR),
        code: Some(tower_lsp::lsp_types::NumberOrString::String(
            "canonical-order".to_owned(),
        )),
        code_description: None,
        source: Some("lazuli-canonical".to_owned()),
        message: format!(
            "non-canonical block order in feature `{feature_name}`: `{}` appears after `{}`. Expected order: {CANONICAL_FEATURE_ORDER}.",
            found.label(),
            previous.label()
        ),
        related_information: None,
        tags: None,
        data: None,
    }
}

pub(crate) fn canonical_block_kind(trimmed_line: &str) -> Option<CanonicalBlockKind> {
    let first = trimmed_line.split_whitespace().next()?;

    match first {
        "purpose" | "non_goals" | "context" => Some(CanonicalBlockKind::Meta),
        "defaults" => Some(CanonicalBlockKind::Defaults),
        "uses" => Some(CanonicalBlockKind::Uses),
        "refs" => Some(CanonicalBlockKind::Refs),
        "domain" => Some(CanonicalBlockKind::Domain),
        "policies" => Some(CanonicalBlockKind::Policies),
        "errors" => Some(CanonicalBlockKind::Errors),
        "auth" => Some(CanonicalBlockKind::Auth),
        "command" => Some(CanonicalBlockKind::Command),
        "api" => Some(CanonicalBlockKind::Api),
        "workflow" => Some(CanonicalBlockKind::Workflow),
        "job" => Some(CanonicalBlockKind::Job),
        "webhook" => Some(CanonicalBlockKind::Webhook),
        "surface" => Some(CanonicalBlockKind::Surface),
        "extensions" => Some(CanonicalBlockKind::Extensions),
        "escape_route" => Some(CanonicalBlockKind::EscapeRoute),
        _ => None,
    }
}

/// Closed catalog of every keyword that can introduce an indent-2
/// child of `feature X`. Used by `feature_unknown_kind_diagnostics`
/// to detect typos like `comand` (command) / `quiery` (query) /
/// `wokflow` (workflow) — surfaced 2026-05-15 when Lucas wrote
/// `comand move` and the LSP stayed silent.
///
/// Keep this list aligned with the parser's accepted feature-body
/// vocabulary. Sorted alphabetically for diff hygiene.
pub(crate) const FEATURE_BODY_KINDS: &[&str] = &[
    "agent",
    "aggregate",
    "api",
    "attach_ctx",
    "auth",
    "cache",
    "channel",
    "command",
    "compatibility",
    "context",
    "defaults",
    "delegated_to",
    "domain",
    "emits",
    "enum",
    "errors",
    "escape_route",
    "event",
    "event.trace",
    "event_group",
    "events",
    "extends",
    "extensions",
    "import",
    "imports",
    "invariants",
    "job",
    "mcp_server",
    "non_goals",
    "notification",
    "operation",
    "out_of_scope",
    "permission",
    "poller",
    "policies",
    "purpose",
    "query.list",
    "query.lookup",
    "query.sql",
    "query.view",
    "record",
    "refs",
    "report",
    "requires",
    "role",
    "secret_rotation",
    "subscription",
    "surface",
    "tenant_migration",
    "tests",
    "tools",
    "translation",
    "uses",
    "view",
    "webhook",
    "webhook_event",
    "workflow",
];

/// 2026-05-15 — file-local diagnostic that flags any indent-2 word
/// inside `feature X` body which is NOT a known kind keyword.
/// Suggests the closest known kind via Damerau-Levenshtein distance ≤ 2
/// when one exists; otherwise lists all valid kinds. Fires as a
/// WARNING (not ERROR) so the user can keep typing while the squiggle
/// nudges them to fix.
///
/// Ignores comments, blank lines, and lines whose first token starts
/// with `@` (decorator/anchor reference) or contains `(`/`:` (typed
/// field decl, namespaced-decorator call, key-value).
pub(crate) fn feature_unknown_kind_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut inside_feature = false;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let leading = leading_spaces(line);
        if leading == 0 {
            inside_feature = trimmed.starts_with("feature ");
            continue;
        }
        if !inside_feature || leading != 2 {
            continue;
        }
        let Some(first) = trimmed.split_whitespace().next() else {
            continue;
        };
        // Skip decorators, anchors, namespaced refs, key-value lines.
        if first.starts_with('@')
            || first.contains('(')
            || first.contains(':')
            || first.contains('=')
        {
            continue;
        }
        if FEATURE_BODY_KINDS.contains(&first) {
            continue;
        }
        let suggestion = closest_feature_body_kind(first, 2);
        let message = match suggestion {
            Some(suggested) => {
                format!("unknown feature block kind `{first}`. Did you mean `{suggested}`?")
            }
            None => format!(
                "unknown feature block kind `{first}`. Valid kinds: command / api / query.list / query.lookup / query.sql / query.view / view / webhook / job / agent / notification / poller / report / channel / cache / aggregate / events / event_group / event.trace / workflow / surface / extensions / tests / auth / errors / policies / domain / defaults / uses / purpose / context / non_goals / role / permission / etc."
            ),
        };
        // ERROR not WARNING: an unknown kind keyword causes the
        // parser to SILENTLY skip the entire block — the user-intended
        // command/api/query never enters the IR, and the regenerated
        // dist looks like the feature simply forgot to declare it.
        // Compile-blocking visibility is the right choice.
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::ERROR,
            "feature-unknown-kind",
            &message,
        ));
    }

    diagnostics
}

/// Damerau-Levenshtein-style closest match against `FEATURE_BODY_KINDS`.
/// Returns the closest kind when distance ≤ `max_distance`, else `None`.
/// Plain Levenshtein (no transposition) is enough for our typo cases
/// (`comand` / `quiery` / `wokflow`) — adjacent-swap support adds
/// complexity without much gain at our scale.
pub(crate) fn closest_feature_body_kind(word: &str, max_distance: usize) -> Option<&'static str> {
    closest_kind(word, FEATURE_BODY_KINDS, max_distance)
}

/// Generic closest-kind matcher shared by every `<context>-unknown-kind`
/// diagnostic. Returns the closest catalog entry when its plain-Levenshtein
/// edit distance is ≤ `max_distance`, else `None`. The catalog is a
/// `&'static [&'static str]` so the returned suggestion can flow into the
/// diagnostic message without heap-allocating per call.
///
/// Added 2026-05-15 alongside the typo-detection sweep that promoted six
/// other contexts (app/registry/view/surface/command-statement/
/// query-statement/audience) to the same closed-catalog treatment as
/// `feature_unknown_kind_diagnostics`. Reuse this — do NOT copy-paste the
/// O(n*m) loop into each new diagnostic.
pub(crate) fn closest_kind(
    word: &str,
    catalog: &[&'static str],
    max_distance: usize,
) -> Option<&'static str> {
    let mut best: Option<(&'static str, usize)> = None;
    for &candidate in catalog {
        let d = levenshtein(word, candidate);
        if d > max_distance {
            continue;
        }
        match best {
            None => best = Some((candidate, d)),
            Some((_, prev_d)) if d < prev_d => best = Some((candidate, d)),
            _ => {}
        }
    }
    best.map(|(k, _)| k)
}

/// Plain Levenshtein edit distance. O(n*m) DP. Used only for short
/// keyword names so the cost is negligible.
pub(crate) fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let n = a.len();
    let m = b.len();
    if n == 0 {
        return m;
    }
    if m == 0 {
        return n;
    }
    let mut prev: Vec<usize> = (0..=m).collect();
    let mut curr: Vec<usize> = vec![0; m + 1];
    for i in 1..=n {
        curr[0] = i;
        for j in 1..=m {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            curr[j] = (curr[j - 1] + 1).min(prev[j] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[m]
}

// =============================================================================
// 2026-05-15 — Typo-detection asymmetry sweep (R1.C audit follow-up).
//
// `feature_unknown_kind_diagnostics` covers ONE context: indent-2 lines
// inside `feature X`. The sweep surfaced 7 other contexts where a typo
// in a kind keyword silently breaks compilation (parser drops the block,
// IR loses the declaration, regenerated `dist/` looks like the user
// simply forgot to write the command/api/view). Each helper below
// follows the same skeleton: walk lines, detect "inside the context",
// at the appropriate sub-indent check the first token against a closed
// `<CONTEXT>_BODY_KINDS` catalog, skip decorator/field/assignment lines,
// and emit ERROR with a `closest_kind` suggestion when the distance is
// ≤ 2.
//
// All diagnostics share the `levenshtein` + `closest_kind` infrastructure
// — the catalogs are the only per-context state. Each catalog is sorted
// alphabetically for diff hygiene; keep new entries in order.
// =============================================================================

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
/// `settings`) and the route/extends/anchor/audience scaffolding from
/// the L0 #6 grammar.
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
    "policy",
    "prerender",
    "route",
    "search",
    "sections",
    "selection",
    "settings",
    "slot",
    "sort",
    "source",
    "submit",
];

/// Closed catalog of indent-2 child kinds inside `surface X <platform>`
/// (canonical `.lzi`). The `.lzi` surface accepts both the
/// audience-grouped form (`audience <name>` with nested views) and the
/// flat form (`view <name> <Component>` directly under surface). Plus
/// the `uses experience` declaration. The `uses` prefix-form is
/// matched on the head token alone.
pub(crate) const SURFACE_BODY_KINDS: &[&str] = &["audience", "uses", "view"];

/// Closed catalog of indent-4 statement keywords inside a `command X`
/// body. Mirrors `parse_command_decl`'s prefix dispatch table. The
/// effect targets (`creates Foo`, `updates Foo`, etc.) are statement
/// keywords; the resource name they target is filtered out by the
/// per-line capitalized-identifier check (a bare `Customer` line is a
/// stray token, not an unknown kind).
/// Sorted alphabetically.
pub(crate) const COMMAND_STATEMENT_KINDS: &[&str] = &[
    "approval",
    "audit",
    "calls",
    "creates",
    "deletes",
    "deprecated",
    "emits",
    "gate",
    "handler",
    "idempotency",
    "input",
    "invalidates",
    "let",
    "policy",
    "previously",
    "rate_limit",
    "retry",
    "returns",
    "route",
    "target",
    "tests",
    "timeout",
    "updates",
    "validate",
    "write_window",
];

/// Closed catalog of indent-4 statement keywords inside `query.list`,
/// `query.lookup`, `query.sql`, and `query.view` bodies. Mirrors the prefix-dispatch
/// arms in `parse_query_list_decl`, `parse_query_lookup_decl`, and
/// `parse_query_sql_decl`. Includes the union (a `query.lookup` body
/// will never use `cache`/`paginate`/`order` — but flagging those as
/// typos would cause false positives across query kinds, so we accept
/// them and let the parser emit the precise per-kind error).
/// Sorted alphabetically.
pub(crate) const QUERY_STATEMENT_KINDS: &[&str] = &[
    "cache", "filters", "gate", "modifier", "order", "paginate", "params", "policy", "returns",
    "scope", "search", "source", "sql",
];

/// Closed catalog of children inside `audience <name>` blocks.
/// Per `parse_lzx_audience_block`, ONLY `requires @scope.<name>` and
/// `view list|detail|create <name>` are valid. The `requires` lines
/// are filtered out by the leading-`@` skip; we catalog the bare kind
/// keywords here.
pub(crate) const AUDIENCE_BODY_KINDS: &[&str] = &["policy", "requires", "view"];

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

        let (_header_indent, body_indent) = current_view.unwrap();
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
                "unknown view body kind `{first}`. Valid kinds: source / submit / columns / fields / sections / cells / route / actions / search / filter / filters / drawer / sort / selection / bulk_actions / settings / block / slot / extends / extensible_by / anchor / audience / lazy / prerender."
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

        let (_header_indent, body_indent) = current_surface.unwrap();
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

/// 2026-05-15 — Indent-4 statement keywords inside `command X` body.
/// A typo like `audt` silently drops audit metadata; `inalidates`
/// strips cache invalidation. Both regenerate clean Go that quietly
/// loses behavior — exactly the silent-failure mode this lint catches.
///
/// Skips lines that are NOT keyword-statements: assignments (`x = ...`),
/// effect targets (capitalized identifier like `Customer`), field-name
/// lines inside `input`/`output` sub-blocks (which carry `:`).
pub(crate) fn command_statement_unknown_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut current_command: Option<(usize, usize)> = None;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let leading = leading_spaces(line);

        if let Some((header_indent, _body_indent)) = current_command {
            if leading <= header_indent {
                current_command = None;
            }
        }

        if current_command.is_none() {
            if trimmed.starts_with("command ") {
                current_command = Some((leading, leading + 2));
            }
            continue;
        }

        let (_header_indent, body_indent) = current_command.unwrap();
        if leading != body_indent {
            continue;
        }
        let Some(first) = trimmed.split_whitespace().next() else {
            continue;
        };
        // Skip decorators, namespaced refs, key-value lines, assignments.
        // The `=` and `:` checks scan the WHOLE trimmed line because
        // command bodies host `<field> = <expr>` and `<field>: <Type>`
        // forms where the LHS identifier carries no punctuation.
        if first.starts_with('@')
            || first.contains('(')
            || trimmed.contains(':')
            || trimmed.contains('=')
        {
            continue;
        }
        // Skip effect-target "bare resource" lines like `Customer` (a
        // capitalized identifier on its own is not a statement keyword).
        if first
            .chars()
            .next()
            .map(|c| c.is_ascii_uppercase())
            .unwrap_or(false)
        {
            continue;
        }
        if COMMAND_STATEMENT_KINDS.contains(&first) {
            continue;
        }
        let suggestion = closest_kind(first, COMMAND_STATEMENT_KINDS, 2);
        let message = match suggestion {
            Some(suggested) => {
                format!("unknown command statement `{first}`. Did you mean `{suggested}`?")
            }
            None => format!(
                "unknown command statement `{first}`. Valid statements: previously / route / input / policy / rate_limit / audit / approval / target / let / validate / creates / updates / deletes / returns / handler / emits / invalidates / calls / timeout / retry / idempotency / write_window / tests / deprecated / gate."
            ),
        };
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::ERROR,
            "command-statement-unknown",
            &message,
        ));
    }

    diagnostics
}

/// 2026-05-15 — Indent-4 statement keywords inside `query.list`,
/// `query.lookup`, `query.sql`, and `query.view` bodies. A typo like `paginat` silently
/// drops pagination; `cahce` drops the cache profile binding.
pub(crate) fn query_statement_unknown_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut current_query: Option<(usize, usize)> = None;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let leading = leading_spaces(line);

        if let Some((header_indent, _body_indent)) = current_query {
            if leading <= header_indent {
                current_query = None;
            }
        }

        if current_query.is_none() {
            if trimmed.starts_with("query.list ")
                || trimmed.starts_with("query.lookup ")
                || trimmed.starts_with("query.sql ")
                || trimmed.starts_with("query.view ")
            {
                current_query = Some((leading, leading + 2));
            }
            continue;
        }

        let (_header_indent, body_indent) = current_query.unwrap();
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
        if QUERY_STATEMENT_KINDS.contains(&first) {
            continue;
        }
        let suggestion = closest_kind(first, QUERY_STATEMENT_KINDS, 2);
        let message = match suggestion {
            Some(suggested) => {
                format!("unknown query statement `{first}`. Did you mean `{suggested}`?")
            }
            None => format!(
                "unknown query statement `{first}`. Valid statements: policy / params / filters / scope / modifier / search / cache / paginate / order / returns / sql / source / gate."
            ),
        };
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::ERROR,
            "query-statement-unknown",
            &message,
        ));
    }

    diagnostics
}

/// 2026-05-15 — Children of `audience <name>` blocks. Per the parser's
/// strict check (`parse_lzx_audience` line 962), ONLY `view <name>
/// <Component>` lines are accepted. A typo like `vieww list ItemList`
/// causes the parser to bail with a generic shape error; this lint
/// turns it into a precise typo suggestion.
pub(crate) fn audience_unknown_kind_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut current_audience: Option<(usize, usize)> = None;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let leading = leading_spaces(line);

        if let Some((header_indent, _body_indent)) = current_audience {
            if leading <= header_indent {
                current_audience = None;
            }
        }

        if current_audience.is_none() {
            if trimmed.starts_with("audience ") {
                current_audience = Some((leading, leading + 2));
            }
            continue;
        }

        let (_header_indent, body_indent) = current_audience.unwrap();
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
        if AUDIENCE_BODY_KINDS.contains(&first) {
            continue;
        }
        let suggestion = closest_kind(first, AUDIENCE_BODY_KINDS, 2);
        let message = match suggestion {
            Some(suggested) => {
                format!("unknown audience child `{first}`. Did you mean `{suggested}`?")
            }
            None => format!(
                "unknown audience child `{first}`. Valid children: `view <name> <Component>` declarations (or `requires @scope.<name>`)."
            ),
        };
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::ERROR,
            "audience-unknown-kind",
            &message,
        ));
    }

    diagnostics
}
