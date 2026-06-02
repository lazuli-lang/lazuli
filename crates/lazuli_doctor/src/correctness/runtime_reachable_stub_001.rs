//! RUNTIME-REACHABLE-STUB-001 — a DSL construct lowers to a runtime path
//! that is a known not-implemented stub (returns `501` / "not yet
//! implemented" at request time).
//!
//! **Severity:** error. This is the "compiles but dead on arrival"
//! class: the feature parses, lowers, and `go build`s cleanly, but the
//! emitted code routes to a runtime arm that *always* returns a 501 /
//! `not yet implemented` error. The audit (W2-2) cited two live ones —
//! a `target.<field>` binding/filter (codegen emits `lazuli.FromTarget`,
//! whose runtime arm `sourceTarget` returns 501 in
//! `runtime/go/lazuli/handle.go:resolveSource`) and a resource
//! `retention <dur> then archive` policy (`RetentionAction::Archive`,
//! whose runtime arm returns `ErrRetentionArchiveNotImplemented` in
//! `runtime/go/lazuli/retention.go:applyRetentionAction`). This rule
//! surfaces "you used feature X but the runtime doesn't implement it
//! yet" at `lazuli check` / `doctor` time, not at the first request.
//!
//! **Fires when** a construct in the [`STUB_TABLE`] is present:
//!   - a `creates` / `updates` / `deletes` binding RHS, or an authored
//!     `where <col> = target.<field>` row, whose path head is `target`
//!     (→ `FromTarget` → `sourceTarget` 501);
//!   - a `query.list` / `query.lookup` filter or lookup-key value side
//!     whose path head is `target` (same lowering, same 501);
//!   - a `resource` carrying `retention <dur> then archive`
//!     (→ `RetentionAction::Archive` → `ErrRetentionArchiveNotImplemented`).
//!
//! **Does not fire** for the implemented sibling kinds — `input.*`,
//! `ctx.*`, `route.*` / `params.*` paths, `@fn.*()` calls, literals,
//! or `retention ... then delete` / `then anonymize` (both have live
//! runtime arms). Those are the same dispatch the emitter and runtime
//! agree on; only the documented stub arms below trip this rule.
//!
//! ## The stub table — maintenance contract
//!
//! [`STUB_TABLE`] is the explicit, reviewed map from a DSL construct to
//! the runtime symbol that stubs it. Each entry names the runtime file +
//! symbol so a reviewer can verify the stub still exists. When a runtime
//! arm gets implemented, delete its row here AND update the runtime
//! face-parity guard (`runtime/go/lazuli/readctx_parity_test.go`
//! `knownStubKinds` for `FromTarget`; `runtime/go/lazuli/retention_test.go`
//! for archive) so the two faces tighten together. A new runtime 501
//! stub reachable from a DSL construct should gain a row here.

use std::path::{Path, PathBuf};

use lazuli_ir::{
    Assignment, CommandEffect, Expr, Feature, Filter, KeyClause, Predicate, Query, RetentionAction,
};

// stub table

/// One row of the construct → known-runtime-stub map. `construct` is the
/// author-facing name of the DSL shape; `runtime_symbol` is the Go symbol
/// (with file) that stubs it, so a reviewer can confirm the stub is real.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StubEntry {
    /// Author-facing construct name (e.g. `"target.<field>` binding").
    pub construct: &'static str,
    /// Runtime symbol + file that returns the 501 / not-implemented stub.
    pub runtime_symbol: &'static str,
}

/// `target.<field>` source — codegen `FromTarget`, runtime `sourceTarget`
/// arm returns 501 "not yet implemented in runtime spike".
pub const STUB_TARGET_SOURCE: StubEntry = StubEntry {
    construct: "`target.<field>` source (creates/updates/deletes binding or query filter)",
    runtime_symbol: "runtime/go/lazuli/handle.go: resolveSource sourceTarget arm \
                     (codegen lazuli.FromTarget) → 501 \"target binding not yet implemented\"",
};

