//! Integration tests for `@correctness.migration_out_of_sync`
//! (cell A12, codegen-correctness-cycle-2026-05-21).
//!
//! Four scenarios per the cell spec:
//! 1. IR + migration in sync → no diagnostic.
//! 2. IR has a column not in the migration → warning fires.
//! 3. Migration has a column not in the IR → warning fires.
//! 4. No migration file on disk → warning fires asking the author to
//!    run the initial codegen.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use lazuli_doctor::correctness::schema_migration_present::{self, Finding, FindingKind};
use lazuli_ir::{
    BuiltinType, Defaults, Feature, Field, FieldConstraints, Policies, Resource, TypeRef,
};

// ── fixture helpers ─────────────────────────────────────────────────────────

fn make_field(name: &str, ty: BuiltinType, required: bool) -> Field {
    Field {
        name: name.to_owned(),
        type_ref: TypeRef::Builtin(ty),
        required,
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

fn make_resource(name: &str, fields: Vec<Field>) -> Resource {
    Resource {
        name: name.to_owned(),
        public_contract: None,
        tenancy: None,
        soft_delete: false,
        soft_delete_actor: false,
        timestamps: None,
        fields,
        constraints: Vec::new(),
        validate: None,
        validates: Vec::new(),
        retention: None,
        previous_names: Vec::new(),
        span_ref: None,
        lifecycle: None,
        invariants: Vec::new(),
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

fn make_feature(name: &str, resources: Vec<Resource>) -> Feature {
    Feature {
        name: name.to_owned(),
        purpose: None,
        non_goals: Vec::new(),
        context_path: None,
        knowledge: None,
        defaults: Defaults::default(),
        uses: Vec::new(),
        uses_spans: Vec::new(),
        uses_versions: Vec::new(),
        requirements: Vec::new(),
        enums: Vec::new(),
        resources,
        events: Vec::new(),
        rules: Vec::new(),
        policies: Policies::default(),
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
        pollers: Vec::new(),
        auth: None,
        surfaces: Vec::new(),
        extensions: Vec::new(),
        escape_routes: Vec::new(),
        agents: Vec::new(),
        reports: Vec::new(),
        channels: Vec::new(),
        caches: Vec::new(),
        aggregates: Vec::new(),
        mcp_servers: Vec::new(),
        previous_names: Vec::new(),
        synth_origins: BTreeMap::new(),
        span_ref: None,
    }
}

/// Build a unique temp capsule root with the optional `migration_sql`
/// written to `dist/go/migrations/001_<feature>_<resource>.sql`.
fn make_capsule(scenario: &str, migration_sql: Option<&str>) -> PathBuf {
    let unique = format!(
        "lazuli-a12-{scenario}-{pid}-{nanos}",
        scenario = scenario,
        pid = std::process::id(),
        nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0),
    );
    let root = std::env::temp_dir().join(unique);
    // Defensive cleanup if a prior run crashed between mkdir and rm.
    let _ = fs::remove_dir_all(&root);
    if let Some(sql) = migration_sql {
        let dir = root.join("dist").join("go").join("migrations");
        fs::create_dir_all(&dir).expect("mkdir migrations");
        fs::write(dir.join("001_billing_invoice.sql"), sql).expect("write migration");
    } else {
        fs::create_dir_all(&root).expect("mkdir capsule root");
    }
    root
}

fn cleanup(root: &PathBuf) {
    let _ = fs::remove_dir_all(root);
}

fn billing_invoice_resource(fields: Vec<Field>) -> (Feature, Resource) {
    let resource = make_resource("Invoice", fields);
    let feature = make_feature("billing", vec![resource.clone()]);
    (feature, resource)
}

// ── scenario 1: in sync ─────────────────────────────────────────────────────

#[test]
fn in_sync_emits_no_diagnostic() {
    let migration = "\
-- Code generated by lazuli; DO NOT EDIT.
CREATE TABLE IF NOT EXISTS \"invoice\" (
    id BIGSERIAL PRIMARY KEY,
    amount NUMERIC NOT NULL,
    description TEXT NOT NULL
);
";
    let root = make_capsule("in-sync", Some(migration));
    let (feature, _) = billing_invoice_resource(vec![
        make_field("amount", BuiltinType::Decimal, true),
        make_field("description", BuiltinType::Text, true),
    ]);
    let findings = schema_migration_present::check(&feature, &root.join("billing.lzi"), &root);
    cleanup(&root);
    assert!(
        findings.is_empty(),
        "expected no findings when IR + migration align, got {findings:?}"
    );
}

// ── scenario 2: IR has column not in migration ──────────────────────────────

#[test]
fn ir_adds_column_not_in_migration_fires_drift() {
    let migration = "\
CREATE TABLE IF NOT EXISTS invoice (
    id BIGSERIAL PRIMARY KEY,
    amount NUMERIC NOT NULL
);
";
    let root = make_capsule("ir-ahead", Some(migration));
    // IR adds `description` that the migration does not have.
    let (feature, _) = billing_invoice_resource(vec![
        make_field("amount", BuiltinType::Decimal, true),
        make_field("description", BuiltinType::Text, true),
    ]);
    let findings = schema_migration_present::check(&feature, &root.join("billing.lzi"), &root);
    cleanup(&root);
    assert_eq!(findings.len(), 1, "expected one drift finding");
    let Finding {
        kind: FindingKind::Drift { adds, drops, .. },
        feature: feat,
        resource,
        ..
    } = &findings[0]
    else {
        panic!("expected Drift kind, got {findings:?}");
    };
    assert_eq!(feat, "billing");
    assert_eq!(resource, "Invoice");
    assert_eq!(adds, &vec!["description".to_string()]);
    assert!(drops.is_empty(), "no unexpected migration columns");
    assert_eq!(
        findings[0].kind.code_id(),
        "@correctness.migration_out_of_sync",
        "diagnostic ID matches cell spec",
    );
    let msg = findings[0].message();
    assert!(
        msg.contains("description"),
        "message lists missing column: {msg}"
    );
    assert!(
        msg.contains("lazuli generate go ."),
        "message tells the author what to do: {msg}"
    );
}

// ── scenario 3: migration has column not in IR ──────────────────────────────

#[test]
fn migration_has_column_not_in_ir_fires_drift() {
    let migration = "\
CREATE TABLE IF NOT EXISTS invoice (
    id BIGSERIAL PRIMARY KEY,
    amount NUMERIC NOT NULL,
    legacy_note TEXT
);
";
    let root = make_capsule("mig-ahead", Some(migration));
    // IR does not declare `legacy_note` — the migration carries a
    // column the IR no longer wants. The drift warning must flag it.
    let (feature, _) =
        billing_invoice_resource(vec![make_field("amount", BuiltinType::Decimal, true)]);
    let findings = schema_migration_present::check(&feature, &root.join("billing.lzi"), &root);
    cleanup(&root);
    assert_eq!(findings.len(), 1, "expected one drift finding");
    let Finding {
        kind: FindingKind::Drift { adds, drops, .. },
        ..
    } = &findings[0]
    else {
        panic!("expected Drift kind, got {findings:?}");
    };
    assert!(adds.is_empty(), "IR contributes nothing new");
    assert_eq!(drops, &vec!["legacy_note".to_string()]);
    let msg = findings[0].message();
    assert!(
        msg.contains("legacy_note"),
        "message lists migration-only column: {msg}"
    );
}

