//! Auth cross-feature aggregator (sibling of `auth.rs`) —
//! `AUTH-ACTOR-SUBJECT-AMBIGUOUS-001`.
//!
//! Severity: **warn by default, never error** (the unified `ctx.User`
//! runtime slot is an intentional design; the ambiguity is only a smell,
//! not a hard contract violation). The diagnostic hard-codes
//! `DoctorSeverity::Warning` rather than routing through the category
//! default so that the `Production` profile's `Security`→`Error`
//! escalation can never promote it — matching the proposal's "never
//! error by default" requirement.
//!
//! Fires when (the conjunction):
//!  (a) the app's `actor_query` resolves the authenticated actor to a
//!      resource that is **not** named `User`, AND
//!  (b) some command/query in the loaded feature set writes/scopes via
//!      `ctx.user` (`owner = ctx.user`, `ctx.user.id`, `ctx.user.org`)
//!      or carries `@scope.owner` / `@scope.same_org`, AND a `User`
//!      resource is declared somewhere in the package.
//!
//! Both the actor identity (e.g. `Customer`) and the staff identity
//! (`User`) collapse into the single `ctx.User` runtime slot, so an
//! authenticated non-`User` silently satisfies an owner/scope check that
//! was written to gate staff — an authorization-bug-prone setup with no
//! other diagnostic today.
//!
//! Skips cleanly for single-identity apps (`actor_query` resolves to
//! `User`) and for `Customer`-only apps that declare no `User` resource
//! (no second identity to collapse into).
//!
//! Opt-out: `# doctor:allow AUTH-ACTOR-SUBJECT-AMBIGUOUS-001 -- reason
//! "..."` anywhere in the app-manifest source the diagnostic anchors at
//! (matched via [`source_contains_doctor_allow`] against the loaded
//! manifest source, so it works for the LSP's in-memory pass too).
//! `examples/full-capsule` carries this allow on its `actor_query` line
//! because it intentionally demonstrates the dual-identity pattern.

use std::collections::{BTreeMap, BTreeSet};

use lazuli_doctor::allow_comment::source_contains_doctor_allow;
use lazuli_ir::{CommandEffect, Expr, PolicyRef, Predicate, Query, TypeRef};

use crate::doctor::{DoctorAppManifest, DoctorDiagnostic, DoctorSeverity, Tier3FeatureFacts};

/// Rule code surfaced in the diagnostic + matched by the inline-allow
/// comment scan.
pub(crate) const CODE: &str = "AUTH-ACTOR-SUBJECT-AMBIGUOUS-001";

/// Public entrypoint called by the doctor dispatcher (right after the
/// `auth::diagnostics` pass).
///
/// The pass needs three inputs the existing `auth::diagnostics` pass
/// does not: the `Tier3FeatureFacts` slice (carrying the lifted
/// commands / queries / resources / policies per feature), the app
/// manifest (carrying `actor_query`), and `feature_uses` (so a query in
/// `customer_auth` resolving `Customer` declared in `customer` works
/// through the `uses` graph).
pub(crate) fn diagnostics(
    tier3_facts: &[Tier3FeatureFacts],
    app: Option<&DoctorAppManifest>,
    feature_uses: &BTreeMap<String, BTreeSet<String>>,
) -> Vec<DoctorDiagnostic> {
    let Some(app) = app else {
        return Vec::new();
    };
    let Some(actor_query) = app.manifest.actor_query.as_deref() else {
        return Vec::new();
    };

    // (a) Resolve actor_query -> returned resource name. Skip cleanly
    // when it does not resolve (a different rule, ROUTE-GUARD-004, owns
    // "actor_query does not exist") or when it resolves to `User`
    // (single-identity app).
    let Some(subject) = resolve_actor_subject_resource(actor_query, tier3_facts, feature_uses)
    else {
        return Vec::new();
    };
    if subject == "User" {
        return Vec::new();
    }

    // Customer-only app with no `User` resource anywhere: no second
    // identity to collapse into, so the slot is never shared.
    if !package_declares_user_resource(tier3_facts) {
        return Vec::new();
    }

    // (b) Find a ctx.user / @scope.owner / @scope.same_org site in any
    // loaded feature. One hit is enough to confirm the conjunction.
    let Some(site) = first_ctx_user_or_scope_site(tier3_facts) else {
        return Vec::new();
    };

    // Inline opt-out: the diagnostic anchors at the app manifest, so a
    // `# doctor:allow <CODE>` comment in that source silences it. Scans
    // the in-memory manifest source (works for the LSP too) rather than
    // re-reading disk.
    if source_contains_doctor_allow(&app.source, CODE) {
        return Vec::new();
    }

    vec![DoctorDiagnostic {
        path: app.path.clone(),
        line: actor_query_line(&app.source),
        column: 1,
        // Hard-coded Warning — see module header. Never error by default.
        severity: DoctorSeverity::Warning,
        code: CODE.to_owned(),
        message: format!(
            "actor_query `{actor_query}` resolves the authenticated actor to `{subject}` (not `User`), but `{site}` writes/scopes via ctx.user or @scope.owner / @scope.same_org against a User-typed owner. Both identities share the single ctx.User runtime slot, so an authenticated `{subject}` silently satisfies an owner/scope check meant for staff. Split the identities (distinct actor slots) or add `# doctor:allow {CODE} -- reason \"...\"` if the unified slot is intentional.",
        ),
        category: None,
        feature_name: None,
        construct: None,
        fix: None,
        group: None,
    }]
}