/// `retention <dur> then archive` — runtime `applyRetentionAction`
/// Archive arm returns `ErrRetentionArchiveNotImplemented`.
pub const STUB_RETENTION_ARCHIVE: StubEntry = StubEntry {
    construct: "resource `retention <dur> then archive` policy",
    runtime_symbol: "runtime/go/lazuli/retention.go: applyRetentionAction RetentionArchive arm \
                     → ErrRetentionArchiveNotImplemented \"retention archive action not yet \
                     implemented\"",
};

/// The reviewed, explicit construct → known-stub table (v1). Each entry
/// is documented above; see the module header for the maintenance
/// contract that keeps this in lockstep with the runtime stub arms.
pub const STUB_TABLE: &[StubEntry] = &[STUB_TARGET_SOURCE, STUB_RETENTION_ARCHIVE];

// output

/// One RUNTIME-REACHABLE-STUB-001 finding: a construct that lowers to a
/// known not-implemented runtime arm.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub path: PathBuf,
    /// The owning construct's name (command / query / resource name).
    pub owner: String,
    /// The matched stub-table entry.
    pub entry: StubEntry,
    /// Where the construct lives, for the message (`command` / `query` /
    /// `resource`).
    pub kind: &'static str,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "RUNTIME-REACHABLE-STUB-001";

    /// Render the "reachable runtime stub" message — name the construct,
    /// the DSL shape, and the runtime symbol so the author knows the
    /// feature is declared-but-unimplemented before they ship it.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::correctness::runtime_reachable_stub_001::{Finding, STUB_TARGET_SOURCE};
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("f.lzi"),
    ///     owner: "set_prev".into(),
    ///     entry: STUB_TARGET_SOURCE,
    ///     kind: "command",
    /// };
    /// assert!(f.message().contains("not yet implemented"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "{} `{}` uses {} which lowers to a runtime path that is NOT YET IMPLEMENTED: \
             it compiles and `go build`s, but returns 501 / \"not yet implemented\" at the \
             first request (runtime stub: {}). Remove the construct or implement the runtime \
             arm before shipping.",
            self.kind, self.owner, self.entry.construct, self.entry.runtime_symbol,
        )
    }
}

// detection

/// Run RUNTIME-REACHABLE-STUB-001 for one feature.
///
/// `path` anchors findings; no I/O is performed here. Walks command
/// bindings + query filters for a `target.<field>` source, and resources
/// for `retention ... then archive`.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::correctness::runtime_reachable_stub_001::check;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower a feature");
/// let _ = check(&feature, Path::new("billing.lzi"));
/// ```
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    let mut out = Vec::new();

    // (1) command bindings — SET + authored WHERE rows.
    for command in &feature.commands {
        let mut hit = false;
        match &command.effect {
            CommandEffect::Creates(create) => {
                hit |= any_target_source(&create.assignments);
            }
            CommandEffect::Updates(update) => {
                hit |= any_target_source(&update.assignments);
                hit |= any_target_source(&update.where_clause);
            }
            CommandEffect::Deletes(delete) => {
                hit |= any_target_source(&delete.where_clause);
            }
            CommandEffect::Reorders(_) | CommandEffect::Returns(_) | CommandEffect::None => {}
        }
        if hit {
            out.push(Finding {
                path: path.to_path_buf(),
                owner: command.name.clone(),
                entry: STUB_TARGET_SOURCE,
                kind: "command",
            });
        }
    }

    // (2) query filters + lookup keys — the value side resolves to a
    //     `FromTarget` source through the same emitter dispatch.
    for query in &feature.queries {
        let hit = match query {
            Query::List(q) => filters_hit_target(&q.filters),
            Query::Lookup(q) => filters_hit_target(&q.filters) || keys_hit_target(&q.keys),
            // query.sql RHS is hand-rolled SQL, not a binding-source
            // expression — it never lowers to FromTarget.
            Query::Sql(_) => false,
        };
        if hit {
            out.push(Finding {
                path: path.to_path_buf(),
                owner: query.name().to_owned(),
                entry: STUB_TARGET_SOURCE,
                kind: "query",
            });
        }
    }

    // (3) resource retention archive.
    for resource in &feature.resources {
        if let Some(spec) = &resource.retention
            && spec.action == RetentionAction::Archive
        {
            out.push(Finding {
                path: path.to_path_buf(),
                owner: resource.name.clone(),
                entry: STUB_RETENTION_ARCHIVE,
                kind: "resource",
            });
        }
    }

    out
}

