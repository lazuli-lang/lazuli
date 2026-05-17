//! Cell P.C — `Poller` kind emission. Walks every `Poller` declared on a
//! feature and emits `<feature>/poller.gen.go` containing a single
//! `RegisterPollers(*poller.Registry)` function that builds one
//! `poller.Spec[...]` literal per authored poller and registers it.
//!
//! Per docs/proposals/poller-vocab.md §6.1.
//!
//! Wire-thin: imports `lazuli.dev/runtime/lazuli/poller` and `time`;
//! never reaches into the runtime's pgx/v5 surface (handlers do that
//! in user-land). Generated handler signature stubs include a doc
//! comment documenting the vendor-side idempotency contract per §10
//! risk #1 — the handler MUST be idempotent against re-invocation
//! with the same `(row.id, attempts)` pair (scheduler crash recovery).

use lazuli_ir::{
    Feature, IdempotencyKey, Path as IrPath, Poller, PollerBackoff, PollerRetryQuirk,
    PollerStateKind,
};

use super::casing::pascal_case;
use super::imports::ImportSet;
use super::patterns::{PATTERN_POLLER_REGISTER, emit_pattern_header};
use super::printer::GoPrinter;

/// Emit `<feature>/poller.gen.go` for a feature, or `None` when the
/// feature declares no pollers.
pub fn emit_poller_file(source_label: &str, feature: &Feature) -> Option<String> {
    if feature.pollers.is_empty() {
        return None;
    }

    let mut p = GoPrinter::new();
    let mut imports = ImportSet::new();
    imports.add("time");
    imports.add("lazuli.dev/runtime/lazuli/poller");

    p.banner(source_label, &super::casing::gen_package_name(&feature.name));
    imports.emit(&mut p);
    p.blank();

    // Header comment documenting the vendor-side idempotency contract.
    // Per docs/proposals/poller-vocab.md §10 risk #1: the handler MUST be
    // idempotent against re-invocation with the same `(row.id, attempts)`
    // pair. The runtime cannot guarantee what it didn't compute; this
    // comment is the surface where authors are reminded.
    p.line("// RegisterPollers wires every authored `poller <name>` in this");
    p.line(&format!(
        "// feature into `r`. Generated from {} feature `{}`.",
        source_label, feature.name
    ));
    p.line("//");
    p.line("// IMPORTANT: each poller's `resolve via @fn.<name>` handler is");
    p.line("// called once per row per tick. On scheduler crash recovery the");
    p.line("// handler MAY be invoked again with the SAME (row.id, row.attempts)");
    p.line("// pair (see docs/proposals/poller-vocab.md §10 risk #1). The");
    p.line("// handler MUST be idempotent against this — deduplicate by the");
    p.line("// vendor's external request handle if needed. The conditional");
    p.line("// UPDATE keyed on (id, attempts) inside the runtime guarantees");
    p.line("// at-most-one commit per (row, attempts) pair.");

    let mut pollers: Vec<&Poller> = feature.pollers.iter().collect();
    pollers.sort_by(|a, b| a.name.cmp(&b.name));

    emit_pattern_header(&mut p, PATTERN_POLLER_REGISTER);
    p.line("func RegisterPollers(r *poller.Registry) {");
    p.indent();
    for poll in &pollers {
        emit_register_call(&mut p, feature, poll);
    }
    p.dedent();
    p.line("}");

    Some(p.finish())
}

