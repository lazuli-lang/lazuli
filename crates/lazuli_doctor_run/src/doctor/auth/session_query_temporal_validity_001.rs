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
    use lazuli_ir::{
        Auth, AuthIdentity, AuthSessions, BuiltinType, Defaults, EnumLiteral, Feature, Field,
        FieldConstraints, FieldRef, ListQuery, Path as IrPath, Policies, PolicyRef, Predicate,
        QualifiedName, Query, Resource, SpanRef, TypeRef,
    };

    use super::*;

    fn qn(name: &str) -> QualifiedName {
        QualifiedName {
            feature: None,
            name: name.to_owned(),
        }
    }

    fn mk_field(name: &str, type_ref: TypeRef) -> Field {
        Field {
            name: name.to_owned(),
            type_ref,
            required: true,
            unique: false,
            slug: false,
            default: None,
            derived_from: None,
            computed_date: None,
            constraints: FieldConstraints::default(),
            full_text: false,
            previous_names: vec![],
            pii: None,
            owner_axis: None,
            cross_feature_target: None,
            span_ref: None,
        }
    }

    fn mk_resource(name: &str) -> Resource {
        Resource {
            name: name.to_owned(),
            public_contract: None,
            tenancy: None,
            soft_delete: false,
            timestamps: None,
            fields: vec![
                mk_field("id", TypeRef::Builtin(BuiltinType::Id)),
                mk_field("expires_at", TypeRef::Builtin(BuiltinType::DateTime)),
            ],
            constraints: vec![],
            validate: None,
            validates: vec![],
            retention: None,
            previous_names: vec![],
            span_ref: None,
            lifecycle: None,
            lifecycle_routes: None,
            polymorphic_refs: Vec::new(),
            many_through: Vec::new(),
            append_only: false,
            invariants: vec![],
            lock: None,
            composite_key: None,
            conventions: Vec::new(),
        }
    }

    /// `expires_at <op> ctx.now`.
    fn temporal_filter(op: CompareOp) -> Filter {
        Filter {
            predicate: Predicate::Comparison {
                left: Expr::Path(IrPath::from_segments(["expires_at"])),
                op,
                right: Expr::Path(IrPath::from_segments(["ctx", "now"])),
            },
            when: None,
        }
    }

    /// A non-temporal equality filter (`customer.id = params.customer_id`).
    fn owner_filter() -> Filter {
        Filter {
            predicate: Predicate::Comparison {
                left: Expr::Path(IrPath::from_segments(["customer", "id"])),
                op: CompareOp::Eq,
                right: Expr::Path(IrPath::from_segments(["params", "customer_id"])),
            },
            when: None,
        }
    }

    fn list_query(name: &str, modifier: Option<&str>, filters: Vec<Filter>) -> Query {
        Query::List(ListQuery {
            name: name.to_owned(),
            public_contract: None,
            params: vec![],
            scope: vec![],
            scope_override: false,
            filters,
            order: vec![],
            paginate: None,
            modifier: modifier.map(str::to_owned),
            cache: None,
            policy: PolicyRef::None,
            policy_expr: None,
            policy_when_denied: None,
            previous_names: vec![],
            span_ref: Some(SpanRef { start: 42, end: 99 }),
            owner_scope_sql: None,
        })
    }

    fn feature_with(
        session_resource: &str,
        resources: Vec<Resource>,
        queries: Vec<Query>,
    ) -> Feature {
        Feature {
            name: "customer_auth".to_owned(),
            purpose: None,
            non_goals: vec![],
            context_path: None,
            knowledge: None,
            defaults: Defaults::default(),
            uses: vec![],
            uses_spans: Vec::new(),
            uses_versions: Vec::new(),
            requirements: vec![],
            enums: vec![],
            resources,
            events: vec![],
            rules: vec![],
            policies: Policies::default(),
            errors: None,
            commands: vec![],
            apis: vec![],
            records: vec![],
            queries,
            resume_routers: vec![],
            workflows: vec![],
            jobs: vec![],
            webhooks: vec![],
            notifications: vec![],
            event_groups: vec![],
            tenant_migrations: vec![],
            translation: None,
            pollers: vec![],
            channels: vec![],
            caches: vec![],
            aggregates: vec![],
            mcp_servers: vec![],
            auth: Some(Auth {
                identity: AuthIdentity {
                    field: FieldRef {
                        resource: qn(session_resource),
                        field: "email".to_owned(),
                    },
                    public_contract: None,
                },
                password: None,
                sessions: Some(AuthSessions {
                    resource: qn(session_resource),
                    ttl: "7 days".to_owned(),
                    refresh: false,
                    extra_columns: vec![],
                    access_ttl: None,
                    rotation: None,
                    cookie: None,
                }),
                mfa: None,
                oauth: vec![],
                span_ref: None,
            }),
            surfaces: vec![],
            extensions: vec![],
            escape_routes: vec![],
            agents: vec![],
            reports: vec![],
            previous_names: vec![],
            synth_origins: std::collections::BTreeMap::new(),
            span_ref: None,
        }
    }

    #[test]
    fn positive_fires_when_session_query_lacks_temporal_bound() {
        // Ported from the LSP negative case: a session-listing query
        // under a NON-`active_sessions` name with no temporal filter.
        let feature = feature_with(
            "UserSession",
            vec![mk_resource("UserSession")],
            vec![list_query(
                "live_tokens",
                Some("@query_modifier.active_session_scope"),
                vec![owner_filter()],
            )],
        );
        let findings = check(&feature, Path::new("auth.lzi"));
        assert_eq!(findings.len(), 1, "got: {findings:?}");
        assert_eq!(Finding::CODE, "session_query_temporal_validity_001");
        assert_eq!(Finding::KEBAB_CODE, "session-query-temporal-validity");
        assert_eq!(findings[0].query, "live_tokens");
        assert_eq!(findings[0].session_resource, "UserSession");
        assert_eq!(findings[0].offset, Some(42));
        assert!(findings[0].message().contains("live_tokens"));
        assert!(findings[0].message().contains("UserSession"));
        assert!(findings[0].message().contains("expires_at > ctx.now"));
    }

    #[test]
    fn positive_fires_when_only_modifier_present() {
        // A modifier alone is NOT sufficient evidence (canonical
        // semantics §"Active sessions"); the IR carries no `guarantees`
        // contract on the opaque modifier name.
        let feature = feature_with(
            "UserSession",
            vec![mk_resource("UserSession")],
            vec![list_query(
                "active_sessions",
                Some("@query_modifier.active_session_scope"),
                vec![],
            )],
        );
        let findings = check(&feature, Path::new("auth.lzi"));
        assert_eq!(
            findings.len(),
            1,
            "modifier alone must still fire: {findings:?}"
        );
    }

    #[test]
    fn negative_clean_with_explicit_gt_filter() {
        // Mirrors examples/production-grade/features/auth/auth.lzi:74 —
        // explicit `expires_at > ctx.now`, no modifier.
        let feature = feature_with(
            "UserSession",
            vec![mk_resource("UserSession")],
            vec![list_query(
                "active_sessions",
                None,
                vec![owner_filter(), temporal_filter(CompareOp::Gt)],
            )],
        );
        assert!(check(&feature, Path::new("auth.lzi")).is_empty());
    }

    #[test]
    fn negative_clean_with_ge_filter() {
        let feature = feature_with(
            "UserSession",
            vec![mk_resource("UserSession")],
            vec![list_query(
                "active_sessions",
                None,
                vec![temporal_filter(CompareOp::Ge)],
            )],
        );
        assert!(check(&feature, Path::new("auth.lzi")).is_empty());
    }

    #[test]
    fn negative_clean_with_modifier_and_filter() {
        // Mirrors examples/user-auth.lzi:58 + full-capsule.lzi:585 —
        // modifier present AND explicit filter present.
        let feature = feature_with(
            "Session",
            vec![mk_resource("Session")],
            vec![list_query(
                "active_sessions",
                Some("@query_modifier.active_session_scope"),
                vec![owner_filter(), temporal_filter(CompareOp::Gt)],
            )],
        );
        assert!(check(&feature, Path::new("auth.lzi")).is_empty());
    }

    #[test]
    fn negative_temporal_bound_inside_and_conjunction() {
        let and = Filter {
            predicate: Predicate::And(vec![
                owner_filter().predicate,
                temporal_filter(CompareOp::Gt).predicate,
            ]),
            when: None,
        };
        let feature = feature_with(
            "UserSession",
            vec![mk_resource("UserSession")],
            vec![list_query("active_sessions", None, vec![and])],
        );
        assert!(check(&feature, Path::new("auth.lzi")).is_empty());
    }

    #[test]
    fn positive_or_branch_does_not_prove_validity() {
        // An `Or` can take the branch that omits the bound, so it does
        // not guarantee every row is unexpired.
        let or = Filter {
            predicate: Predicate::Or(vec![
                owner_filter().predicate,
                temporal_filter(CompareOp::Gt).predicate,
            ]),
            when: None,
        };
        let feature = feature_with(
            "UserSession",
            vec![mk_resource("UserSession")],
            vec![list_query("active_sessions", None, vec![or])],
        );
        assert_eq!(check(&feature, Path::new("auth.lzi")).len(), 1);
    }

    #[test]
    fn negative_no_sessions_block_does_not_fire() {
        let mut feature = feature_with(
            "UserSession",
            vec![mk_resource("UserSession")],
            vec![list_query("active_sessions", None, vec![])],
        );
        // Strip the sessions binding — no session axis to enforce.
        if let Some(auth) = feature.auth.as_mut() {
            auth.sessions = None;
        }
        assert!(check(&feature, Path::new("auth.lzi")).is_empty());
    }

    #[test]
    fn negative_no_auth_block_does_not_fire() {
        let mut feature = feature_with(
            "UserSession",
            vec![mk_resource("UserSession")],
            vec![list_query("active_sessions", None, vec![])],
        );
        feature.auth = None;
        assert!(check(&feature, Path::new("auth.lzi")).is_empty());
    }

    #[test]
    fn negative_session_resource_not_declared_locally_does_not_fire() {
        // auth_sessions_resource_unknown_001 owns the "binding names a
        // missing resource" case; this rule stays silent so the two do
        // not double-fire.
        let feature = feature_with(
            "MissingSession",
            vec![mk_resource("UserSession")],
            vec![list_query("active_sessions", None, vec![])],
        );
        assert!(check(&feature, Path::new("auth.lzi")).is_empty());
    }

    #[test]
    fn negative_non_session_query_is_out_of_scope() {
        // A list query that scores onto a NON-session resource is not
        // checked. Multi-resource feature so the scorer is active.
        let feature = feature_with(
            "UserSession",
            vec![mk_resource("UserSession"), mk_resource("AuditLog")],
            vec![list_query("audit_logs", None, vec![])],
        );
        assert!(
            check(&feature, Path::new("auth.lzi")).is_empty(),
            "audit_logs targets AuditLog, not the session resource"
        );
    }

    #[test]
    fn positive_name_agnostic_multi_resource_scores_session() {
        // Multi-resource feature; a non-`active_sessions` query name that
        // scores onto the session resource still fires.
        let feature = feature_with(
            "UserSession",
            vec![mk_resource("UserSession"), mk_resource("AuditLog")],
            vec![list_query("user_sessions", None, vec![owner_filter()])],
        );
        let findings = check(&feature, Path::new("auth.lzi"));
        assert_eq!(findings.len(), 1, "got: {findings:?}");
        assert_eq!(findings[0].query, "user_sessions");
    }

    #[test]
    fn edge_enum_rhs_is_not_ctx_now() {
        // Guard against a false negative: `expires_at > Status.active`
        // (nonsense, but exercises the RHS gate) must not count.
        let weird = Filter {
            predicate: Predicate::Comparison {
                left: Expr::Path(IrPath::from_segments(["expires_at"])),
                op: CompareOp::Gt,
                right: Expr::Enum(EnumLiteral {
                    type_name: None,
                    variant: "now".to_owned(),
                }),
            },
            when: None,
        };
        let feature = feature_with(
            "UserSession",
            vec![mk_resource("UserSession")],
            vec![list_query("active_sessions", None, vec![weird])],
        );
        assert_eq!(check(&feature, Path::new("auth.lzi")).len(), 1);
    }
}
