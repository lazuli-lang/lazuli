//! Diagnostics for cryptographic primitives and integrity contracts:
//! capability tiers under `@cap.*`, the `secret_rotation` registry
//! block, and the `idempotency` source declaration.
//!
//! | Producer | Concern |
//! |---|---|
//! | [`crypto_contract_diagnostics`] | reject legacy `@cap.Secret`; require tier args on `@cap.Hashed` (algorithm), `@cap.Encrypted`/`@cap.E2ee` (key scope), `@cap.Token` (ttl, single_use, store). |
//! | [`secret_rotation_contract_diagnostics`] | `secret_rotation <name>` must declare `cadence`/`overlap` duration literals and `auto_rollback true|false`. |
//! | [`idempotency_key_diagnostics`] | `idempotency` outside `tenant_migration` must use the `by` source form. |
//!
//! Cross-feature crypto checks (binding `@key.<scope>` to a registered
//! key, profile-level cadence/overlap interaction, rotation-vs-token
//! coherence) live in `lazuli_doctor`. This module is file-local only.

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use crate::{
    capability_arg, capability_args, is_duration_literal, is_key_scope, leading_spaces,
    simple_canonical_diagnostic, warn_unknown_capability_args,
};

pub(crate) fn crypto_contract_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        // A `@cap.*` token inside a `@doctor.allow(...)` reason is opaque prose,
        // not a crypto contract declaration (spec 0028 Gap A).
        if trimmed.is_empty()
            || trimmed.starts_with('#')
            || lazuli_syntax::doctor_allow::line_is_doctor_allow_node(trimmed)
        {
            continue;
        }

        if line.contains("@cap.Secret") {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "crypto-tier",
                "`@cap.Secret` is legacy; choose an explicit tier such as `@cap.Hashed(...)`, `@cap.Encrypted(key:@key.*)`, or `@cap.Token(...)`.",
            ));
        }

        let hashed_args = capability_args(line, "Hashed");
        if line.contains("@cap.Hashed")
            && hashed_args
                .as_deref()
                .is_none_or(|args| capability_arg(args, "algorithm").is_none())
        {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "crypto-hash-algorithm",
                "`@cap.Hashed` should declare `algorithm:<name>` so the hash contract is audit-visible.",
            ));
        }
        if let Some(args) = hashed_args.as_deref() {
            warn_unknown_capability_args(
                &mut diagnostics,
                line_index,
                line,
                "@cap.Hashed",
                args,
                &["algorithm"],
            );
            if let Some(algorithm) = capability_arg(args, "algorithm")
                && !matches!(algorithm, "argon2id" | "bcrypt")
            {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::WARNING,
                    "crypto-hash-algorithm",
                    "canonical v0 hash algorithms are `argon2id` or `bcrypt` for legacy migration.",
                ));
            }
        }

        for capability in ["Encrypted", "E2ee"] {
            let args = capability_args(line, capability);
            if line.contains(&format!("@cap.{capability}"))
                && args
                    .as_deref()
                    .is_none_or(|args| capability_arg(args, "key").is_none())
            {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::WARNING,
                    "crypto-key-scope",
                    &format!(
                        "`@cap.{capability}` should declare `key:@key.<scope>` so key blast radius is audit-visible."
                    ),
                ));
            }
            if let Some(args) = args.as_deref() {
                warn_unknown_capability_args(
                    &mut diagnostics,
                    line_index,
                    line,
                    &format!("@cap.{capability}"),
                    args,
                    &["key"],
                );
                if let Some(key) = capability_arg(args, "key")
                    && !is_key_scope(key)
                {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        line,
                        DiagnosticSeverity::WARNING,
                        "crypto-key-scope",
                        "encryption capability keys should use `key:@key.<scope>`.",
                    ));
                }
            }
        }

        if line.contains("@cap.Token") {
            let token_args = capability_args(line, "Token");
            for (required, message) in [
                (
                    "ttl",
                    "`@cap.Token` should declare `ttl:<duration>` for expiry.",
                ),
                (
                    "single_use",
                    "`@cap.Token` should declare `single_use:true|false`.",
                ),
                (
                    "store",
                    "`@cap.Token` should declare `store:hashed` or another explicit storage strategy.",
                ),
            ] {
                if token_args
                    .as_deref()
                    .is_none_or(|args| capability_arg(args, required).is_none())
                {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        line,
                        DiagnosticSeverity::WARNING,
                        "crypto-token-contract",
                        message,
                    ));
                }
            }
            if let Some(args) = token_args.as_deref() {
                warn_unknown_capability_args(
                    &mut diagnostics,
                    line_index,
                    line,
                    "@cap.Token",
                    args,
                    &["ttl", "single_use", "store"],
                );
                if let Some(ttl) = capability_arg(args, "ttl")
                    && !is_duration_literal(ttl)
                {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        line,
                        DiagnosticSeverity::WARNING,
                        "crypto-token-contract",
                        "`@cap.Token` ttl should use `ttl:<duration>` such as `30s`, `10m`, `1h`, or `7d`.",
                    ));
                }
                if let Some(single_use) = capability_arg(args, "single_use")
                    && !matches!(single_use, "true" | "false")
                {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        line,
                        DiagnosticSeverity::WARNING,
                        "crypto-token-contract",
                        "`@cap.Token` single_use should be `true` or `false`.",
                    ));
                }
                if let Some(store) = capability_arg(args, "store")
                    && store != "hashed"
                {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        line,
                        DiagnosticSeverity::WARNING,
                        "crypto-token-contract",
                        "`@cap.Token` store should be `hashed` in canonical v0.",
                    ));
                }
            }
        }
    }

    diagnostics
}