// ── scenario 4: no migration on disk ────────────────────────────────────────

#[test]
fn missing_migration_directory_fires_missing() {
    let root = make_capsule("no-migrations-dir", None);
    let (feature, _) =
        billing_invoice_resource(vec![make_field("amount", BuiltinType::Decimal, true)]);
    let findings = schema_migration_present::check(&feature, &root.join("billing.lzi"), &root);
    cleanup(&root);
    assert_eq!(
        findings.len(),
        1,
        "expected one MigrationMissing finding when dist/go/migrations is absent"
    );
    assert!(matches!(
        findings[0].kind,
        FindingKind::MigrationMissing { .. }
    ));
    let msg = findings[0].message();
    assert!(
        msg.contains("lazuli generate go ."),
        "missing message instructs initial codegen: {msg}"
    );
    assert!(msg.contains("billing"), "message names the feature: {msg}");
    assert!(msg.contains("Invoice"), "message names the resource: {msg}");
}

// ── extra: empty migrations dir but no matching file ────────────────────────

#[test]
fn migrations_dir_present_but_no_match_fires_missing() {
    let migration = "\
CREATE TABLE IF NOT EXISTS audit_log (
    id BIGSERIAL PRIMARY KEY
);
";
    let root = make_capsule("no-match", Some(migration));
    // The seeded migration file is for `billing_invoice` (matches our
    // helper), so seed an unrelated resource feature and ensure the
    // dir walk still fires MigrationMissing.
    let other = make_resource(
        "Customer",
        vec![make_field("name", BuiltinType::Text, true)],
    );
    let feature = make_feature("crm", vec![other]);
    let findings = schema_migration_present::check(&feature, &root.join("crm.lzi"), &root);
    cleanup(&root);
    assert_eq!(findings.len(), 1);
    assert!(matches!(
        findings[0].kind,
        FindingKind::MigrationMissing { .. }
    ));
}

