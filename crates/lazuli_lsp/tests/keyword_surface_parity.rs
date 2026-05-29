//! Cross-surface keyword parity gate — **re-rooted on `lazuli_keywords`
//! (Wave H3).**
//!
//! Enforcement for inviolable rule #7 ("magic discovery requires
//! visibility") and the standing discipline that a language construct
//! recognized by the parser must surface in EVERY downstream consumer.
//! Historically the parser grew keywords while the LSP catalog, the VS
//! Code highlighter, and the grammar docs silently drifted behind it
//! (e.g. `attach_ctx` parsed but was unhighlighted, undocumented, and
//! absent from completion).
//!
//! Before H3 this test asserted a hand-curated 22-token `CANONICAL`
//! list. That made coverage a *curated sample* — a keyword could be
//! added to the parser and forgotten everywhere except this list, which
//! itself was also forgotten. H3 re-roots the gate on
//! [`lazuli_keywords::ALL`] (proven complete against the parser by Wave
//! H1), so "every keyword is highlighted + completed + documented" is now
//! **by-construction**: the test iterates the registry, not a sample.
//!
//! Scope of enforcement (per the H3 work-order, option (b)):
//!
//! 1. **LSP catalog — FULLY enforced.** Every registry keyword-token
//!    literal MUST appear in the LSP keyword/hover/typo-catalog sources.
//!    H3 owns this surface; there is no allowlist.
//! 2. **tmLanguage / grammar / quickref — allowlisted gap.** Re-rooting
//!    on the full registry surfaced a large, pre-existing documentation
//!    debt (≈200 keyword literals never added to `grammar.*.md` /
//!    `quickref.md` / the tmLanguage alternations). Mass-fixing the docs
//!    is out of scope for H3, so the [`DOC_SURFACE_GAP`] allowlist
//!    carries those literals with a TODO for sequencing. Any *new*
//!    keyword added to the registry that is missing from these three
//!    surfaces (and not in the allowlist) fails this test — the
//!    by-construction guarantee holds going forward; the allowlist only
//!    grandfathers today's debt.
//!
//! Surfaces checked:
//!   1. LSP        — `crates/lazuli_lsp/src/{keywords.rs, hover/*, diagnostics/canonical_kinds/*}`
//!   2. Highlight  — `editors/vscode/syntaxes/lazuli.tmLanguage.json`
//!   3. Grammar    — `docs/grammar.lzi.md` + `docs/grammar.lzx.md`
//!   4. Quickref   — `docs/quickref.md`
//!
//! Substring presence is intentionally coarse: the point is "did the
//! author remember this surface at all", not exact-form validation.
//!
//! See CLAUDE.md / AGENTS.md §"Language-change surface checklist".

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use lazuli_keywords::{ALL, SemanticToken};

/// Tokens that must NOT appear as live, highlighted feature-header
/// keywords — the dead forms whose silent-drop earlier waves removed. The
/// feature-level `context "..."` was migrated to `attach_ctx` and now
/// hard-errors (`E-CONTEXT-RETIRED`); the retired `workflow` block
/// hard-errors (`E-WORKFLOW-RETIRED`). They must be gone from the LSP
/// feature-body catalog.
const RETIRED_FEATURE_KEYWORDS: &[&str] = &["workflow"];

/// `@semantic.*` scalar VALUES. Unlike keywords these are not in the LSP
/// keyword catalog and are highlighted generically by tmLanguage's
/// `@semantic.<Type>` rule, so the meaningful surfaces are the
/// authoritative analyzer catalog (the framework's source of truth for
/// which semantic scalars exist) plus the docs. These are NOT
/// `lazuli_keywords` literals (the registry carries keywords/constructs,
/// not semantic type names), so they stay an explicit list.
const SEMANTIC_VALUES: &[&str] = &["HexColor", "Percentage"];

