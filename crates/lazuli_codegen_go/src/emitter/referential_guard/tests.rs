//! Spec 0014 — golden tests for the referential-guard emitter.
//!
//! These are the load-bearing correctness gate: they assert the emitted
//! `EXISTS` carries `tenant_id = $N` IFF the referencing relation is
//! tenant-scoped and `deleted_at IS NULL` IFF it is soft-deletable, handles
//! the `where`-subset filter, and rejects with `runtime.ErrReferencedInUse`
//! — matching the hand-written pauta guards exactly.

use lazuli_ir::{
    BuiltinType, Defaults, Feature, Field, FieldConstraints, Policies, Resource, RestrictOnDelete,
    Tenancy, TypeRef,
};

use super::emit_referential_guard_file;

fn base_feature(name: &str) -> Feature {
    Feature {
        name: name.to_owned(),
        purpose: None,
        non_goals: Vec::new(),
        context_path: None,
        knowledge: None,
        defaults: Defaults {
            tenancy: None,
            timestamps: false,
            policy: None,
            rate_limit: None,
            audit: None,
        },
        uses: Vec::new(),
        uses_spans: Vec::new(),
        uses_versions: Vec::new(),
        requirements: Vec::new(),
        enums: Vec::new(),
        resources: Vec::new(),
        events: Vec::new(),
        rules: Vec::new(),
        policies: Policies {
            categories: Vec::new(),
            fields: Vec::new(),
            span_ref: None,
        },
        errors: None,
        commands: Vec::new(),
        apis: Vec::new(),
        records: Vec::new(),
        queries: Vec::new(),
        resume_routers: Vec::new(),
        workflows: Vec::new(),
        jobs: Vec::new(),
        webhooks: Vec::new(),
        notifications: Vec::new(),
        event_groups: Vec::new(),
        tenant_migrations: Vec::new(),
        translation: None,
        pollers: vec![],
        auth: None,
        surfaces: Vec::new(),
        extensions: Vec::new(),
        escape_routes: Vec::new(),
        agents: Vec::new(),
        reports: Vec::new(),
        channels: Vec::new(),
        caches: Vec::new(),
        aggregates: vec![],
        mcp_servers: vec![],
        previous_names: Vec::new(),
        span_ref: None,
        synth_origins: std::collections::BTreeMap::new(),
    }
}

fn id_field() -> Field {
    Field {
        name: "name".to_owned(),
        type_ref: TypeRef::Builtin(BuiltinType::Text),
        required: true,
        unique: false,
        slug: false,
        default: None,
        derived_from: None,
        computed_date: None,
        constraints: FieldConstraints::default(),
        full_text: false,
        previous_names: Vec::new(),
        pii: None,
        owner_axis: None,
        cross_feature_target: None,
        span_ref: None,
    }
}

fn resource_with_guards(name: &str, guards: Vec<RestrictOnDelete>) -> Resource {
    Resource {
        name: name.to_owned(),
        public_contract: None,
        tenancy: None,
        soft_delete: false,
        soft_delete_actor: false,
        timestamps: None,
        fields: vec![id_field()],
        constraints: Vec::new(),
        validate: None,
        validates: Vec::new(),
        retention: None,
        previous_names: Vec::new(),
        span_ref: None,
        lifecycle: None,
        invariants: vec![],
        lock: None,
        composite_key: None,
        conventions: Vec::new(),
        lifecycle_routes: None,
        polymorphic_refs: Vec::new(),
        many_through: Vec::new(),
        restrict_on_delete: guards,
        append_only: false,
    }
}

fn guard(relation: &str, fk: &str, tenant_scoped: bool, soft_delete: bool) -> RestrictOnDelete {
    RestrictOnDelete {
        relation: relation.to_owned(),
        fk: fk.to_owned(),
        extra_where: None,
        tenant_scoped,
        soft_delete,
        error_code: None,
    }
}

fn emit(feature: Feature) -> String {
    emit_referential_guard_file("billing.lzi", &feature).expect("feature has guards")
}

