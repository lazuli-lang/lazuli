//! Diagnostics for the top-level `auth` block (passwords + sessions).
//!
//! The `auth` block is the application-wide authentication contract.
//! Today the LSP enforces the minimum security posture:
//!
//! - `auth password` must declare `algorithm <name>` so the hash
//!   contract is audit-visible.
//! - `auth password` must declare `rate_limit` so credential guessing
//!   is bounded.
//! - `auth sessions` must declare `ttl` so generated session lifetime
//!   is explicit.
//!
//! Refresh-token rotation, theft-action, identity-field linking, and
//! OAuth adapter binding are checked by `lazuli_doctor` and the
//! `auth_refresh` code-action provider — those see the full IR.

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use crate::{leading_spaces, simple_canonical_diagnostic};

#[derive(Debug, Default)]
pub(crate) struct AuthSecurityFacts {
    password_line: Option<(usize, String)>,
    password_algorithm: bool,
    password_rate_limit: bool,
    sessions_line: Option<(usize, String)>,
    sessions_ttl: bool,
}

pub(crate) fn auth_security_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut current_top: Option<&str> = None;
    let mut auth = AuthSecurityFacts::default();
    let mut auth_child: Option<&str> = None;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if leading_spaces(line) == 2 {
            if current_top == Some("auth") {
                diagnostics.extend(auth_diagnostics(std::mem::take(&mut auth)));
            }
            current_top = trimmed.split_whitespace().next();
            auth_child = None;
            continue;
        }

        if current_top != Some("auth") {
            continue;
        }

        if leading_spaces(line) == 4 {
            if trimmed == "password" {
                auth.password_line = Some((line_index, line.to_owned()));
                auth_child = Some("password");
            } else if trimmed == "sessions" {
                auth.sessions_line = Some((line_index, line.to_owned()));
                auth_child = Some("sessions");
            } else {
                auth_child = None;
            }
        } else if leading_spaces(line) == 6 {
            match auth_child {
                Some("password") => {
                    if trimmed.starts_with("algorithm ") {
                        auth.password_algorithm = true;
                    } else if trimmed.starts_with("rate_limit ") {
                        auth.password_rate_limit = true;
                    }
                }
                Some("sessions") => {
                    if trimmed.starts_with("ttl ") {
                        auth.sessions_ttl = true;
                    }
                }
                _ => {}
            }
        }
    }

    if current_top == Some("auth") {
        diagnostics.extend(auth_diagnostics(auth));
    }

    diagnostics
}

pub(crate) fn auth_diagnostics(auth: AuthSecurityFacts) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    if let Some((line_index, line)) = auth.password_line {
        if !auth.password_algorithm {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                &line,
                DiagnosticSeverity::WARNING,
                "auth-password-algorithm",
                "`auth password` must declare `algorithm <name>` so the password hash contract is audit-visible.",
            ));
        }
        if !auth.password_rate_limit {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                &line,
                DiagnosticSeverity::WARNING,
                "auth-password-rate-limit",
                "`auth password` must declare a `rate_limit` for credential guessing protection.",
            ));
        }
    }

    if let Some((line_index, line)) = auth.sessions_line
        && !auth.sessions_ttl
    {
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            &line,
            DiagnosticSeverity::WARNING,
            "auth-session-ttl",
            "`auth sessions` must declare `ttl` so generated session lifetime is explicit.",
        ));
    }

    diagnostics
}