// ── GAP-13: polymorphic_ref columns are expected, not drift ─────────────────

/// A `polymorphic_ref <type> <id> targets [...]` emits its discriminator +
/// id columns to the migration but stores them on `resource.polymorphic_refs`,
/// not `resource.fields`. Before the GAP-13 codegen fix, `expected_columns_for`
/// omitted them, so the drift rule flagged `entity_type` / `entity_id` as
/// "migration-only" drops — the exact false-positive the attachments `.lzi`
/// had to waive. With the columns mirrored into the expected set, an in-sync
/// migration must produce NO finding.
#[test]
fn polymorphic_ref_columns_are_expected_not_drift() {
    // Migration carries the discriminator (with its CHECK — skipped by the
    // column parser as a constraint clause) + the id column, exactly as
    // `migration_ddl::constraint::polymorphic_ref_columns` emits them.
    let migration = "\
-- Code generated by lazuli; DO NOT EDIT.
CREATE TABLE IF NOT EXISTS \"invoice\" (
    id BIGSERIAL PRIMARY KEY,
    amount NUMERIC NOT NULL,
    entity_type TEXT NOT NULL CHECK (entity_type IN ('Job', 'Customer')),
    entity_id BIGINT NOT NULL
);
";
    let root = make_capsule("polyref-in-sync", Some(migration));
    let mut resource = make_resource("Invoice", vec![make_field(
        "amount",
        BuiltinType::Decimal,
        true,
    )]);
    resource.polymorphic_refs = vec![lazuli_ir::PolymorphicRef {
        type_field: "entity_type".to_owned(),
        id_field: "entity_id".to_owned(),
        targets: vec!["Job".to_owned(), "Customer".to_owned()],
    }];
    let feature = make_feature("billing", vec![resource]);
    let findings = schema_migration_present::check(&feature, &root.join("billing.lzi"), &root);
    cleanup(&root);
    assert!(
        findings.is_empty(),
        "polymorphic_ref columns emitted to the migration must be recognised \
         as expected (not migration-only drift), got {findings:?}"
    );
}

// ── small extension trait: expose the diagnostic code for tests ─────────────

trait FindingKindCode {
    fn code_id(&self) -> &'static str;
}

impl FindingKindCode for FindingKind {
    fn code_id(&self) -> &'static str {
        // Single diagnostic ID per cell spec — both Drift and
        // MigrationMissing roll up under the same code.
        Finding::CODE
    }
}
