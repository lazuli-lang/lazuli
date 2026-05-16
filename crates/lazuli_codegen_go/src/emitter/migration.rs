//! TenantMigration kind emission. Walks every `tenant_migration`
//! declared on a feature and emits `migrations.TenantMigrationContract`
//! values into `<feature>/migration.gen.go`.
//!
//! Proposal references:
//! - §3.11 — tenant migration contract surface.
//! - §4 — runtime-gap discipline: if the Lazuli Go lib does not expose
//!   a proposal field, keep the generated Go valid and surface a
//!   `// TODO(runtime): ...` comment inside the value literal.
//!
//! Runtime note: the current Lazuli Go migrations package exposes
//! `TenantMigrationContract`, `TenantMigrationTarget`, and
//! `IdempotencyKeySpec`, not the proposal's shorter
//! `MigrationContract`, `Target`, and raw idempotency string shape.
//! The emitter uses the runtime types that exist and leaves TODO
//! comments beside the value so the mismatch stays visible.

use lazuli_ir::{BackoffStrategy, Feature, IdempotencyKey, RetryPolicy, TenantMigration};

use super::casing::{lower_camel, pascal_case};
use super::cross_feature::CrossFeatureIndex;
use super::imports::ImportSet;
use super::printer::GoPrinter;

/// Emit `<feature>/migration.gen.go` for a feature, or `None` when
/// the feature declares no tenant migrations.
pub fn emit_migration_file(
    source_label: &str,
    feature: &Feature,
    module_name: &str,
    cross_index: &CrossFeatureIndex<'_>,
) -> Option<String> {
    if feature.tenant_migrations.is_empty() {
        return None;
    }

    let _ = (module_name, cross_index);

    let mut p = GoPrinter::new();
    let mut imports = ImportSet::new();
    imports.add("lazuli.dev/runtime/lazuli/migrations");

    let mut migrations: Vec<&TenantMigration> = feature.tenant_migrations.iter().collect();
    migrations.sort_by(|a, b| a.name.cmp(&b.name));

    for migration in &migrations {
        if timeout_literal(migration.timeout.as_deref())
            .map(|literal| literal.needs_time)
            .unwrap_or(false)
        {
            imports.add("time");
        }
    }

    p.banner(source_label, &feature.name);
    imports.emit(&mut p);
    p.blank();

    let mut first_block = true;
    for migration in &migrations {
        if !first_block {
            p.blank();
        }
        first_block = false;
        emit_migration(&mut p, feature, migration);
    }

    Some(p.finish())
}

fn emit_migration(p: &mut GoPrinter, feature: &Feature, migration: &TenantMigration) {
    let qualified_name = format!("{}.{}", feature.name, migration.name);

    write_section_banner(
        p,
        &[
            format!("TenantMigration: {qualified_name}"),
            format!("  tenant_migration {}", migration.name),
        ],
    );

    p.line(&format!(
        "var {} = migrations.TenantMigrationContract{{",
        migration_var_name(&feature.name, &migration.name)
    ));
    p.indent();

    let mut rows = Vec::new();
    rows.push(LiteralRow::comment(
        "// TODO(runtime): proposal §3.11 names migrations.MigrationContract/Target; Lazuli Go exposes TenantMigrationContract/TenantMigrationTarget.",
    ));
    rows.push(LiteralRow::field("Feature:", go_string(&feature.name)));
    rows.push(LiteralRow::field("Name:", go_string(&migration.name)));
    rows.push(LiteralRow::field(
        "Target:",
        format!(
            "migrations.TenantMigrationTarget{{Axis: {}}},",
            go_string_literal(&migration.target.axis)
        ),
    ));
    rows.push(LiteralRow::field(
        "Idempotency:",
        format!(
            "migrations.IdempotencyKeySpec{{Path: {}}},",
            go_string_literal(&idempotency_path(&migration.idempotency))
        ),
    ));
    if let Some(retry) = &migration.retry {
        rows.push(LiteralRow::field("Retry:", format_retry(retry)));
    }
    if let Some(timeout) = &migration.timeout {
        match timeout_literal(Some(timeout)) {
            Some(literal) => rows.push(LiteralRow::field(
                "Timeout:",
                format!("{},", literal.expr),
            )),
            None => rows.push(LiteralRow::comment(format!(
                "// TODO(runtime): TenantMigrationContract.Timeout is time.Duration; cannot preserve authored duration \"{}\" without a parser helper.",
                escape_comment(timeout)
            ))),
        }
    }
    rows.push(LiteralRow::field(
        "HandlerPath:",
        go_string(&migration.handler.path),
    ));
    rows.push(LiteralRow::comment(
        "// TODO(runtime): TenantMigrationContract has no per-migration PreHook/PostHook fields.",
    ));
    rows.push(LiteralRow::comment(
        "// TODO(runtime): TenantMigrationContract has no per-migration checkpoint field; runtime exposes deploy-level Checkpoint only.",
    ));

    emit_literal_rows(p, &rows);

    p.dedent();
    p.line("}");
}

