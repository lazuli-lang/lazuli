//! `conventions [crud]` per-bundle IR builders + shared synth helpers.
//!
//! Spec: `docs/proposals/ir-resource-conventions-crud.md` §5.1–§5.9.
//!
//! Each `build_*` function emits ONE IR shape — a `Command` or a
//! `Query` matching the canonical convention. The emitted IR contains
//! zero control flow (RULE-VOCAB-03 / §7); downstream codegen lowers
//! each to one fixed SQL.
//!
//! ## Layout
//!
//! * `build_create_command` / `build_update_command` /
//!   `build_delete_command` — the three write commands.
//! * `build_lookup_query` / `build_list_query` — the two read queries.
//! * `synth_crud_invalidates` — canonical invalidates list shared by
//!   every write command (closes the cache-staleness pilot bug from
//!   2026-05-22).
//! * `default_synth_command` — shared `Command` shape defaults
//!   (`policy authenticated`, `audit default`, `rate_limit`).
//! * `crud_write_rate_limit` — §5.9 single source of truth for the
//!   write-side rate limit string.

use lazuli_ir as ir;

use super::fields::{input_field_assignments, input_to_command_input};

/// §5.2 — build `create_<resource>` command IR.
pub(crate) fn build_create_command(
    name: &str,
    resource: &str,
    input_fields: &[(&ir::Field, bool)],
) -> ir::Command {
    ir::Command {
        name: name.to_owned(),
        public_contract: None,
        kind: ir::CommandKind::Create,
        route: Vec::new(),
        input: input_to_command_input(input_fields),
        target: None,
        lets: Vec::new(),
        effect: ir::CommandEffect::Creates(ir::CreateEffect {
            resource: ir::QualifiedName {
                feature: None,
                name: resource.to_owned(),
            },
            from_input: true,
            // §5.2 — one `<field> = input.<field>` assignment per input
            // slot so the codegen emits a populated `lazuli.Bindings{}`
            // body. Without this, the synthesized INSERT had no columns
            // to bind and tripped runtime panics at first call. The
            // emitter checks `TypedSlot.required` to decide between
            // `FromInput` (required) and `FromInputOptional` (optional,
            // skip-on-nil so column defaults apply).
            assignments: input_field_assignments(input_fields),
        }),
        ..default_synth_command(crud_write_rate_limit())
    }
}

/// §5.3 — build `update_<resource>` command IR.
pub(crate) fn build_update_command(
    name: &str,
    resource: &str,
    input_fields: &[(&ir::Field, bool)],
) -> ir::Command {
    ir::Command {
        name: name.to_owned(),
        public_contract: None,
        kind: ir::CommandKind::Update,
        route: vec![ir::RouteSlot {
            name: "id".to_owned(),
            type_ref: ir::TypeRef::Builtin(ir::BuiltinType::Id),
            from: None,
            kind: ir::RouteSlotKind::Plain,
        }],
        input: input_to_command_input(input_fields),
        target: None,
        lets: Vec::new(),
        effect: ir::CommandEffect::Updates(ir::UpdateEffect {
            resource: ir::QualifiedName {
                feature: None,
                name: resource.to_owned(),
            },
            // §5.3 — every input slot becomes a `<field> = input.<field>`
            // assignment. `update_input_fields` marks all of them
            // optional, so the codegen emits `FromInputOptional` and
            // the runtime skips columns whose input pointer was nil —
            // i.e. fields the wire payload didn't include stay
            // untouched (partial-update semantics, §5.3 third para).
            assignments: input_field_assignments(input_fields),
            // Synth CRUD update scopes by the route `id` key (the legacy
            // `resolve_where_keys` path), not an authored `where`.
            where_clause: Vec::new(),
        }),
        ..default_synth_command(crud_write_rate_limit())
    }
}

/// Build the canonical `invalidates` list for a synth `create_<R>` /
/// `update_<R>` / `delete_<R>` command. Without this list, clients
/// (TS `useLazuliCommand`) never refresh `lookup_<R>` and
/// `list_<R>s` after a mutation — the cached query result is shown
/// until next manual reload. The 2026-05-22 the canonical pilot settings save
/// outage surfaced exactly this: after the partial-update bug was
/// fixed, users still saw stale data on re-entering the panel
/// because every synth command shipped with `invalidates: []`.
///
/// When the resource also declares `conventions [me]`, the `me`
/// bundle's `lookup_my_<R>` query is appended too — it shares the
/// same row set, just keyed off the actor instead of the route id.
pub(crate) fn synth_crud_invalidates(
    lookup_name: &str,
    list_name: &str,
    has_me: bool,
    resource_snake: &str,
) -> Vec<ir::InvalidatesSpec> {
    let mut out = vec![
        ir::InvalidatesSpec {
            query: ir::QualifiedName {
                feature: None,
                name: lookup_name.to_owned(),
            },
            args: Vec::new(),
        },
        ir::InvalidatesSpec {
            query: ir::QualifiedName {
                feature: None,
                name: list_name.to_owned(),
            },
            args: Vec::new(),
        },
    ];
    if has_me {
        out.push(ir::InvalidatesSpec {
            query: ir::QualifiedName {
                feature: None,
                name: format!("lookup_my_{}", resource_snake),
            },
            args: Vec::new(),
        });
    }
    out
}