/// Resolve `actor_query` (`<feature>.query.<name>`) to the name of the
/// resource the query returns. Mirrors ROUTE-GUARD-004's
/// `parse_query_ref` for the feature/name split, then the lifecycle-
/// gate `query_resource` span heuristic for the lookup/list case (the
/// returned resource is the feature's principal entity). `query.sql`
/// returns name its declared `returns` type directly.
fn resolve_actor_subject_resource(
    actor_query: &str,
    tier3_facts: &[Tier3FeatureFacts],
    feature_uses: &BTreeMap<String, BTreeSet<String>>,
) -> Option<String> {
    let (feature, name) = parse_query_ref(actor_query)?;
    let fact = tier3_facts.iter().find(|f| f.feature == feature)?;
    let query = fact.queries.iter().find(|q| q.name() == name)?;
    match query {
        Query::Sql(q) => type_ref_resource_name(&q.returns),
        // The lookup/list principal resource lives in the query's own
        // feature; when that feature declares none, fall back to a
        // feature it `uses` (mirrors `resolve_resource_for_feature` in
        // the sibling `auth.rs`).
        _ => principal_resource_for_query(query, fact).or_else(|| {
            feature_uses.get(&feature).and_then(|deps| {
                deps.iter()
                    .filter_map(|dep| tier3_facts.iter().find(|f| &f.feature == dep))
                    .find_map(|dep_fact| principal_resource_for_query(query, dep_fact))
            })
        }),
    }
}

/// Split `<feature>.query.<name>` / `<feature>.query.<kind>.<name>` into
/// `(feature, name)`. Verbatim mirror of the route-guard helper of the
/// same name (kept private here to avoid a cross-crate dependency on an
/// analyzer-internal `pub(super)` fn).
fn parse_query_ref(text: &str) -> Option<(String, String)> {
    let head = text.split('(').next().unwrap_or(text).trim();
    let parts: Vec<_> = head.split('.').collect();
    match parts.as_slice() {
        [feature, "query", name] => Some(((*feature).to_owned(), (*name).to_owned())),
        [feature, "query", _kind, name] => Some(((*feature).to_owned(), (*name).to_owned())),
        _ => None,
    }
}

/// Pick the resource a `query.lookup` / `query.list` returns: the
/// feature's principal entity. Prefers a lifecycle-bearing resource
/// declared before the query (the lifecycle-gate `query_resource`
/// heuristic), else the last resource declared before the query, else
/// the single resource when the feature declares exactly one.
fn principal_resource_for_query(query: &Query, fact: &Tier3FeatureFacts) -> Option<String> {
    let q_start = query_span_start(query);
    let before: Vec<&lazuli_ir::Resource> = fact
        .resources
        .iter()
        .filter(|r| match (q_start, r.span_ref) {
            (Some(qs), Some(rs)) => rs.start <= qs,
            _ => true,
        })
        .collect();
    if let Some(with_lifecycle) = before.iter().rev().find(|r| r.lifecycle.is_some()) {
        return Some(with_lifecycle.name.clone());
    }
    before
        .iter()
        .max_by_key(|r| r.span_ref.map(|s| s.start).unwrap_or(0))
        .map(|r| r.name.clone())
        .or_else(|| (fact.resources.len() == 1).then(|| fact.resources[0].name.clone()))
}

fn query_span_start(query: &Query) -> Option<usize> {
    match query {
        Query::List(q) => q.span_ref.map(|s| s.start),
        Query::Lookup(q) => q.span_ref.map(|s| s.start),
        Query::Sql(q) => q.span_ref.map(|s| s.start),
    }
}

/// Base resource name a `TypeRef` points at (unwrapping `Many`). `None`
/// for builtins / capabilities / unresolved-but-empty.
fn type_ref_resource_name(ty: &TypeRef) -> Option<String> {
    match ty {
        TypeRef::UserDefined(q) | TypeRef::EnumRef(q) => Some(q.name.clone()),
        TypeRef::Many(inner) => type_ref_resource_name(inner),
        TypeRef::Unresolved(s) => {
            let base = s.trim().trim_end_matches("[]").trim();
            (!base.is_empty()).then(|| base.to_owned())
        }
        _ => None,
    }
}