fn emit_register_call(p: &mut GoPrinter, feature: &Feature, poll: &Poller) {
    p.blank();
    p.line(&format!("// poller `{}` over `{}`.", poll.name, poll.source));
    p.line(&format!(
        "poller.Register(r, poller.Spec[{row_type}, {state_type}, {term_type}, {result_type}]{{",
        row_type = pascal_case(&poll.source),
        state_type = state_enum_name(&poll.source),
        term_type = poll
            .terminal_status_field
            .as_deref()
            .map(|_| pascal_case(&format!("{}_terminal", poll.source)))
            .unwrap_or_else(|| "string".to_owned()),
        result_type = "map[string]any",
    ));
    p.indent();

    p.line(&format!(
        "Name:   \"{}.{}\",",
        escape_string(&feature.name),
        escape_string(&poll.name)
    ));
    p.line(&format!(
        "Source: \"{}\",",
        escape_string(&snake_case(&poll.source))
    ));

    // Cursor
    p.line("Cursor: poller.Cursor{");
    p.indent();
    p.line(&format!(
        "NextAtField:     \"{}\",",
        escape_string(&poll.cursor.next_at_field)
    ));
    p.line(&format!(
        "ResolvedAtField: \"{}\",",
        escape_string(&poll.cursor.resolved_at_field)
    ));
    p.line(&format!(
        "AttemptsField:   \"{}\",",
        escape_string(&poll.cursor.attempts_field)
    ));
    p.dedent();
    p.line("},");

    // Retry
    p.line("Retry: poller.Retry{");
    p.indent();
    p.line(&format!("MaxAttempts: {},", poll.retry.max_attempts));
    p.line(&format!("Backoff:     {},", backoff_literal(&poll.retry.backoff)));
    p.dedent();
    p.line("},");

    // States
    p.line(&format!(
        "States: []poller.State_[{}]{{",
        state_enum_name(&poll.source)
    ));
    p.indent();
    for s in &poll.states {
        p.line(&format!(
            "{{Name: \"{}\", Kind: poller.{}}},",
            escape_string(&s.name),
            state_kind_const(s.kind),
        ));
    }
    p.dedent();
    p.line("},");

    // Tick
    p.line(&format!(
        "Tick: poller.Tick{{Every: {}, Batch: {}}},",
        duration_literal(&poll.tick.every),
        poll.tick.batch,
    ));

    // Optional fields
    if let Some(f) = &poll.terminal_status_field {
        p.line(&format!(
            "TerminalStatusField: \"{}\",",
            escape_string(f)
        ));
    }
    if let Some(f) = &poll.terminal_result_field {
        p.line(&format!(
            "TerminalResultField: \"{}\",",
            escape_string(f)
        ));
    }
    if let Some(t) = &poll.tenant_from {
        p.line(&format!(
            "TenantFrom: \"{}\",",
            escape_string(&format_path_raw(&t.path))
        ));
    }
    p.line(&format!(
        "Idempotency: \"{}\",",
        escape_string(&format_idempotency(&poll.idempotency))
    ));
    if !poll.emits.is_empty() {
        let mut sorted: Vec<&String> = poll.emits.iter().collect();
        sorted.sort();
        let entries = sorted
            .iter()
            .map(|e| format!("\"{}\"", escape_string(e)))
            .collect::<Vec<_>>()
            .join(", ");
        p.line(&format!("Emits: []string{{{entries}}},"));
    }
    if !poll.retry_quirks.is_empty() {
        p.line("RetryQuirks: []poller.Quirk{");
        p.indent();
        for q in &poll.retry_quirks {
            match q {
                PollerRetryQuirk::GenderFlipOnce {
                    when,
                    counter_field,
                    gender_field,
                } => {
                    p.line(&format!(
                        "poller.GenderFlipOnce{{When: {when:?}, CounterField: {cf:?}, GenderField: {gf:?}}},",
                        when = when,
                        cf = counter_field,
                        gf = gender_field,
                    ));
                }
            }
        }
        p.dedent();
        p.line("},");
    }

    // Resolve handler stub — codegen wires the user handler from
    // `extensions/<name>.go` via a generated package-level reference.
    p.line(&format!(
        "Resolve: {handler}_resolve, // wired from @fn.{name}; see ./handlers/{name}.go",
        handler = poll.resolve_handler.name,
        name = poll.resolve_handler.name,
    ));

    p.dedent();
    p.line("})");
}

fn state_enum_name(source: &str) -> String {
    // Per proposal §3.6, the codegen-derived state enum is named
    // `<Source>Status`. The author maintains it as a Lazuli `enum`
    // declaration; the codegen references it by name.
    format!("{}Status", pascal_case(source))
}

fn state_kind_const(kind: PollerStateKind) -> &'static str {
    match kind {
        PollerStateKind::Initial => "Initial",
        PollerStateKind::Intermediate => "Intermediate",
        PollerStateKind::Terminal => "Terminal",
    }
}

fn backoff_literal(b: &PollerBackoff) -> String {
    match b {
        PollerBackoff::Fixed { base } => {
            let base = base
                .as_deref()
                .map(duration_literal)
                .unwrap_or_else(|| "30 * time.Second".to_owned());
            format!("poller.Fixed{{Base: {base}}}")
        }
        PollerBackoff::Linear { base, cap } => {
            let cap_lit = cap
                .as_deref()
                .map(|c| format!(", Cap: {}", duration_literal(c)))
                .unwrap_or_default();
            format!(
                "poller.Linear{{Base: {}{cap_lit}}}",
                duration_literal(base)
            )
        }
        PollerBackoff::Exponential { base, cap } => {
            let cap_lit = cap
                .as_deref()
                .map(|c| format!(", Cap: {}", duration_literal(c)))
                .unwrap_or_default();
            format!(
                "poller.Exponential{{Base: {}{cap_lit}}}",
                duration_literal(base)
            )
        }
    }
}

