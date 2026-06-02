use super::*;
use lazuli_ir::{
    AuditSpec, Command, CommandEffect, CommandInput, CommandKind, Defaults, Event, EventKind,
    OutboxMode, Policies, PolicyRef,
};
use std::fs;
use tempfile::TempDir;

// ── IR fixture builders ────────────────────────────────────────────────

fn mk_feature(name: &str) -> Feature {
    Feature {
        name: name.into(),
        purpose: None,
        non_goals: vec![],
        context_path: None,
        knowledge: None,
        defaults: Defaults::default(),
        uses: vec![],
        uses_spans: vec![],
        uses_versions: vec![],
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

fn mk_command(name: &str, audit: Option<AuditSpec>) -> Command {
    Command {
        name: name.into(),
        public_contract: None,
        kind: CommandKind::Create,
        route: vec![],
        input: CommandInput::Empty,
        target: None,
        lets: vec![],
        effect: CommandEffect::None,
        policy: PolicyRef::None,
        policy_expr: None,
        policy_when_denied: None,
        emits: vec![],
        rate_limit: None,
        audit,
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
        owner_scope_sql: None,
        previous_names: vec![],
        span_ref: None,
        derived_from: None,
    }
}

fn audit_default() -> AuditSpec {
    AuditSpec {
        subjects: vec!["default".into()],
        emit_to: None,
        data_subject: None,
        record_before: false,
        record_after: false,
        retain_for: None,
        materialize: None,
    }
}

fn mk_event(name: &str, outbox: OutboxMode) -> Event {
    Event {
        name: name.into(),
        kind: EventKind::Domain,
        payload: vec![],
        payload_none: true,
        level: None,
        outbox,
        previous_names: vec![],
        span_ref: None,
    }
}

/// Write `dist/go/migrations/<name>` under `root`, creating parents.
fn write_migration(root: &Path, name: &str, body: &str) -> PathBuf {
    let dir = root.join("dist").join("go").join("migrations");
    fs::create_dir_all(&dir).unwrap();
    let p = dir.join(name);
    fs::write(&p, body).unwrap();
    p
}

const AUDIT_LOG_DDL: &str = "CREATE TABLE IF NOT EXISTS audit_log (id BIGSERIAL PRIMARY KEY);\n";

// ── positive cases (rule fires) ─────────────────────────────────────────

#[test]
fn audit_command_without_lazuli_audit_migration_fires() {
    let tmp = TempDir::new().unwrap();
    // audit_log is present (always emitted) — but lazuli_audit is missing.
    write_migration(tmp.path(), "audit_log.sql", AUDIT_LOG_DDL);

    let mut feature = mk_feature("account");
    feature.commands.push(mk_command("create", Some(audit_default())));
    let features = [&feature];

    let findings = check(&features, tmp.path());
    let tables: Vec<&str> = findings.iter().map(|f| f.table.as_str()).collect();
    assert!(
        tables.contains(&"lazuli_audit"),
        "expected lazuli_audit finding, got {tables:?}"
    );
    assert!(
        !tables.contains(&"audit_log"),
        "audit_log is present — must not fire: {tables:?}"
    );
    let f = findings.iter().find(|f| f.table == "lazuli_audit").unwrap();
    assert_eq!(Finding::CODE, "RUNTIME-EMITTED-TABLE-MIGRATION-001");
    assert!(f.message().contains("does not exist"));
    assert!(f.message().contains("lazuli_audit"));
}

#[test]
fn guaranteed_outbox_without_outbox_migration_fires() {
    let tmp = TempDir::new().unwrap();
    write_migration(tmp.path(), "audit_log.sql", AUDIT_LOG_DDL);

    let mut feature = mk_feature("billing");
    feature.events.push(mk_event("InvoicePaid", OutboxMode::Guaranteed));
    let features = [&feature];

    let findings = check(&features, tmp.path());
    let tables: Vec<&str> = findings.iter().map(|f| f.table.as_str()).collect();
    assert!(
        tables.contains(&"lazuli_outbox"),
        "expected lazuli_outbox finding, got {tables:?}"
    );
}

#[test]
fn audit_log_missing_fires_when_present_command() {
    // Edge: no migrations at all but the dir exists — audit_log (Always)
    // fires because the runtime emits it for every command.
    let tmp = TempDir::new().unwrap();
    // Create an empty migrations dir (a non-framework table migration).
    write_migration(
        tmp.path(),
        "001_account_user.sql",
        "CREATE TABLE \"user\" (id BIGSERIAL PRIMARY KEY);\n",
    );

    let feature = mk_feature("account");
    let features = [&feature];

    let findings = check(&features, tmp.path());
    let tables: Vec<&str> = findings.iter().map(|f| f.table.as_str()).collect();
    assert!(
        tables.contains(&"audit_log"),
        "audit_log is Always-active and absent — must fire: {tables:?}"
    );
}

// ── negative cases (no false positive) ──────────────────────────────────

#[test]
fn complete_migration_tree_is_clean() {
    let tmp = TempDir::new().unwrap();
    write_migration(tmp.path(), "audit_log.sql", AUDIT_LOG_DDL);
    write_migration(
        tmp.path(),
        "000_lazuli_audit.sql",
        "CREATE TABLE IF NOT EXISTS lazuli_audit (id BIGSERIAL PRIMARY KEY);\n",
    );
    write_migration(
        tmp.path(),
        "002_create_lazuli_outbox.sql",
        "CREATE TABLE IF NOT EXISTS lazuli_outbox (id BIGSERIAL PRIMARY KEY);\n",
    );

    let mut feature = mk_feature("billing");
    feature.commands.push(mk_command("create", Some(audit_default())));
    feature.events.push(mk_event("InvoicePaid", OutboxMode::Guaranteed));
    let features = [&feature];

    let findings = check(&features, tmp.path());
    assert!(findings.is_empty(), "expected clean, got {findings:?}");
}

#[test]
fn inactive_tables_do_not_fire() {
    // No audit-declaring command, no guaranteed outbox → only audit_log
    // (Always) is in scope, and it is present.
    let tmp = TempDir::new().unwrap();
    write_migration(tmp.path(), "audit_log.sql", AUDIT_LOG_DDL);

    let mut feature = mk_feature("catalog");
    feature.commands.push(mk_command("create", None));
    feature.events.push(mk_event("Listed", OutboxMode::None));
    let features = [&feature];

    let findings = check(&features, tmp.path());
    assert!(
        findings.is_empty(),
        "no audit/outbox usage — must be clean, got {findings:?}"
    );
}

#[test]
fn no_migrations_dir_is_silent() {
    // No dist/go/migrations dir at all → schema_migration_present owns the
    // "run initial codegen" message; this rule stays silent.
    let tmp = TempDir::new().unwrap();
    let mut feature = mk_feature("account");
    feature.commands.push(mk_command("create", Some(audit_default())));
    let features = [&feature];

    let findings = check(&features, tmp.path());
    assert!(findings.is_empty(), "{findings:?}");
}

#[test]
fn doctor_allow_opt_out_silences() {
    let tmp = TempDir::new().unwrap();
    write_migration(tmp.path(), "audit_log.sql", AUDIT_LOG_DDL);
    write_migration(
        tmp.path(),
        "001_account_user.sql",
        "-- doctor:allow RUNTIME-EMITTED-TABLE-MIGRATION-001 — reason \"managed externally\"\n\
         CREATE TABLE \"user\" (id BIGSERIAL PRIMARY KEY);\n",
    );

    let mut feature = mk_feature("account");
    feature.commands.push(mk_command("create", Some(audit_default())));
    let features = [&feature];

    let findings = check(&features, tmp.path());
    assert!(findings.is_empty(), "opt-out must silence: {findings:?}");
}

#[test]
fn manual_activation_table_never_fires() {
    // lazuli_inbox is Activation::Manual — even with a full feature set and
    // a clean-but-incomplete migration tree, it must never fire.
    let tmp = TempDir::new().unwrap();
    write_migration(tmp.path(), "audit_log.sql", AUDIT_LOG_DDL);

    let feature = mk_feature("account");
    let features = [&feature];

    let findings = check(&features, tmp.path());
    let tables: Vec<&str> = findings.iter().map(|f| f.table.as_str()).collect();
    assert!(
        !tables.contains(&"lazuli_inbox"),
        "lazuli_inbox is Manual — must never fire: {tables:?}"
    );
}

// ── parser unit tests ───────────────────────────────────────────────────

#[test]
fn parser_picks_quoted_bare_and_if_not_exists() {
    let v = parse_create_table_names(
        "CREATE TABLE IF NOT EXISTS \"a\" (id INT); CREATE TABLE b (id INT); \
         CREATE TABLE IF NOT EXISTS public.c (id INT);",
    );
    assert_eq!(v, vec!["a".to_string(), "b".to_string(), "c".to_string()]);
}

#[test]
fn parser_ignores_word_boundary_collision() {
    let v = parse_create_table_names("-- recreate_table foo\nCREATE TABLE bar (id INT);");
    assert_eq!(v, vec!["bar".to_string()]);
}

#[test]
fn catalog_is_nonempty_and_audit_log_is_always() {
    assert!(!SYNTHESIZED_TABLES.is_empty());
    let audit = SYNTHESIZED_TABLES
        .iter()
        .find(|t| t.table == "audit_log")
        .expect("audit_log must be catalogued");
    assert_eq!(audit.activation, Activation::Always);
}
