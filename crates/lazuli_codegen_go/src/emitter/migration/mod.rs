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
///
/// ## Examples
///
/// ```ignore
/// let go_src = emit_migration_file("billing.lzi", &feature, "demo", &cross_index);
/// ```
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

    p.banner(
        source_label,
        &super::casing::gen_package_name(&feature.name),
    );
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
    raw.replace(['\n', '\r'], " ")
}

#[cfg(test)]
mod feature_emit_tests;
