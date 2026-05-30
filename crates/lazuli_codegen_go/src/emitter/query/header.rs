//! Cell E4 — shared header emission for every query value
//! (`lazuli.Query[A, R]{ Name, Resource, Kind, Policy, ... }`).
//!
//! All three kinds (`list`, `lookup`, `sql`) share the same header
//! shape — the per-kind dispatcher only differs in `kind_const` and
//! the kind-specific body that follows. The policy precedence logic
//! lives here too: per-query authored `policy @policy.<X>` wins;
//! `PolicyRef::None` falls back to `feature.defaults.policy`.
//!
//! Gate annotations (`Prelude: []billing.GateRef{...}`) and scope
//! TODOs ride with the header because they touch the same KV block.

use lazuli_ir::{Feature, Gate, Predicate, Query, Resource};

use super::super::error_resolver::policy_denied_key_for_policy;
use super::super::module::EmitContext;
use super::super::printer::GoPrinter;
use super::sql::sql_query_callable_kind;
use super::util::lower_camel;

/// Emit the shared `Name / Resource / Kind / Policy / ErrorKeys`
/// rows that prefix every query value, then write the
/// `Source: lazuli.Source{...}` field via `emit_with_source_field`.
#[allow(clippy::too_many_arguments)]
pub(super) fn emit_query_header(
    p: &mut GoPrinter,
    feature: &Feature,
    qualified_name: &str,
    resource: Option<&Resource>,
    kind_const: &str,
    emit_ctx: &EmitContext<'_>,
    op: &str,
    span: Option<lazuli_ir::SpanRef>,
    query_policy: &lazuli_ir::PolicyRef,
    query_policy_expr: Option<&lazuli_ir::PolicyExpr>,
    query_policy_when_denied: Option<&lazuli_ir::TranslationKeyRef>,
    error_keys_var: &str,
) {
    let mut kv_rows: Vec<(String, String)> = Vec::new();
    kv_rows.push(("Name:".to_owned(), format!("\"{qualified_name}\",")));
    if let Some(resource) = resource {
        kv_rows.push((
            "Resource:".to_owned(),
            format!("&{}Resource,", lower_camel(&resource.name)),
        ));
    } else {
        kv_rows.push(("Resource:".to_owned(), "nil,".to_owned()));
    }
    kv_rows.push(("Kind:".to_owned(), format!("{kind_const},")));
    // QUERY-POLICY-001 — precedence mirrors `Command`: per-query
    // authored `policy @policy.<X>` (carried on the IR field) wins;
    // `PolicyRef::None` (the default) means "no override authored on
    // this query" and falls back to the feature-level default. Without
    // this branch, every authored `policy @policy.X` line on a query
    // was lost between parse and emit, and the runtime refused to run
    // the query with a `command/query registered with empty policy`
    // panic.
    let policy = if !query_policy.is_none() || query_policy_expr.is_some() {
        super::super::command::format_policy_with_expr_public(
            query_policy,
            query_policy_expr,
            Some(&feature.policies),
        )
    } else {
        match &feature.defaults.policy {
            Some(policy) => super::super::command::format_policy_with_expr_public(
                policy,
                None,
                Some(&feature.policies),
            ),
            None => super::super::command::format_policy_with_expr_public(
                &lazuli_ir::PolicyRef::None,
                None,
                Some(&feature.policies),
            ),
        }
    };
    kv_rows.push(("Policy:".to_owned(), policy));
    if query_policy_denied_key_for_parts(
        feature,
        query_policy,
        query_policy_expr,
        query_policy_when_denied,
    )
    .is_some()
    {
        kv_rows.push(("ErrorKeys:".to_owned(), format!("&{error_keys_var},")));
    }

    let key_width = kv_rows.iter().map(|(k, _)| k.len()).max().unwrap_or(0);
    for (key, value) in &kv_rows {
        let pad = key_width.saturating_sub(key.len());
        p.line(&format!("{}{} {}", key, " ".repeat(pad), value));
    }
    emit_ctx.emit_with_source_field(p, "query", op, span);
}