/// §5.4 — build `delete_<resource>` command IR.
pub(crate) fn build_delete_command(name: &str, resource: &str) -> ir::Command {
    ir::Command {
        name: name.to_owned(),
        public_contract: None,
        kind: ir::CommandKind::Delete,
        route: vec![ir::RouteSlot {
            name: "id".to_owned(),
            type_ref: ir::TypeRef::Builtin(ir::BuiltinType::Id),
            from: None,
            kind: ir::RouteSlotKind::Plain,
        }],
        input: ir::CommandInput::Empty,
        target: None,
        lets: Vec::new(),
        effect: ir::CommandEffect::Deletes(ir::DeleteEffect {
            resource: ir::QualifiedName {
                feature: None,
                name: resource.to_owned(),
            },
            // Synth CRUD delete scopes by the route `id` key, not an
            // authored `where`.
            where_clause: Vec::new(),
        }),
        ..default_synth_command(crud_write_rate_limit())
    }
}

/// §5.5 — build `lookup_<resource>` query IR.
pub(crate) fn build_lookup_query(name: &str, resource: &str) -> ir::Query {
    let _ = resource;
    ir::Query::Lookup(ir::LookupQuery {
        name: name.to_owned(),
        public_contract: None,
        params: Vec::new(),
        keys: vec![ir::KeyClause {
            path: ir::Path::from_segments(["id".to_owned()]),
            equals: ir::Expr::Path(ir::Path::from_segments(["id".to_owned()])),
        }],
        scope: Vec::new(),
        scope_override: false,
        filters: Vec::new(),
        policy: ir::PolicyRef::Local("authenticated".to_owned()),
        policy_expr: None,
        policy_when_denied: None,
        previous_names: Vec::new(),
        span_ref: None,
        owner_scope_sql: None,
    })
}

/// §5.6 — build `list_<resource>s` query IR.
pub(crate) fn build_list_query(name: &str, resource: &str) -> ir::Query {
    let _ = resource;
    ir::Query::List(ir::ListQuery {
        name: name.to_owned(),
        public_contract: None,
        params: vec![
            ir::TypedSlot {
                name: "limit".to_owned(),
                type_ref: ir::TypeRef::Builtin(ir::BuiltinType::Integer),
                required: false,
                constraints: ir::FieldConstraints::default(),
                validate_skip: false,
            },
            ir::TypedSlot {
                name: "offset".to_owned(),
                type_ref: ir::TypeRef::Builtin(ir::BuiltinType::Integer),
                required: false,
                constraints: ir::FieldConstraints::default(),
                validate_skip: false,
            },
        ],
        scope: Vec::new(),
        scope_override: false,
        filters: Vec::new(),
        order: Vec::new(),
        // §5.6 default limit 50.
        paginate: Some(50),
        modifier: None,
        cache: None,
        policy: ir::PolicyRef::Local("authenticated".to_owned()),
        policy_expr: None,
        policy_when_denied: None,
        previous_names: Vec::new(),
        span_ref: None,
        owner_scope_sql: None,
    })
}

/// Common command-shape defaults applied to every synthesized CRUD
/// command. `policy authenticated`, `audit default`, `rate_limit` set
/// by the caller (write vs read uses different limits per §5.9).
pub(crate) fn default_synth_command(rate_limit: &str) -> ir::Command {
    ir::Command {
        name: String::new(),
        public_contract: None,
        kind: ir::CommandKind::Returns,
        route: Vec::new(),
        input: ir::CommandInput::Empty,
        target: None,
        lets: Vec::new(),
        effect: ir::CommandEffect::None,
        policy: ir::PolicyRef::Local("authenticated".to_owned()),
        policy_expr: None,
        policy_when_denied: None,
        emits: Vec::new(),
        rate_limit: Some(ir::RateLimitSpec::from_default(rate_limit.to_owned())),
        audit: Some(ir::AuditSpec {
            subjects: vec!["default".to_owned()],
            emit_to: None,
            data_subject: None,
            record_before: false,
            record_after: false,
            retain_for: None,
            materialize: None,
        }),
        approval: None,
        invalidates: Vec::new(),
        external_calls: Vec::new(),
        timeout: None,
        retry: None,
        idempotency: None,
        write_window: None,
        deprecated: None,
        handler: None,
        tests: None,
        previous_names: Vec::new(),
        span_ref: None,
        triggers: Vec::new(),
        synthesized_from_cap_file: None,
        // owner-scope §7.3 — left `None` here; the synth pass mutates
        // each synthesized command to attach the resolved scope (see
        // `synthesize_conventions`). The default keeps tenant-only
        // shape stable for command IR not produced by the synth pass.
        owner_scope_sql: None,
        derived_from: None,
    }
}

/// §5.9 — create / update / delete share `rate_limit "100 per 10 minutes per ip"`.
pub(crate) fn crud_write_rate_limit() -> &'static str {
    "100 per 10 minutes per ip"
}