enum LiteralRow {
    Field { key: String, value: String },
    Comment(String),
}

impl LiteralRow {
    fn field(key: &str, value: String) -> Self {
        Self::Field {
            key: key.to_owned(),
            value,
        }
    }

    fn comment(text: impl Into<String>) -> Self {
        Self::Comment(text.into())
    }
}

fn emit_literal_rows(p: &mut GoPrinter, rows: &[LiteralRow]) {
    let key_width = rows
        .iter()
        .filter_map(|row| match row {
            LiteralRow::Field { key, .. } => Some(key.len()),
            LiteralRow::Comment(_) => None,
        })
        .max()
        .unwrap_or(0);

    for row in rows {
        match row {
            LiteralRow::Field { key, value } => {
                let pad = key_width.saturating_sub(key.len());
                p.line(&format!("{}{} {}", key, " ".repeat(pad), value));
            }
            LiteralRow::Comment(text) => p.line(text),
        }
    }
}

fn format_retry(retry: &RetryPolicy) -> String {
    format!(
        "&migrations.RetryPolicy{{Count: {}, Backoff: {}}},",
        retry.count,
        backoff_const(retry.backoff)
    )
}

fn backoff_const(backoff: BackoffStrategy) -> &'static str {
    match backoff {
        BackoffStrategy::Fixed => "migrations.BackoffFixed",
        BackoffStrategy::Exponential => "migrations.BackoffExponential",
    }
}

struct TimeoutLiteral {
    expr: String,
    needs_time: bool,
}

fn timeout_literal(raw: Option<&str>) -> Option<TimeoutLiteral> {
    let raw = raw?.trim();
    if raw.is_empty() {
        return None;
    }

    let split_at = raw
        .find(|ch: char| !ch.is_ascii_digit())
        .unwrap_or(raw.len());
    if split_at == 0 || split_at == raw.len() {
        return None;
    }

    let amount = &raw[..split_at];
    if !amount.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }
    let unit = &raw[split_at..];
    let go_unit = match unit {
        "ns" => "Nanosecond",
        "us" => "Microsecond",
        "ms" => "Millisecond",
        "s" => "Second",
        "m" => "Minute",
        "h" => "Hour",
        _ => return None,
    };

    if amount == "0" {
        return Some(TimeoutLiteral {
            expr: "0".to_owned(),
            needs_time: false,
        });
    }
    if amount == "1" {
        return Some(TimeoutLiteral {
            expr: format!("time.{go_unit}"),
            needs_time: true,
        });
    }
    Some(TimeoutLiteral {
        expr: format!("{amount} * time.{go_unit}"),
        needs_time: true,
    })
}

fn idempotency_path(idempotency: &IdempotencyKey) -> String {
    idempotency.by.segments.join(".")
}