/// **Pre-existing documentation debt grandfathered by Wave H3.** Each
/// entry is a registry keyword-token literal that is NOT yet present in
/// one or more of `lazuli.tmLanguage.json` / `grammar.lzi.md` /
/// `grammar.lzx.md` / `quickref.md`. Re-rooting `keyword_surface_parity`
/// on the full registry surfaced these (≈200 literals — header/
/// proxy scalars, deploy + migration keywords, RBAC verbs, surface-view
/// primitives, observability scalars, …). Closing them means editing the
/// tmLanguage grammar + the grammar/quickref docs, which is explicitly
/// out of scope for H3 (LSP codegen). The list is reported back for
/// sequencing as a follow-up wave; until then it keeps the by-
/// construction gate green on everything H1+H2+H3 actually cover while
/// still failing for any NEW keyword that lands without doc/highlight
/// coverage.
///
/// The `cookie-sessions-child` parity pass (the `auth.sessions.cookie`
/// block + its `same_site` / `secure` / `http_only` attributes) retired
/// `cookie` / `same_site` / `secure` / `http_only` from this list once
/// they landed in `grammar.lzi.md` + `quickref.md` (they were already in
/// the generated tmLanguage `#kw-cookie` alternation). `domain` / `path`
/// were never in the gap. They are now enforced by-construction.
///
/// To retire an entry: add the literal to the relevant doc/grammar/
/// tmLanguage surface and delete it here. The test will tell you which
/// surface is still missing it.
const DOC_SURFACE_GAP: &[&str] = &[
    // Audit-block connectors (registry-backfilled via H2): in the generated
    // tmLanguage + LSP catalog, but not yet in grammar.*.md / quickref — same
    // pre-existing doc debt class as the rest of this allowlist.
    "data_subject",
    "retain_for",
    "access_ttl",
    "actions",
    "actor_query",
    "aggregate",
    "architecture",
    "async",
    "auth_failed_redirect",
    "auto_rollback",
    "bindings",
    "body_size",
    "boundaries",
    "breakpoint",
    "bulk_actions",
    "burst",
    "cadence",
    "cells",
    "channel",
    "checkpoint",
    "coalesce",
    "communication",
    "composite_key",
    "constraints",
    "consumes",
    "conventions",
    "cors",
    "counter",
    "credentials",
    "csp",
    "csrf",
    "cursor",
    "dark",
    "data_classification",
    "dedupe",
    "default_locale",
    "default_policy",
    "default_timezone",
    "default_unauthenticated_redirect",
    "default_unauthorized_redirect",
    "deprecated",
    "design",
    "destructive_migrations",
    "detail",
    "digest",
    "dlq",
    "drawer",
    "easing",
    "emit_to",
    "encryption",
    "enforce_service_boundaries",
    "enroll",
    "error_page",
    "error_redact",
    "error_view",
    "event.trace",
    "exporter",
    "exposes",
    "external",
    "fallback",
    "family",
    "flash",
    "foreground",
    "forwarded_host_header",
    "forwarded_proto_header",
    "gateway",
    "golden",
    "grace",
    "grants",
    "grants_all",
    "group_by",
    "has_many",
    "header_size",
    "headers",
    "healthcheck",
    "hide",
    "hint",
    "hsts",
    "icon",
    "include_subdomains",
    "inverse",
    "lanes",
    "lazuli_version",
    "lazy",
    "limits",
    "line_height",
    "loader",
    "locale",
    "lock_timeout",
    "logging",
    "materialize_strategy",
    "max_age",
    "max_attempts",
    "max_per",
    "max_size",
    "max_tokens",
    "mcp_server",
    "migration_lock",
    "min_score",
    "motion",
    "mutate",
    "no_timestamps",
    "not_found",
    "notification",
    "oauth",
    "on_delete",
    "on_lifecycle_pending",
    "on_success",
    "on_unauthenticated",
    "on_unauthorized",
    "outbox",
    "overlap",
    "owns",
    "payload_axis",
    "payload_from",
    "payload_group",
    "pending_view",
    "per_channel",
    "per_recipient",
    "permission",
    "permissions_policy",
    "permits",
    "persist",
    "plural",
    "poller",
    "post_migration_hook",
    "pre_migration_hook",
    "preload",
    "prerender",
    "previous_version",
    "primary",
    "propagate",
    "proxy",
    "query.view",
    "radius",
    "readiness",
    "real_ip_header",
    "recipient",
    "redact",
    "redirect",
    "referrer_policy",
    "refresh",
    "refresh_ttl",
    "replace",
    "replacement",
    "replay",
    "resume",
    "retry_quirk",
    "revoke_session_family",
    "revoke_user",
    "role_mismatch",
    "rollback",
    "root",
    "rotation",
    "rotation_profile",
    "route_guard",
    "runs",
    "sample_rate",
    "scale",
    "secret_rotation",
    "sections",
    "selection",
    "server",
    "serves",
    "service_ready",
    "settings",
    "shadow",
    "shared_registry",
    "since",
    "skeleton",
    "sliding",
    "soft_delete",
    "sort",
    "stale_while_revalidate",
    "strategy",
    "subscription",
    "sunset",
    "supported",
    "template_strategy",
    "tenant_migration",
    "theft_detection_action",
    "throttle",
    "tick",
    "top_p",
    "tracing",
    "tracking",
    "translation",
    "transport",
    "trial",
    "trusted",
    "typography",
    "unlimited",
    "upload_size",
    "urls",
    "version_field",
    "webhook_event",
    "webhook_events",
    "weight",
    "within",
    "write_window",
    "x_content_type_options",
    "x_frame_options",
];