// internals

/// `true` when any assignment RHS is a `target.<field>` path. Mirrors the
/// emitter's `format_path_source` `"target" =>` arm
/// (`crates/lazuli_codegen_go/src/emitter/command/effects_format.rs`).
fn any_target_source(assignments: &[Assignment]) -> bool {
    assignments.iter().any(|a| is_target_path(&a.value))
}

/// `true` when any filter's value side (the non-column side of an
/// equality comparison) is a `target.<field>` path. Conservatively scans
/// BOTH sides of every comparison so the rule is robust to which side the
/// emitter treats as the column.
fn filters_hit_target(filters: &[Filter]) -> bool {
    filters.iter().any(|f| predicate_hits_target(&f.predicate))
}

fn keys_hit_target(keys: &[KeyClause]) -> bool {
    keys.iter().any(|k| is_target_path(&k.equals))
}

fn predicate_hits_target(predicate: &Predicate) -> bool {
    match predicate {
        Predicate::Comparison { left, right, .. } => {
            is_target_path(left) || is_target_path(right)
        }
        Predicate::Has {
            collection,
            element,
        } => is_target_path(collection) || is_target_path(element),
        Predicate::And(ps) | Predicate::Or(ps) => ps.iter().any(predicate_hits_target),
    }
}

/// `true` when `expr` is a `Path` whose head segment is `target` — the
/// exact shape both `effects_format.rs` and `query/filters.rs` lower to
/// `lazuli.FromTarget(...)`.
fn is_target_path(expr: &Expr) -> bool {
    matches!(
        expr,
        Expr::Path(p) if p.segments.first().map(|s| s.as_str()) == Some("target")
    )
}