fn migration_var_name(feature_name: &str, migration_name: &str) -> String {
    format!(
        "{}{}Migration",
        lower_camel(feature_name),
        pascal_case(migration_name)
    )
}

fn write_section_banner(p: &mut GoPrinter, lines: &[String]) {
    let rule = "-".repeat(76);
    p.line(&format!("// {rule}"));
    for line in lines {
        p.line(&format!("// {line}"));
    }
    p.line(&format!("// {rule}"));
    p.blank();
}

fn go_string(raw: &str) -> String {
    format!("{},", go_string_literal(raw))
}

fn go_string_literal(raw: &str) -> String {
    format!("\"{}\"", escape_string(raw))
}

fn escape_string(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            _ => out.push(ch),
        }
    }
    out
}

fn escape_comment(raw: &str) -> String {
    raw.replace('\n', " ").replace('\r', " ")
}

#[cfg(test)]
mod feature_emit_tests {
    use super::*;
    use lazuli_ir::{
        AppManifest, Defaults, Module, Path, PathRef, Policies, TenantMigrationTarget,
    };

    fn base_feature(name: &str) -> Feature {
        Feature {
            name: name.to_owned(),
            purpose: None,
            non_goals: Vec::new(),
            context_path: None,
            defaults: Defaults {
                tenancy: None,
                timestamps: false,
                policy: None,
            },
            uses: Vec::new(),
            uses_spans: Vec::new(),
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
            commands: Vec::new(),
            apis: Vec::new(),
            records: Vec::new(),
            queries: Vec::new(),
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
            previous_names: Vec::new(),
            span_ref: None,
        }
    }

    fn minimal_app() -> AppManifest {
        AppManifest {
            name: "test".to_owned(),
            title: None,
            version: None,
        lazuli_version: None,
            targets: Vec::new(),
            default_locale: None,
            default_timezone: None,
            auth_failed_redirect: None,
            not_found: None,
            error_pages: Vec::new(),
            uses: Vec::new(),
            packs: Vec::new(),
            bindings: Vec::new(),
            architecture: None,
            services: Vec::new(),
            communication: None,
            environments: Vec::new(),
            urls: Vec::new(),
            cors: None,
            headers: None,
            cookie: None,
            proxy: None,
            limits: None,
            env: Vec::new(),
            integrations: Vec::new(),
            capabilities: Vec::new(),
            runtime: Vec::new(),
            deploy: None,
            logging: None,
            tracing: None,
            observability: None,
            locale: None,
            encryption_bindings: Vec::new(),
            span_ref: None,
        }
    }

    fn module_with_feature(feature: Feature) -> Module {
        Module {
            workspace: None,
            contracts: Vec::new(),
            app: Some(minimal_app()),
            registry: None,
            profiles: Vec::new(),
            design: None,
            rbac: None,
            features: vec![feature],
        }
    }

    fn emit(feature: Feature) -> Option<String> {
        let module = module_with_feature(feature);
        let index = CrossFeatureIndex::build(&module);
        emit_migration_file("examples/x.lzi", &module.features[0], "lazuli/test", &index)
    }

    fn migration(name: &str) -> TenantMigration {
        TenantMigration {
            name: name.to_owned(),
            target: TenantMigrationTarget {
                operation: None,
                axis: "org".to_owned(),
            },
            idempotency: IdempotencyKey {
                by: Path::from_segments(["tenant", "org_id"]),
            },
            retry: Some(RetryPolicy {
                count: 3,
                backoff: BackoffStrategy::Exponential,
            }),
            timeout: Some("5m".to_owned()),
            handler: PathRef::authored(format!("./migrations/{name}.go")),
            previous_names: Vec::new(),
            span_ref: None,
        }
    }

    #[test]
    fn empty_feature_returns_none() {
        let feature = base_feature("customer");
        assert!(emit(feature).is_none());
    }