fn workspace_root() -> PathBuf {
    // CARGO_MANIFEST_DIR = <root>/crates/lazuli_lsp
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

fn read(rel: &str) -> String {
    let p = workspace_root().join(rel);
    fs::read_to_string(&p).unwrap_or_else(|e| panic!("cannot read {}: {e}", p.display()))
}

fn lsp_sources() -> String {
    // Files where keyword/kind/hover knowledge lives. Concatenate and search.
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let files = [
        "src/keywords.rs",
        "src/hover/domain.rs",
        "src/hover/surface.rs",
        "src/hover/manifest.rs",
        "src/diagnostics/canonical_kinds/feature.rs",
        "src/diagnostics/canonical_kinds/sections/blocks.rs",
        "src/diagnostics/canonical_kinds/sections/statements.rs",
    ];
    let mut acc = String::new();
    for f in files {
        let p = manifest.join(f);
        if let Ok(s) = fs::read_to_string(&p) {
            acc.push_str(&s);
            acc.push('\n');
        }
    }
    acc
}

/// The substring to search for a literal across surfaces. `@`-decorators
/// are stored bare in the tmLanguage namespace alternation
/// (`@(?:policy|scope|…)`), so strip the sigil; dotted kinds
/// (`query.list`) keep their full form.
fn search_token(literal: &str) -> &str {
    literal.strip_prefix('@').unwrap_or(literal)
}

/// Registry keyword-token literals — the constructs a highlighter +
/// completion + docs must surface. Values / operators / modifiers /
/// type-constructors are highlighted generically (constant.language,
/// keyword.operator, storage.modifier) and are not keyword-catalog
/// entries, so the parity gate is scoped to `SemanticToken::Keyword`
/// rows (plus decorators are covered by their own tmLanguage rule).
fn registry_keyword_literals() -> BTreeSet<&'static str> {
    ALL.iter()
        .filter(|c| c.token == SemanticToken::Keyword)
        .map(|c| c.literal)
        .collect()
}

