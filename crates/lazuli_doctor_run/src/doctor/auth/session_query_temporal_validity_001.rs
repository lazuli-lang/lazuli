//! session_query_temporal_validity_001 — a `query.list` over the
//! resource bound by `auth sessions resource <X>` must carry a temporal
//! lower bound on the session-expiry field.
//!
//! The session-listing query is the read surface most likely to leak
//! authentication state: without a `expires_at > ctx.now` (or `>=`)
//! filter it returns rows for sessions that have already expired, so a
//! "list my active sessions" screen shows ghosts and a revocation check
//! built on it can hand out access past expiry.
//!
//! This rule is **name-agnostic**: it does NOT gate on the literal query
//! name `active_sessions`. It resolves the session resource from the
//! `auth sessions resource <X>` binding, then attaches each `query.list`
//! to its semantic resource with the same name-overlap scorer codegen
//! uses (`resource_for_query`). Every list query that lands on the
//! session resource must prove temporal validity.
//!
//! The escape hatch in the canonical semantics — hiding the predicate
//! inside a `modifier` that `guarantees expires_at > ctx.now` — is NOT
//! honored by this IR rule: the query `modifier` lowers to an opaque
//! `Option<String>` name with no `guarantees` contract in the IR, and
//! `docs/canonical-semantics.md` (§"Active sessions") is explicit that
//! "the modifier name alone is not enough evidence for codegen or
//! review." A blocking rule therefore requires the explicit filter; a
//! modifier may coexist with it but cannot stand in for it. The warn-
//! only in-editor squiggle (`active-session-temporal-scope`, LSP) keeps
//! its softer text-scan posture.
//!
//! Severity: **error** under strict/production (joins the session-family
//! enforcement codes `auth-session-ttl` / `auth_sessions_resource_unknown`
//! via `security_profile::is_security_enforcement_code`), WARNING under
//! the prototype profile.
//!
//! Reference: docs/canonical-semantics.md §"Active sessions"
//! Reference: crates/lazuli_lsp/src/diagnostics/query/list.rs (the
//! warn-only text-scan twin this promotes to an IR-driven error).

use std::path::{Path, PathBuf};

use lazuli_ir::{CompareOp, Expr, Feature, Filter, Predicate, Query};

// ── output ──────────────────────────────────────────────────────────────────

/// One session-listing `query.list` that lacks a temporal lower bound on
/// its expiry field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// `.lzi` path the offending `query.list` lives in.
    pub path: PathBuf,
    /// Feature owning the query + the `auth sessions` binding.
    pub feature: String,
    /// The `query.list <name>` that targets the session resource.
    pub query: String,
    /// The session resource named by `auth sessions resource <X>`.
    pub session_resource: String,
    /// Byte offset of the `query.list` header (from `ListQuery.span_ref`)
    /// for source anchoring. `None` when the IR carried no span.
    pub offset: Option<usize>,
}

impl Finding {
    /// Stable snake_case doctor rule code (parity with the sibling
    /// `auth_*_001` modules whose `CODE` is snake_case).
    pub const CODE: &'static str = "session_query_temporal_validity_001";

    /// Kebab-case LSP/profile code registered in
    /// `security_profile::is_security_enforcement_code` so the rule is a
    /// WARNING under prototype and an ERROR under strict/production.
    pub const KEBAB_CODE: &'static str = "session-query-temporal-validity";

    /// The session-expiry field this rule requires a lower bound on.
    pub const EXPIRY_FIELD: &'static str = "expires_at";

    /// Render the remediation message naming the query + session
    /// resource and the exact predicate that silences it.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// // let msg = finding.message();
    /// ```
    pub fn message(&self) -> String {
        format!(
            "`query.list {query}` reads session resource `{resource}` but proves no temporal validity; it can return expired sessions. Add an explicit `{field} > ctx.now` (or `>=`) filter. A `modifier` is not sufficient evidence on its own.",
            query = self.query,
            resource = self.session_resource,
            field = Self::EXPIRY_FIELD,
        )
    }
}

// ── detection ─────────────────────────────────────────────────────────────────

/// Run session_query_temporal_validity_001 on a single feature.
///
/// Returns one finding per `query.list` that targets the
/// `auth sessions resource <X>` resource without an `expires_at >
/// ctx.now` / `>=` lower-bound filter. Empty when the feature has no
/// `auth.sessions` binding, no list query lands on the session resource,
/// or every such query carries the temporal filter.
///
/// The session resource is resolved from the binding (name-agnostic);
/// each `query.list` is attached to its resource via the same
/// name-overlap scorer codegen uses, so a query named `live_tokens` over
/// the session resource is checked exactly like one named
/// `active_sessions`.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_ir::Feature;
/// // let findings = check(&feature, Path::new("auth.lzi"));
/// ```
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    let Some(auth) = feature.auth.as_ref() else {
        return Vec::new();
    };
    let Some(sessions) = auth.sessions.as_ref() else {
        return Vec::new();
    };
    let session_resource = sessions.resource.name.as_str();

    // Resolve which resource the session binding points at within this
    // feature. When the binding names a resource the feature does not
    // declare (a separate rule, auth_sessions_resource_unknown_001,
    // fires for that), there is nothing to attach queries to.
    if !feature.resources.iter().any(|r| r.name == session_resource) {
        return Vec::new();
    }

    let mut findings = Vec::new();
    for query in &feature.queries {
        let Query::List(list) = query else {
            continue;
        };
        // Name-agnostic attachment: this list query is in scope iff the
        // codegen scorer attaches it to the session resource.
        if !targets_resource(feature, &list.name, session_resource) {
            continue;
        }
        if has_temporal_lower_bound(&list.filters) {
            continue;
        }
        findings.push(Finding {
            path: path.to_path_buf(),
            feature: feature.name.clone(),
            query: list.name.clone(),
            session_resource: session_resource.to_owned(),
            offset: list.span_ref.as_ref().map(|s| s.start),
        });
    }
    findings
}

