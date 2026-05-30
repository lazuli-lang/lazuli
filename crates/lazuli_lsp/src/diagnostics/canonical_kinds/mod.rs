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

mod feature;
mod sections;

// Re-exports for `pub(crate) use diagnostics::canonical_kinds::*;`
// in lib.rs and direct consumers in `dispatch.rs`. Catalogs are
// surfaced too (not currently consumed outside this directory but
// kept exported for parity with the pre-split API surface).
#[allow(unused_imports)]
pub(crate) use feature::{FEATURE_BODY_KINDS, feature_unknown_kind_diagnostics};
#[allow(unused_imports)]
pub(crate) use sections::{
    APP_BODY_KINDS, AUDIENCE_BODY_KINDS, COMMAND_STATEMENT_KINDS, QUERY_STATEMENT_KINDS,
    REGISTRY_BODY_KINDS, SESSIONS_BODY_KINDS, SESSIONS_COOKIE_BODY_KINDS, SURFACE_BODY_KINDS,
    VIEW_BODY_KINDS, app_unknown_kind_diagnostics, audience_unknown_kind_diagnostics,
    command_statement_unknown_diagnostics, query_statement_unknown_diagnostics,
    registry_unknown_kind_diagnostics, sessions_unknown_kind_diagnostics,
    surface_unknown_kind_diagnostics, view_unknown_kind_diagnostics,
};

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

        if let Some(previous) = feature.last_kind
            && kind.rank() < previous.rank()
        {
            diagnostics.push(canonical_order_diagnostic(
                line_index,
                line,
                &feature.name,
                kind,
                previous,
            ));
            continue;
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
        "purpose" | "non_goals" | "knowledge" | "context" => Some(CanonicalBlockKind::Meta),
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