/// `true` when any loaded feature declares a `resource User`.
fn package_declares_user_resource(tier3_facts: &[Tier3FeatureFacts]) -> bool {
    tier3_facts
        .iter()
        .any(|f| f.resources.iter().any(|r| r.name == "User"))
}

/// Find the first `ctx.user` / `@scope.owner` / `@scope.same_org` site in
/// any loaded feature, returning a `<feature>.<construct>` label for the
/// diagnostic. Walks command effect assignments (`owner = ctx.user`),
/// query scope predicates (`org = ctx.user.org`), and the policy atom
/// list resolved per `SCOPE-OWNER-COLUMN-001`.
fn first_ctx_user_or_scope_site(tier3_facts: &[Tier3FeatureFacts]) -> Option<String> {
    for feature in tier3_facts {
        let local_policies: BTreeMap<&str, &Vec<String>> = feature
            .policies
            .categories
            .iter()
            .map(|c| (c.name.as_str(), &c.atoms))
            .collect();

        for command in &feature.commands {
            let assignments: &[lazuli_ir::Assignment] = match &command.effect {
                CommandEffect::Creates(c) => &c.assignments,
                CommandEffect::Updates(u) => &u.assignments,
                _ => &[],
            };
            if assignments.iter().any(|a| expr_is_ctx_user(&a.value)) {
                return Some(format!("{}.command.{}", feature.feature, command.name));
            }
            if policy_carries_owner_scope(&command.policy, &local_policies) {
                return Some(format!("{}.command.{}", feature.feature, command.name));
            }
        }

        for query in &feature.queries {
            let (scope, policy) = match query {
                Query::List(q) => (&q.scope, &q.policy),
                Query::Lookup(q) => (&q.scope, &q.policy),
                Query::Sql(q) => (&q.scope, &q.policy),
            };
            if scope.iter().any(predicate_touches_ctx_user) {
                return Some(format!("{}.query.{}", feature.feature, query.name()));
            }
            if policy_carries_owner_scope(policy, &local_policies) {
                return Some(format!("{}.query.{}", feature.feature, query.name()));
            }
        }
    }
    None
}

/// `true` when `expr` is `ctx.user`, `ctx.user.id`, `ctx.user.org`, or
/// any deeper `ctx.user.*` path.
fn expr_is_ctx_user(expr: &Expr) -> bool {
    let Expr::Path(path) = expr else {
        return false;
    };
    matches!(path.segments.as_slice(), [first, second, ..]
        if first == "ctx" && second == "user")
}

/// `true` when either side of a (possibly nested) comparison reads a
/// `ctx.user.*` path.
fn predicate_touches_ctx_user(predicate: &Predicate) -> bool {
    match predicate {
        Predicate::Comparison { left, right, .. } => {
            expr_is_ctx_user(left) || expr_is_ctx_user(right)
        }
        Predicate::Has {
            collection,
            element,
        } => expr_is_ctx_user(collection) || expr_is_ctx_user(element),
        Predicate::And(parts) | Predicate::Or(parts) => {
            parts.iter().any(predicate_touches_ctx_user)
        }
    }
}

/// Resolve a `PolicyRef` to its atom list (mirroring
/// `SCOPE-OWNER-COLUMN-001`'s `Local` -> categories + `Atom` ->
/// `@scope.<name>` resolution) and report whether `@scope.owner` or
/// `@scope.same_org` is present.
fn policy_carries_owner_scope(
    policy: &PolicyRef,
    local_policies: &BTreeMap<&str, &Vec<String>>,
) -> bool {
    let atoms: Vec<String> = match policy {
        PolicyRef::Local(name) => local_policies
            .get(name.as_str())
            .map(|atoms| (*atoms).clone())
            .unwrap_or_default(),
        PolicyRef::Atom(atom) => {
            if let Some(local) = atom.strip_prefix("policy.") {
                local_policies
                    .get(local)
                    .map(|atoms| (*atoms).clone())
                    .unwrap_or_else(|| vec![format!("@{atom}")])
            } else {
                vec![format!("@{atom}")]
            }
        }
        _ => return false,
    };
    atoms
        .iter()
        .any(|a| a == "@scope.owner" || a == "@scope.same_org")
}

/// 1-based source line of the `actor_query "..."` declaration in the app
/// manifest, falling back to line 1. Used to anchor the diagnostic at the
/// authored token rather than the `app` header.
fn actor_query_line(source: &str) -> usize {
    source
        .lines()
        .enumerate()
        .find(|(_, l)| l.trim_start().starts_with("actor_query "))
        .map(|(i, _)| i + 1)
        .unwrap_or(1)
}