/// `secret_rotation <name>` declared inside `registry`. Children are
/// closed-catalog `cadence <dur>`, `overlap <dur>`, `auto_rollback
/// <bool>`. Cross-feature checks (overlap > cadence, binding to an
/// unknown profile) live in doctor.
pub(crate) fn secret_rotation_contract_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let lines: Vec<&str> = source.lines().collect();

    let mut in_registry = false;
    let mut in_rotation = false;
    for (line_index, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let leading = leading_spaces(line);
        if leading == 0 {
            in_registry = trimmed == "registry";
            in_rotation = false;
            continue;
        }
        if !in_registry {
            continue;
        }
        if leading == 2 {
            in_rotation = false;
            if let Some(rest) = trimmed.strip_prefix("secret_rotation ") {
                let name = rest.trim();
                if name.is_empty() || name.contains(char::is_whitespace) {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        line,
                        DiagnosticSeverity::ERROR,
                        "secret_rotation_contract_diagnostics",
                        "`secret_rotation` requires a single identifier name (e.g. `secret_rotation default`).",
                    ));
                } else {
                    in_rotation = true;
                }
            }
        } else if leading == 4 && in_rotation {
            if let Some(rest) = trimmed.strip_prefix("cadence ") {
                if lazuli_ir::security_duration::duration_seconds(rest.trim()).is_none() {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        line,
                        DiagnosticSeverity::ERROR,
                        "secret_rotation_contract_diagnostics",
                        "`secret_rotation cadence` expects a duration literal (e.g. `30d`, `24h`).",
                    ));
                }
            } else if let Some(rest) = trimmed.strip_prefix("overlap ") {
                if lazuli_ir::security_duration::duration_seconds(rest.trim()).is_none() {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        line,
                        DiagnosticSeverity::ERROR,
                        "secret_rotation_contract_diagnostics",
                        "`secret_rotation overlap` expects a duration literal (e.g. `24h`, `0h`).",
                    ));
                }
            } else if let Some(rest) = trimmed.strip_prefix("auto_rollback ") {
                let value = rest.trim();
                if !matches!(value, "true" | "false") {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        line,
                        DiagnosticSeverity::ERROR,
                        "secret_rotation_contract_diagnostics",
                        &format!(
                            "`secret_rotation auto_rollback {value}` is invalid — closed catalog is `true` or `false`."
                        ),
                    ));
                }
            } else {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::ERROR,
                    "secret_rotation_contract_diagnostics",
                    "`secret_rotation` children are `cadence <duration>`, `overlap <duration>`, or `auto_rollback <bool>`.",
                ));
            }
        }
    }

    diagnostics
}

pub(crate) fn idempotency_key_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut in_tenant_migration = false;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        let indent = line.len().saturating_sub(trimmed.len());

        if indent <= 2 {
            in_tenant_migration = indent == 2 && trimmed.starts_with("tenant_migration ");
        }

        if trimmed.starts_with("idempotency ")
            && !trimmed.starts_with("idempotency by ")
            && !in_tenant_migration
        {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "idempotency-by",
                "`idempotency` should declare its source with `by`, e.g. `idempotency by envelope.id` for event jobs or `idempotency by payload.external_id` for webhooks.",
            ));
        }
    }

    diagnostics
}