#[test]
fn every_registry_keyword_is_in_the_lsp_catalog() {
    let lsp = lsp_sources();
    let mut failures: Vec<String> = Vec::new();

    for kw in registry_keyword_literals() {
        let tok = search_token(kw);
        if !lsp.contains(kw) && !lsp.contains(tok) {
            failures.push(format!(
                "`{kw}` missing from LSP catalog (keywords.rs / hover / canonical_kinds)"
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "LSP-catalog coverage drift — a `lazuli_keywords` keyword is not surfaced by the LSP. \
         Add it to `crate::keywords::KEYWORDS` (and a hover one-liner falls out of the registry \
         via `hover::keyword_description`). H3 owns this surface fully; there is no allowlist.\n  - {}",
        failures.join("\n  - ")
    );
}

#[test]
fn every_registry_keyword_is_highlighted_and_documented() {
    let tm = read("editors/vscode/syntaxes/lazuli.tmLanguage.json");
    let g_lzi = read("docs/grammar.lzi.md");
    let g_lzx = read("docs/grammar.lzx.md");
    let grammar = format!("{g_lzi}\n{g_lzx}");
    let quickref = read("docs/quickref.md");

    let allow: BTreeSet<&str> = DOC_SURFACE_GAP.iter().copied().collect();

    let mut failures: Vec<String> = Vec::new();
    let mut stale_allow: BTreeSet<&str> = allow.clone();

    for kw in registry_keyword_literals() {
        let tok = search_token(kw);
        let in_tm = tm.contains(tok);
        let in_grammar = grammar.contains(tok);
        let in_quickref = quickref.contains(tok);
        let covered = in_tm && in_grammar && in_quickref;

        if covered {
            // Fine — fully documented + highlighted.
            stale_allow.remove(kw);
            continue;
        }

        if allow.contains(kw) {
            // Grandfathered debt; still tracked.
            stale_allow.remove(kw);
            continue;
        }

        // A NEW keyword (post-H3) missing from a doc/highlight surface.
        let mut missing = Vec::new();
        if !in_tm {
            missing.push("tmLanguage");
        }
        if !in_grammar {
            missing.push("grammar.*.md");
        }
        if !in_quickref {
            missing.push("quickref.md");
        }
        failures.push(format!("`{kw}` missing from {}", missing.join(" + ")));
    }

    assert!(
        failures.is_empty(),
        "keyword surface-parity drift — a registry keyword is missing from a highlight/doc \
         surface and is not in the grandfathered DOC_SURFACE_GAP allowlist. Add it to the \
         surface(s) below, or (only if it is genuinely pre-existing debt) to DOC_SURFACE_GAP.\n  - {}",
        failures.join("\n  - ")
    );

    // Hygiene: an allowlist entry that is now fully covered (or no longer
    // a registry keyword) should be deleted so the debt list shrinks
    // truthfully. `stale_allow` holds entries never matched above.
    assert!(
        stale_allow.is_empty(),
        "DOC_SURFACE_GAP has stale entries that are now covered (or are no longer registry \
         keywords) — delete them so the debt list stays honest: {stale_allow:?}"
    );
}

#[test]
fn retired_feature_keywords_are_absent_from_feature_catalog() {
    let feature_catalog = {
        let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src/diagnostics/canonical_kinds/feature.rs");
        fs::read_to_string(&p).unwrap_or_default()
    };
    let mut failures = Vec::new();
    for kw in RETIRED_FEATURE_KEYWORDS {
        // The catalog stores kinds as quoted string literals, e.g. `"workflow"`.
        let quoted = format!("\"{kw}\"");
        if feature_catalog.contains(&quoted) {
            failures.push(format!(
                "retired feature keyword `{kw}` still present in canonical_kinds/feature.rs \
                 FEATURE_BODY_KINDS — it hard-errors in the parser and must not be offered"
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn every_semantic_scalar_is_cataloged_and_documented() {
    // Authoritative semantic-scalar catalog lives in the analyzer.
    let analyzer = {
        let mut acc = String::new();
        for rel in [
            "crates/lazuli_analyzer/src/types.rs",
            "crates/lazuli_analyzer/src/checks/scalar_fixtures/mod.rs",
        ] {
            acc.push_str(&read(rel));
            acc.push('\n');
        }
        acc
    };
    let g_lzi = read("docs/grammar.lzi.md");
    let quickref = read("docs/quickref.md");

    let mut failures: Vec<String> = Vec::new();
    for kw in SEMANTIC_VALUES {
        if !analyzer.contains(kw) {
            failures.push(format!(
                "`@semantic.{kw}` missing from the analyzer catalog (types.rs / scalar_fixtures)"
            ));
        }
        if !g_lzi.contains(kw) {
            failures.push(format!("`@semantic.{kw}` missing from grammar.lzi.md"));
        }
        if !quickref.contains(kw) {
            failures.push(format!("`@semantic.{kw}` missing from quickref.md"));
        }
    }
    assert!(
        failures.is_empty(),
        "semantic-scalar surface drift:\n  - {}",
        failures.join("\n  - ")
    );
}