/// True when `resource_for_query`'s name-overlap scorer attaches the
/// query to `target`. Mirrors the codegen attachment so the rule's
/// notion of "this query reads the session resource" matches what the
/// emitter actually generates. Single-resource features always resolve
/// to that resource (scorer is a no-op), so the gate is the resolved
/// resource's name.
fn targets_resource(feature: &Feature, query_name: &str, target: &str) -> bool {
    resource_for_query(feature, query_name)
        .map(|name| name == target)
        .unwrap_or(false)
}

/// True when at least one filter predicate is a temporal lower bound on
/// the expiry field: `expires_at > ctx.now` or `expires_at >= ctx.now`.
/// Recurses into `And` (a temporal bound inside a conjunction still
/// proves validity); `Or` is intentionally NOT accepted (an alternative
/// branch can omit the bound, so the guarantee no longer holds on every
/// row).
fn has_temporal_lower_bound(filters: &[Filter]) -> bool {
    filters
        .iter()
        .any(|f| predicate_proves_validity(&f.predicate))
}

fn predicate_proves_validity(predicate: &Predicate) -> bool {
    match predicate {
        Predicate::Comparison { left, op, right } => {
            matches!(op, CompareOp::Gt | CompareOp::Ge) && is_expiry_path(left) && is_ctx_now(right)
        }
        Predicate::And(inner) => inner.iter().any(predicate_proves_validity),
        Predicate::Or(_) | Predicate::Has { .. } => false,
    }
}

/// LHS `expires_at` — the single-segment expiry column path.
fn is_expiry_path(expr: &Expr) -> bool {
    matches!(expr, Expr::Path(path)
        if path.segments.as_slice() == [Finding::EXPIRY_FIELD])
}

/// RHS `ctx.now` — the request-time clock the runtime resolves in
/// `readCtx`.
fn is_ctx_now(expr: &Expr) -> bool {
    matches!(expr, Expr::Path(path)
        if path.segments.len() == 2
            && path.segments[0] == "ctx"
            && path.segments[1] == "now")
}

// ── resource attachment (mirrors lazuli_codegen_go query::util) ───────────────

/// Score-based resolution from a query name to a resource, mirroring
/// `lazuli_codegen_go::emitter::query::util::resource_for_query`. Kept
/// local (doctor does not depend on the Go emitter) so the rule's
/// attachment stays byte-identical to codegen's: single-resource
/// features resolve to that resource; multi-resource features score
/// identifier-token overlap with `plural()` tolerance, so `live_tokens`
/// and `active_sessions` both land on `UserSession`.
fn resource_for_query<'a>(feature: &'a Feature, query_name: &str) -> Option<&'a str> {
    let mut resources: Vec<&str> = feature.resources.iter().map(|r| r.name.as_str()).collect();
    resources.sort_unstable();
    if resources.len() <= 1 {
        return resources.into_iter().next();
    }

    let query_tokens = split_ident_tokens(query_name);
    resources
        .into_iter()
        .map(|name| {
            let tokens = split_ident_tokens(name);
            let last = tokens.last().cloned().unwrap_or_default();
            let mut score = 0usize;
            for token in &tokens {
                if query_tokens
                    .iter()
                    .any(|q| q == token || q == &plural(token))
                {
                    score += 10;
                }
            }
            if !last.is_empty()
                && query_tokens
                    .iter()
                    .any(|q| q == &last || q == &plural(&last))
            {
                score += 50;
            }
            (score, name)
        })
        .max_by(|(score_a, a), (score_b, b)| score_a.cmp(score_b).then_with(|| b.cmp(a)))
        .map(|(_, name)| name)
}

/// Lowercase identifier tokens — `UserSession` -> `["user", "session"]`,
/// `active_sessions` -> `["active", "sessions"]`. Matches
/// `query::util::split_ident_tokens`.
fn split_ident_tokens(s: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut prev_lower_or_digit = false;
    for ch in s.chars() {
        if ch == '_' || ch == '-' || ch == ' ' {
            if !current.is_empty() {
                words.push(current.to_ascii_lowercase());
                current.clear();
            }
            prev_lower_or_digit = false;
            continue;
        }
        if ch.is_ascii_uppercase() && prev_lower_or_digit && !current.is_empty() {
            words.push(current.to_ascii_lowercase());
            current.clear();
        }
        current.push(ch);
        prev_lower_or_digit = ch.is_ascii_lowercase() || ch.is_ascii_digit();
    }
    if !current.is_empty() {
        words.push(current.to_ascii_lowercase());
    }
    words
}

/// Naive lowercase pluralizer (scoring only). Matches
/// `query::util::plural`.
fn plural(word: &str) -> String {
    if let Some(stem) = word.strip_suffix('y') {
        format!("{stem}ies")
    } else if word.ends_with('s') {
        format!("{word}es")
    } else {
        format!("{word}s")
    }
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    include!("session_query_temporal_validity_001_tests.rs");
}