// tests

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        BuiltinType, Command, CommandEffect, CommandInput, CommandKind, CompareOp, CreateEffect,
        Defaults, DeleteEffect, Field, FieldConstraints, FnCallExpr, ListQuery, LookupQuery,
        Path as IrPath, Policies, PolicyRef, QualifiedName, Resource, RetentionSpec, TypeRef,
        UpdateEffect,
    };

    fn qn(name: &str) -> QualifiedName {
        QualifiedName {
            feature: None,
            name: name.to_owned(),
        }
    }

    fn path_expr(segments: &[&str]) -> Expr {
        Expr::Path(IrPath::from_segments(segments.iter().copied()))
    }

    fn assign(field: &str, value: Expr) -> Assignment {
        Assignment {
            field: field.to_owned(),
            value,
        }
    }

    fn eq_filter(left: Expr, right: Expr) -> Filter {
        Filter {
            predicate: Predicate::Comparison {
                left,
                op: CompareOp::Eq,
                right,
            },
            when: None,
        }
    }

    fn mk_cmd(name: &str, effect: CommandEffect) -> Command {
        let kind = match &effect {
            CommandEffect::Creates(_) => CommandKind::Create,
            CommandEffect::Updates(_) => CommandKind::Update,
            CommandEffect::Deletes(_) => CommandKind::Delete,
            _ => CommandKind::Returns,
        };
        Command {
            name: name.to_owned(),
            public_contract: None,
            kind,
            route: vec![],
            input: CommandInput::Empty,
            target: None,
            lets: vec![],
            effect,
            policy: PolicyRef::None,
            policy_expr: None,
            policy_when_denied: None,
            emits: vec![],
            rate_limit: None,
            audit: None,
            approval: None,
            invalidates: vec![],
            external_calls: vec![],
            timeout: None,
            retry: None,
            idempotency: None,
            write_window: None,
            deprecated: None,
            handler: None,
            tests: None,
            triggers: vec![],
            synthesized_from_cap_file: None,
            previous_names: vec![],
            span_ref: None,
            owner_scope_sql: None,
            derived_from: None,
        }
    }

    fn empty_feature() -> Feature {
        Feature {
            name: "billing".into(),
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
            resources: vec![],
            events: vec![],
            rules: vec![],
            policies: Policies::default(),
            errors: None,
            commands: vec![],
            apis: vec![],
            records: vec![],
            queries: vec![],
            resume_routers: vec![],
            workflows: vec![],
            jobs: vec![],
            webhooks: vec![],
            notifications: vec![],
            event_groups: vec![],
            tenant_migrations: vec![],
            translation: None,
            pollers: vec![],
            auth: None,
            surfaces: vec![],
            extensions: vec![],
            escape_routes: vec![],
            agents: vec![],
            reports: vec![],
            channels: vec![],
            caches: vec![],
            aggregates: vec![],
            mcp_servers: vec![],
            previous_names: vec![],
            synth_origins: std::collections::BTreeMap::new(),
            span_ref: None,
        }
    }

    fn creates(assignments: Vec<Assignment>) -> CommandEffect {
        CommandEffect::Creates(CreateEffect {
            resource: qn("Account"),
            from_input: false,
            assignments,
        })
    }

    fn updates(assignments: Vec<Assignment>, where_clause: Vec<Assignment>) -> CommandEffect {
        CommandEffect::Updates(UpdateEffect {
            resource: qn("Account"),
            assignments,
            where_clause,
        })
    }

    fn deletes(where_clause: Vec<Assignment>) -> CommandEffect {
        CommandEffect::Deletes(DeleteEffect {
            resource: qn("Account"),
            where_clause,
        })
    }

    fn mk_field(name: &str) -> Field {
        Field {
            name: name.to_owned(),
            type_ref: TypeRef::Builtin(BuiltinType::Integer),
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

    fn mk_resource(name: &str, retention: Option<RetentionSpec>) -> Resource {
        Resource {
            name: name.to_owned(),
            public_contract: None,
            tenancy: None,
            soft_delete: false,
            soft_delete_actor: false,
            timestamps: None,
            fields: vec![mk_field("id")],
            constraints: vec![],
            validate: None,
            validates: vec![],
            retention,
            previous_names: vec![],
            span_ref: None,
            lifecycle: None,
            invariants: vec![],
            lock: None,
            composite_key: None,
            conventions: Vec::new(),
            lifecycle_routes: None,
            polymorphic_refs: Vec::new(),
            many_through: Vec::new(),
            restrict_on_delete: Vec::new(),
            append_only: false,
        }
    }

    // (a) `target.X` in a command binding → fires (code, owner, runtime symbol).

    #[test]
    fn positive_target_in_create_binding_fires() {
        let cmd = mk_cmd(
            "snapshot",
            creates(vec![assign("prev_status", path_expr(&["target", "status"]))]),
        );
        let mut feature = empty_feature();
        feature.commands = vec![cmd];
        let findings = check(&feature, Path::new("billing.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(Finding::CODE, "RUNTIME-REACHABLE-STUB-001");
        assert_eq!(findings[0].owner, "snapshot");
        assert_eq!(findings[0].kind, "command");
        assert_eq!(findings[0].entry, STUB_TARGET_SOURCE);
        assert!(findings[0].message().contains("not yet implemented"));
        assert!(findings[0].message().contains("sourceTarget"));
    }

    #[test]
    fn positive_target_in_update_where_fires() {
        let cmd = mk_cmd(
            "touch",
            updates(
                vec![assign("status", Expr::String("done".into()))],
                vec![assign("id", path_expr(&["target", "id"]))],
            ),
        );
        let mut feature = empty_feature();
        feature.commands = vec![cmd];
        let findings = check(&feature, Path::new("billing.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].owner, "touch");
    }

    #[test]
    fn positive_target_in_delete_where_fires() {
        let cmd = mk_cmd("purge", deletes(vec![assign("id", path_expr(&["target", "id"]))]));
        let mut feature = empty_feature();
        feature.commands = vec![cmd];
        assert_eq!(check(&feature, Path::new("billing.lzi")).len(), 1);
    }

    // (b) `target.X` as a query filter / lookup-key value → fires.

    #[test]
    fn positive_target_in_list_filter_fires() {
        let mut feature = empty_feature();
        feature.queries = vec![Query::List(ListQuery {
            name: "by_prev".into(),
            public_contract: None,
            params: vec![],
            scope: vec![],
            scope_override: false,
            filters: vec![eq_filter(path_expr(&["status"]), path_expr(&["target", "status"]))],
            order: vec![],
            paginate: None,
            modifier: None,
            cache: None,
            policy: PolicyRef::None,
            policy_expr: None,
            policy_when_denied: None,
            previous_names: vec![],
            span_ref: None,
            owner_scope_sql: None,
        })];
        let findings = check(&feature, Path::new("billing.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].kind, "query");
        assert_eq!(findings[0].owner, "by_prev");
    }

    #[test]
    fn positive_target_in_lookup_key_fires() {
        let mut feature = empty_feature();
        feature.queries = vec![Query::Lookup(LookupQuery {
            name: "find".into(),
            public_contract: None,
            params: vec![],
            keys: vec![KeyClause {
                path: IrPath::from_segments(["id"]),
                equals: path_expr(&["target", "id"]),
            }],
            scope: vec![],
            scope_override: false,
            filters: vec![],
            policy: PolicyRef::None,
            policy_expr: None,
            policy_when_denied: None,
            previous_names: vec![],
            span_ref: None,
            owner_scope_sql: None,
        })];
        assert_eq!(check(&feature, Path::new("billing.lzi")).len(), 1);
    }

    // (c) `retention ... then archive` → fires with the archive runtime symbol.

    #[test]
    fn positive_retention_archive_fires() {
        let mut feature = empty_feature();
        feature.resources = vec![mk_resource(
            "Invoice",
            Some(RetentionSpec {
                duration: "90d".into(),
                action: RetentionAction::Archive,
            }),
        )];
        let findings = check(&feature, Path::new("billing.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].kind, "resource");
        assert_eq!(findings[0].owner, "Invoice");
        assert_eq!(findings[0].entry, STUB_RETENTION_ARCHIVE);
        assert!(findings[0].message().contains("retention archive"));
    }

    // (d) implemented sibling kinds → no false positive.

    #[test]
    fn negative_implemented_sources_do_not_fire() {
        let cmd = mk_cmd(
            "ok",
            creates(vec![
                assign("name", path_expr(&["input", "name"])),
                assign("owner_id", path_expr(&["ctx", "actor", "id"])),
                assign("id", path_expr(&["route", "id"])),
                assign("role", Expr::String("MEMBER".into())),
                assign(
                    "hash",
                    Expr::FnCall(FnCallExpr {
                        name: qn("hash"),
                        args: vec![path_expr(&["input", "pw"])],
                    }),
                ),
            ]),
        );
        let mut feature = empty_feature();
        feature.commands = vec![cmd];
        assert!(check(&feature, Path::new("billing.lzi")).is_empty());
    }

    #[test]
    fn negative_retention_delete_and_anonymize_do_not_fire() {
        let mut feature = empty_feature();
        feature.resources = vec![
            mk_resource(
                "A",
                Some(RetentionSpec {
                    duration: "30d".into(),
                    action: RetentionAction::Delete,
                }),
            ),
            mk_resource(
                "B",
                Some(RetentionSpec {
                    duration: "30d".into(),
                    action: RetentionAction::Anonymize,
                }),
            ),
            mk_resource("C", None),
        ];
        assert!(check(&feature, Path::new("billing.lzi")).is_empty());
    }

    #[test]
    fn negative_non_target_query_filter_does_not_fire() {
        let mut feature = empty_feature();
        feature.queries = vec![Query::List(ListQuery {
            name: "clean".into(),
            public_contract: None,
            params: vec![],
            scope: vec![],
            scope_override: false,
            filters: vec![eq_filter(path_expr(&["status"]), path_expr(&["params", "status"]))],
            order: vec![],
            paginate: None,
            modifier: None,
            cache: None,
            policy: PolicyRef::None,
            policy_expr: None,
            policy_when_denied: None,
            previous_names: vec![],
            span_ref: None,
            owner_scope_sql: None,
        })];
        assert!(check(&feature, Path::new("billing.lzi")).is_empty());
    }

    #[test]
    fn stub_table_has_both_documented_entries() {
        assert_eq!(STUB_TABLE.len(), 2);
        assert!(STUB_TABLE.contains(&STUB_TARGET_SOURCE));
        assert!(STUB_TABLE.contains(&STUB_RETENTION_ARCHIVE));
    }
}