#[test]
fn no_guards_emits_no_file() {
    let mut f = base_feature("billing");
    f.resources.push(resource_with_guards("BillingType", vec![]));
    assert!(emit_referential_guard_file("billing.lzi", &f).is_none());
}

#[test]
fn tenant_scoped_and_soft_delete_both_emitted() {
    // The canonical pauta case (billing_type → invoice): the referencing
    // relation carries BOTH tenant_id and deleted_at, so the EXISTS must
    // emit both predicates — matching guard_billing_type_in_use.go.
    let mut f = base_feature("billing");
    f.resources.push(resource_with_guards(
        "BillingType",
        vec![guard("invoice", "billing_type_id", true, true)],
    ));
    let out = emit(f);
    assert!(out.contains("SELECT 1 FROM invoice"), "{out}");
    assert!(out.contains("WHERE billing_type_id = $1"), "{out}");
    assert!(out.contains("AND tenant_id = $2"), "tenant scope missing:\n{out}");
    assert!(
        out.contains("AND deleted_at IS NULL"),
        "soft-delete predicate missing:\n{out}"
    );
    assert!(out.contains("return runtime.ErrReferencedInUse"), "{out}");
    // Argument order: id first ($1), tenantID second ($2).
    assert!(out.contains("db.QueryRow(ctx, q, id, tenantID)"), "{out}");
    assert!(out.contains("func guardBillingTypeInvoiceRefs("), "{out}");
    // BUG 1 regression guard — the `db` param MUST be the runtime-defined
    // `lazuli.DBTX` interface (not the phantom `lazuli.DBTX` that never
    // existed plus a `QueryRowContext` pgx never had). `any` id/tenant lets
    // the int64 call-site values pass straight through pgx.
    assert!(
        out.contains("func guardBillingTypeInvoiceRefs(ctx context.Context, db lazuli.DBTX, tenantID, id any) error"),
        "guard signature must use the defined lazuli.DBTX type + any args:\n{out}"
    );
}

#[test]
fn neither_tenant_nor_soft_delete_emits_bare_exists() {
    // A relation with neither column: NO tenant_id / deleted_at predicate,
    // and tenantID must NOT appear in the arg list.
    let mut f = base_feature("ops");
    f.resources.push(resource_with_guards(
        "WorkflowTemplate",
        vec![guard("job", "workflow_template_id", false, false)],
    ));
    let out = emit(f);
    assert!(out.contains("WHERE workflow_template_id = $1"), "{out}");
    assert!(
        !out.contains("tenant_id"),
        "tenant scope must be absent for a non-tenant relation:\n{out}"
    );
    assert!(
        !out.contains("deleted_at"),
        "soft-delete must be absent for a non-soft-delete relation:\n{out}"
    );
    assert!(out.contains("db.QueryRow(ctx, q, id)"), "{out}");
    assert!(
        !out.contains("db.QueryRow(ctx, q, id, tenantID)"),
        "tenantID must not be passed to the query for a non-tenant relation:\n{out}"
    );
}

#[test]
fn soft_delete_only_skips_tenant_predicate() {
    let mut f = base_feature("ops");
    f.resources.push(resource_with_guards(
        "WorkflowTemplate",
        vec![guard("job", "workflow_template_id", false, true)],
    ));
    let out = emit(f);
    assert!(out.contains("AND deleted_at IS NULL"), "{out}");
    assert!(!out.contains("tenant_id"), "{out}");
}

#[test]
fn extra_where_subset_filter_is_appended() {
    // guard_no_open_activities: only OPEN activities count.
    let mut f = base_feature("ops");
    let mut g = guard("activity", "step_id", true, true);
    g.extra_where = Some("status = 'open'".to_owned());
    f.resources
        .push(resource_with_guards("WorkflowStep", vec![g]));
    let out = emit(f);
    assert!(out.contains("AND tenant_id = $2"), "{out}");
    assert!(out.contains("AND deleted_at IS NULL"), "{out}");
    assert!(
        out.contains("AND (status = 'open')"),
        "extra_where subset filter missing:\n{out}"
    );
}

