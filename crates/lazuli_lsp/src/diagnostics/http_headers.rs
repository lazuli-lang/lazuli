//! Diagnostics for HTTP request/response shape declarations:
//! `app.cors` and `app.headers` blocks.
//!
//! Both belong to the `app.lzi` operational contract. CORS owns the
//! cross-origin policy surface (`allow_origins`, `allow_credentials`,
//! `max_age`); headers owns the security-response-header surface
//! (`csp`, `hsts`, `x_frame_options`, `x_content_type_options`,
//! `referrer_policy`, `permissions_policy`). Cross-feature checks
//! (origin documented in `urls`, env declared, prod-profile
//! completeness, credentials/wildcard conflict) live in
//! `lazuli_doctor`.

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use crate::{leading_spaces, simple_canonical_diagnostic};

/// Cut A.11 — file-local shape checks on the `cors` block in
/// `app.lzi`. Validates `allow_origins` has an env + at least one
/// quoted origin, and `allow_credentials` is `true`/`false`. The
/// cross-feature checks (origin documented in `urls`, environment
/// declared, credentials/wildcard conflict) live in doctor.
pub(crate) fn cors_contract_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let lines: Vec<&str> = source.lines().collect();

    let mut in_cors = false;
    for (line_index, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let leading = leading_spaces(line);

        if leading == 2 {
            in_cors = trimmed == "cors";
            continue;
        }
        if !in_cors || leading != 4 {
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("allow_origins ") {
            // `<env> "<origin>"[, "<origin>"]+`
            let (env, body) = match rest.split_once(char::is_whitespace) {
                Some(pair) => pair,
                None => {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        line,
                        DiagnosticSeverity::ERROR,
                        "cors_contract_diagnostics",
                        "`cors allow_origins` needs an environment and at least one quoted origin: `allow_origins <env> \"<origin>\"`.",
                    ));
                    continue;
                }
            };
            if env.trim().is_empty() {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::ERROR,
                    "cors_contract_diagnostics",
                    "`cors allow_origins` is missing the environment name.",
                ));
                continue;
            }
            let mut saw_origin = false;
            for raw in body.split(',') {
                let token = raw.trim();
                if token.is_empty() {
                    continue;
                }
                saw_origin = true;
                let is_wildcard = token == "\"*\"" || token == "*";
                let quoted = token.starts_with('"') && token.ends_with('"') && token.len() >= 2;
                if !quoted && !is_wildcard {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        line,
                        DiagnosticSeverity::ERROR,
                        "cors_contract_diagnostics",
                        &format!("`cors allow_origins` origin {token} must be a quoted string."),
                    ));
                }
            }
            if !saw_origin {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::ERROR,
                    "cors_contract_diagnostics",
                    "`cors allow_origins <env>` needs at least one origin after the environment.",
                ));
            }
        } else if let Some(rest) = trimmed.strip_prefix("allow_credentials ") {
            let value = rest.trim();
            if !matches!(value, "true" | "false") {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::ERROR,
                    "cors_contract_diagnostics",
                    &format!(
                        "`cors allow_credentials {value}` is invalid — closed catalog is `true` or `false`."
                    ),
                ));
            }
        } else if let Some(rest_raw) = trimmed.strip_prefix("max_age ") {
            // max_age shape is adapter-parseable; LSP just confirms
            // the token is present + quoted.
            let rest = rest_raw.trim();
            if !rest.starts_with('"') {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::ERROR,
                    "cors_contract_diagnostics",
                    "`cors max_age` requires a quoted duration string (e.g. `\"1h\"`).",
                ));
            }
        } else {
            // Unknown child of `cors`.
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::ERROR,
                "cors_contract_diagnostics",
                "`cors` children are `allow_origins`, `allow_credentials`, or `max_age`.",
            ));
        }
    }

    diagnostics
}