    #[test]
    fn canonical_migration_emits_contract_value() {
        let mut feature = base_feature("customer");
        feature
            .tenant_migrations
            .push(migration("backfill_customer_score"));

        let out = emit(feature).expect("must emit");
        assert!(out.contains("// Code generated by lazuli; DO NOT EDIT."));
        assert!(out.contains("package customer"));
        assert!(out.contains("\"time\""));
        assert!(out.contains("\"lazuli.dev/runtime/lazuli/migrations\""));
        assert!(out.contains(
            "var customerBackfillCustomerScoreMigration = migrations.TenantMigrationContract{"
        ));
        assert!(out.contains("Feature:     \"customer\","));
        assert!(out.contains("Name:        \"backfill_customer_score\","));
        assert!(out.contains("Target:      migrations.TenantMigrationTarget{Axis: \"org\"},"));
        assert!(
            out.contains("Idempotency: migrations.IdempotencyKeySpec{Path: \"tenant.org_id\"},")
        );
        assert!(out.contains(
            "Retry:       &migrations.RetryPolicy{Count: 3, Backoff: migrations.BackoffExponential},"
        ));
        assert!(out.contains("Timeout:     5 * time.Minute,"));
        assert!(out.contains("HandlerPath: \"./migrations/backfill_customer_score.go\","));
        assert!(out.contains("// TODO(runtime): TenantMigrationContract has no per-migration PreHook/PostHook fields."));
    }

    #[test]
    fn feature_emit() {
        let mut feature = base_feature("billing");
        feature.tenant_migrations.push(migration("seed_orgs"));

        let out = emit_migration_file(
            "features/billing/billing.lzi",
            &feature,
            "lazuli/test",
            &CrossFeatureIndex::build(&module_with_feature(feature.clone())),
        )
        .expect("feature with tenant_migrations must emit migration file");

        assert!(!out.is_empty());
        assert!(out.contains("// Code generated by lazuli; DO NOT EDIT."));
        assert!(out.contains("package billing"));
        assert!(out.contains("var billingSeedOrgsMigration = migrations.TenantMigrationContract{"));
        assert!(out.contains("Name:        \"seed_orgs\","));
        assert!(out.contains("HandlerPath: \"./migrations/seed_orgs.go\","));
    }

    #[test]
    fn fixed_retry_and_missing_timeout_are_omitted_cleanly() {
        let mut feature = base_feature("customer");
        let mut item = migration("seed_orgs");
        item.retry = Some(RetryPolicy {
            count: 1,
            backoff: BackoffStrategy::Fixed,
        });
        item.timeout = None;
        feature.tenant_migrations.push(item);

        let out = emit(feature).expect("must emit");
        assert!(!out.contains("\"time\""));
        assert!(out.contains(
            "Retry:       &migrations.RetryPolicy{Count: 1, Backoff: migrations.BackoffFixed},"
        ));
        assert!(!out.contains("Timeout:"));
    }

    #[test]
    fn invalid_timeout_emits_runtime_todo_and_stays_deterministic() {
        let mut feature = base_feature("customer");
        let mut zebra = migration("zebra");
        zebra.timeout = Some("1 day".to_owned());
        let alpha = migration("alpha");
        feature.tenant_migrations.push(zebra);
        feature.tenant_migrations.push(alpha);

        let out = emit(feature.clone()).expect("must emit");
        let again = emit(feature).expect("must emit");
        assert_eq!(out, again);
        assert!(out.contains("// TODO(runtime): TenantMigrationContract.Timeout is time.Duration; cannot preserve authored duration \"1 day\" without a parser helper."));
        let alpha_pos = out.find("TenantMigration: customer.alpha").expect("alpha");
        let zebra_pos = out.find("TenantMigration: customer.zebra").expect("zebra");
        assert!(alpha_pos < zebra_pos);
    }
}