#[test]
fn multiple_guards_on_one_resource_each_emit_a_function() {
    // billing_type protected against BOTH invoice and quote references.
    let mut f = base_feature("billing");
    f.resources.push(resource_with_guards(
        "BillingType",
        vec![
            guard("invoice", "billing_type_id", true, true),
            guard("quote", "billing_type_id", true, false),
        ],
    ));
    let out = emit(f);
    assert!(out.contains("func guardBillingTypeInvoiceRefs("), "{out}");
    assert!(out.contains("func guardBillingTypeQuoteRefs("), "{out}");
    // The quote relation is tenant-scoped but NOT soft-delete: its EXISTS
    // carries tenant_id but no deleted_at.
    let quote_fn = out
        .split("func guardBillingTypeQuoteRefs(")
        .nth(1)
        .expect("quote fn present");
    assert!(quote_fn.contains("FROM quote"), "{quote_fn}");
    assert!(quote_fn.contains("AND tenant_id ="), "{quote_fn}");
    // deleted_at appears in the invoice fn above but must not be in quote's.
    let quote_only = quote_fn.split("func ").next().unwrap_or(quote_fn);
    assert!(
        !quote_only.contains("deleted_at"),
        "quote (non-soft-delete) must not carry deleted_at:\n{quote_only}"
    );
}

#[test]
fn no_error_code_emits_bare_sentinel() {
    // Spec 0014 GAP-2 back-compat: a guard with no authored `error <CODE>`
    // keeps the bare `runtime.ErrReferencedInUse` sentinel (byte-identical to
    // the pre-gap2 output).
    let mut f = base_feature("billing");
    f.resources.push(resource_with_guards(
        "BillingType",
        vec![guard("invoice", "billing_type_id", true, true)],
    ));
    let out = emit(f);
    assert!(
        out.contains("return runtime.ErrReferencedInUse"),
        "bare sentinel must be kept when no error code is authored:\n{out}"
    );
    assert!(
        !out.contains("NewReferencedInUseError"),
        "constructor must NOT appear without an authored code:\n{out}"
    );
}

#[test]
fn authored_error_code_emits_domain_constructor() {
    // Spec 0014 GAP-2: a guard carrying `error CATEGORY_HAS_CUSTOMERS`
    // rejects with `runtime.NewReferencedInUseError("CATEGORY_HAS_CUSTOMERS")`
    // — the pilot's wire-pinned domain code — instead of the bare sentinel.
    let mut f = base_feature("customer_management");
    let mut g = guard("customer", "category_id", true, true);
    g.error_code = Some("CATEGORY_HAS_CUSTOMERS".to_owned());
    f.resources
        .push(resource_with_guards("CustomerCategory", vec![g]));
    let out = emit(f);
    assert!(
        out.contains("return runtime.NewReferencedInUseError(\"CATEGORY_HAS_CUSTOMERS\")"),
        "domain-code constructor missing:\n{out}"
    );
    assert!(
        !out.contains("return runtime.ErrReferencedInUse\n"),
        "bare sentinel must NOT be emitted when a code is authored:\n{out}"
    );
    // The authored code is echoed in the self-documenting comment too.
    assert!(
        out.contains("error CATEGORY_HAS_CUSTOMERS"),
        "authored error code should appear in the clause comment:\n{out}"
    );
}

#[test]
fn count_origin_and_exists_origin_lower_identically() {
    // The IR has no notion of COUNT vs EXISTS — both hand-written forms
    // produce the SAME RestrictOnDelete, hence the SAME emitted EXISTS.
    // This test documents that invariant: a tenant+soft-delete guard always
    // canonicalizes to EXISTS regardless of how it was hand-written.
    let mut f = base_feature("billing");
    f.resources.push(resource_with_guards(
        "BillingType",
        vec![guard("invoice", "billing_type_id", true, true)],
    ));
    let out = emit(f);
    assert!(out.contains("SELECT EXISTS ("), "always EXISTS, never COUNT:\n{out}");
    assert!(!out.contains("COUNT(*)"), "COUNT must never be emitted:\n{out}");
}