/// Roadmap §1.10 — file-local shape checks on the `app.headers`
/// block. Closed-catalog tokens (`referrer_policy`,
/// `x_frame_options`, `x_content_type_options`) and basic HSTS
/// shape gate here so the editor catches typos at the keystroke.
/// Cross-feature checks (production-profile completeness) live in
/// doctor's `headers-contract`.
pub(crate) fn headers_contract_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let lines: Vec<&str> = source.lines().collect();

    let mut in_headers = false;
    let mut in_hsts_body = false;
    for (line_index, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let leading = leading_spaces(line);

        if leading == 0 {
            in_headers = false;
            in_hsts_body = false;
            continue;
        }
        if leading == 2 {
            in_headers = trimmed == "headers";
            in_hsts_body = false;
            continue;
        }
        if !in_headers {
            continue;
        }
        if leading == 4 {
            // Reset HSTS body tracking on every indent-4 line; if
            // the line is `hsts ...` it opens a new body, otherwise
            // we close it.
            in_hsts_body = false;
            if let Some(rest) = trimmed.strip_prefix("x_content_type_options ") {
                let value = rest.trim();
                if !lazuli_ir::AppHeaders::is_x_content_type_options_known(value) {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        line,
                        DiagnosticSeverity::ERROR,
                        "headers_contract_diagnostics",
                        &format!(
                            "`app.headers x_content_type_options {value}` is invalid — the only legal token is `nosniff`."
                        ),
                    ));
                }
            } else if let Some(rest) = trimmed.strip_prefix("x_frame_options ") {
                let value = rest.trim();
                if !lazuli_ir::AppHeaders::is_x_frame_options_known(value) {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        line,
                        DiagnosticSeverity::ERROR,
                        "headers_contract_diagnostics",
                        &format!(
                            "`app.headers x_frame_options {value}` is invalid — closed catalog is `DENY`, `SAMEORIGIN`, or `ALLOW-FROM <uri>`."
                        ),
                    ));
                }
            } else if let Some(rest) = trimmed.strip_prefix("referrer_policy ") {
                let value = rest.trim();
                if !lazuli_ir::AppHeaders::is_referrer_policy_known(value) {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        line,
                        DiagnosticSeverity::ERROR,
                        "headers_contract_diagnostics",
                        &format!(
                            "`app.headers referrer_policy {value}` is invalid — closed catalog is `{}`.",
                            lazuli_ir::AppHeaders::REFERRER_POLICY_CATALOG.join(", "),
                        ),
                    ));
                }
            } else if let Some(rest_raw) = trimmed.strip_prefix("csp ") {
                let rest = rest_raw.trim();
                if !rest.starts_with('"') {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        line,
                        DiagnosticSeverity::ERROR,
                        "headers_contract_diagnostics",
                        "`app.headers csp` requires a quoted policy string (e.g. `csp \"default-src 'self'\"`).",
                    ));
                }
            } else if let Some(rest_raw) = trimmed.strip_prefix("permissions_policy ") {
                let rest = rest_raw.trim();
                if !rest.starts_with('"') {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        line,
                        DiagnosticSeverity::ERROR,
                        "headers_contract_diagnostics",
                        "`app.headers permissions_policy` requires a quoted policy string.",
                    ));
                }
            } else if trimmed == "hsts" || trimmed.starts_with("hsts ") {
                in_hsts_body = true;
            } else {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::ERROR,
                    "headers_contract_diagnostics",
                    "`app.headers` children are `csp`, `hsts`, `x_frame_options`, `x_content_type_options`, `referrer_policy`, or `permissions_policy`.",
                ));
            }
        } else if leading == 6 && in_hsts_body {
            // HSTS body: closed catalog is `max_age <n>`,
            // `include_subdomains`, `preload`.
            if let Some(rest) = trimmed.strip_prefix("max_age ") {
                if rest.trim().parse::<u64>().is_err() {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        line,
                        DiagnosticSeverity::ERROR,
                        "headers_contract_diagnostics",
                        "`app.headers hsts max_age` expects a non-negative integer (seconds).",
                    ));
                }
            } else if trimmed != "include_subdomains" && trimmed != "preload" {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::ERROR,
                    "headers_contract_diagnostics",
                    "`app.headers hsts` children are `max_age <n>`, `include_subdomains`, or `preload`.",
                ));
            }
        }
    }

    diagnostics
}