/// Top-level policy-denied key resolver — folds the `Query::List` /
/// `Query::Lookup` / `Query::Sql` axes onto
/// `query_policy_denied_key_for_parts`.
pub(super) fn query_policy_denied_key<'a>(
    feature: &'a Feature,
    query: &'a Query,
) -> Option<&'a lazuli_ir::TranslationKeyRef> {
    match query {
        Query::List(q) => query_policy_denied_key_for_parts(
            feature,
            &q.policy,
            q.policy_expr.as_ref(),
            q.policy_when_denied.as_ref(),
        ),
        Query::Lookup(q) => query_policy_denied_key_for_parts(
            feature,
            &q.policy,
            q.policy_expr.as_ref(),
            q.policy_when_denied.as_ref(),
        ),
        Query::Sql(q) => query_policy_denied_key_for_parts(
            feature,
            &q.policy,
            q.policy_expr.as_ref(),
            q.policy_when_denied.as_ref(),
        ),
        // query.compose: W5 — policy-denied key resolution is real (same
        // policy fields as the other kinds), so error messages work even
        // before compose codegen emission lands.
        Query::Compose(q) => query_policy_denied_key_for_parts(
            feature,
            &q.policy,
            q.policy_expr.as_ref(),
            q.policy_when_denied.as_ref(),
        ),
    }
}

/// Resolve the policy-denied translation key for a query, given the
/// per-query policy / policy-expr / when-denied triple. Falls back to
/// the feature-level default policy when the query authored neither
/// a literal policy nor an expression.
pub(super) fn query_policy_denied_key_for_parts<'a>(
    feature: &'a Feature,
    query_policy: &'a lazuli_ir::PolicyRef,
    query_policy_expr: Option<&'a lazuli_ir::PolicyExpr>,
    query_policy_when_denied: Option<&'a lazuli_ir::TranslationKeyRef>,
) -> Option<&'a lazuli_ir::TranslationKeyRef> {
    let effective_policy = effective_query_policy(feature, query_policy, query_policy_expr);
    policy_denied_key_for_policy(
        query_policy_when_denied,
        effective_policy,
        Some(&feature.policies),
    )
}

/// "Effective" policy: the query's own when set; otherwise the
/// feature default. Used for denial-key resolution in both
/// `query_policy_denied_key_for_parts` and the runtime side.
fn effective_query_policy<'a>(
    feature: &'a Feature,
    query_policy: &'a lazuli_ir::PolicyRef,
    query_policy_expr: Option<&'a lazuli_ir::PolicyExpr>,
) -> &'a lazuli_ir::PolicyRef {
    if !query_policy.is_none() || query_policy_expr.is_some() {
        return query_policy;
    }
    feature.defaults.policy.as_ref().unwrap_or(query_policy)
}

/// Emit `// TODO(runtime):` placeholders for `scope` / `scope_override`
/// — Lazuli Go does not yet honor these on `Query`, so codegen emits a
/// visible breadcrumb instead of silently dropping the predicate.
pub(super) fn emit_scope_gaps(p: &mut GoPrinter, scope: &[Predicate], scope_override: bool) {
    if !scope.is_empty() {
        p.line("// TODO(runtime): Query.scope is not yet in Lazuli Go lib.");
    }
    if scope_override {
        p.line("// TODO(runtime): Query.scope_override is not yet in Lazuli Go lib.");
    }
}

/// PG.C.2 — emit the `Prelude: []billing.GateRef{...}` field on a
/// `lazuli.Query[A, R]` value. The runtime dispatcher (`RunList` /
/// `RunLookup`) consults the slice via `lazuli.RunPrelude` before
/// invoking the handler and via `lazuli.RunIncrement` after a
/// successful return. Empty slice is the no-gate fast path — we
/// skip emission entirely so back-compat call sites stay
/// byte-equivalent.
pub(super) fn emit_gate_annotations(p: &mut GoPrinter, gates: &[Gate]) {
    if gates.is_empty() {
        return;
    }
    p.line("Prelude: []billing.GateRef{");
    p.indent();
    for gate in gates {
        match gate {
            Gate::Behind { feature } => {
                p.line(&format!(
                    "{{Kind: billing.GateBehind, Name: {:?}}},",
                    feature
                ));
            }
            Gate::Quota { limit } => {
                p.line(&format!("{{Kind: billing.GateQuota, Name: {:?}}},", limit));
            }
        }
    }
    p.dedent();
    p.line("},");
}

/// PG.C.2 — translate a `Query` IR variant back to the canonical
/// `<callable_kind>` string used as a key in the emit-time gate
/// map (`<feature>/query.list:<name>`, etc.).
pub(super) fn query_callable_kind(query: &Query) -> &'static str {
    match query {
        Query::List(_) => "query.list",
        Query::Lookup(_) => "query.lookup",
        Query::Sql(q) => sql_query_callable_kind(q),
        // query.compose: W5 — callable-kind key for the emit-time gate map.
        Query::Compose(_) => "query.compose",
    }
}