/// Convert `<integer><s|m|h|d>` → Go `time.Duration` expression.
/// Closed unit catalog mirrors the parser (§3.14 EBNF).
fn duration_literal(d: &str) -> String {
    let (digits, unit) = d.split_at(d.find(|c: char| !c.is_ascii_digit()).unwrap_or(d.len()));
    let n: u64 = digits.parse().unwrap_or(0);
    match unit {
        "s" => format!("{n} * time.Second"),
        "m" => format!("{n} * time.Minute"),
        "h" => format!("{n} * time.Hour"),
        "d" => format!("{} * time.Hour", n.saturating_mul(24)),
        _ => format!("{n} * time.Second /* fallback */"),
    }
}

fn format_idempotency(idem: &IdempotencyKey) -> String {
    idem.by.segments.join(", ")
}

fn format_path_raw(path: &IrPath) -> String {
    // For poller `tenant_from row.<axis>_id` the path's single segment
    // already carries the dotted form (`row.org_id`). Keep verbatim.
    path.segments.join(".")
}

fn snake_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            out.push('_');
        }
        out.extend(c.to_lowercase());
    }
    out
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

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        Defaults, Feature, HandlerRef, IdempotencyKey, Path as IrPath, Poller, PollerBackoff,
        PollerCursor, PollerRetry, PollerState, PollerStateKind, PollerTick, Policies,
        TenantFromSpec,
    };

    fn mk_poller() -> Poller {
        Poller {
            name: "v8_consult_resolver".into(),
            source: "V8PendingConsult".into(),
            cursor: PollerCursor {
                next_at_field: "next_check_at".into(),
                resolved_at_field: "resolved_at".into(),
                attempts_field: "attempts".into(),
                span_ref: None,
            },
            retry: PollerRetry {
                max_attempts: 30,
                backoff: PollerBackoff::Exponential {
                    base: "30s".into(),
                    cap: Some("10m".into()),
                },
                span_ref: None,
            },
            states: vec![
                PollerState {
                    name: "pending".into(),
                    kind: PollerStateKind::Initial,
                    span_ref: None,
                },
                PollerState {
                    name: "resolved".into(),
                    kind: PollerStateKind::Terminal,
                    span_ref: None,
                },
            ],
            resolve_handler: HandlerRef {
                namespace: "fn".into(),
                name: "poll_v8".into(),
                span_ref: None,
            },
            terminal_status_field: Some("final_status".into()),
            terminal_result_field: Some("final_resultado".into()),
            tick: PollerTick {
                every: "15s".into(),
                batch: 100,
            },
            tenant_from: Some(TenantFromSpec {
                path: IrPath::from_segments(["row.org_id"]),
            }),
            idempotency: IdempotencyKey {
                by: IrPath::from_segments(["row.id", "row.attempts"]),
            },
            audit: None,
            emits: vec!["v8_consult_resolved".into(), "v8_consult_failed".into()],
            retry_quirks: vec![],
            span_ref: None,
        }
    }

    fn mk_feature(p: Poller) -> Feature {
        Feature {
            name: "multi_bank".into(),
            purpose: None,
            non_goals: vec![],
            context_path: None,
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
            commands: vec![],
            apis: vec![],
            records: vec![],
            queries: vec![],
            workflows: vec![],
            jobs: vec![],
            webhooks: vec![],
            notifications: vec![],
            event_groups: vec![],
            tenant_migrations: vec![],
            translation: None,
            pollers: vec![p],
            auth: None,
            surfaces: vec![],
            extensions: vec![],
            escape_routes: vec![],
            agents: vec![],
            reports: vec![],
            channels: vec![],
            caches: vec![],
            aggregates: vec![],
            previous_names: vec![],
            span_ref: None,
        }
    }

    #[test]
    fn emits_register_function_with_spec_literal() {
        let feat = mk_feature(mk_poller());
        let out = emit_poller_file("multi_bank.lzi", &feat).expect("emits");
        assert!(out.contains("//lazuli:pattern poller_register v1\nfunc RegisterPollers"));
        assert!(out.contains("func RegisterPollers"));
        assert!(out.contains("poller.Register(r, poller.Spec["));
        assert!(out.contains("\"multi_bank.v8_consult_resolver\""));
        assert!(out.contains("\"v8_pending_consult\"")); // snake_case
        assert!(out.contains("NextAtField:"));
        assert!(out.contains("poller.Exponential{Base: 30 * time.Second, Cap: 10 * time.Minute}"));
        assert!(out.contains("poll_v8_resolve"));
        assert!(out.contains("\"v8_consult_resolved\""));
        // Vendor-idempotency doc comment present.
        assert!(out.contains("crash recovery"));
    }

    #[test]
    fn no_file_when_no_pollers() {
        let mut p = mk_poller();
        p.name = "p".into();
        let mut feat = mk_feature(p);
        feat.pollers.clear();
        assert!(emit_poller_file("x.lzi", &feat).is_none());
    }
}

