//! Diagnostics for the `notification <name>` declaration (Lazuli
//! multi-channel notification primitive).
//!
//! File-local checks: every `notification` block must declare
//! `channel`, `recipient`, `trigger`, `template`, and `policy`. The
//! `channel` accepts only a comma-separated subset of `email`, `push`,
//! `sms`, `in_app`. Deeper structural checks on `digest` / `throttle`
//! sub-blocks live in `tier3_notification_diagnostics` (six
//! `NOTIF-DIGEST-*` / `NOTIF-THROTTLE-*` codes) and run against the
//! typed IR — see `crates/lazuli_cli/src/doctor.rs`.

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use crate::{leading_spaces, simple_canonical_diagnostic};

pub(crate) fn notification_contract_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let lines: Vec<&str> = source.lines().collect();
    let mut index = 0;

    while index < lines.len() {
        let line = lines[index];
        let trimmed = line.trim_start();
        let leading = leading_spaces(line);

        if leading != 2 || !trimmed.starts_with("notification ") {
            index += 1;
            continue;
        }

        let header_index = index;
        let mut has_channel = false;
        let mut has_recipient = false;
        let mut has_trigger = false;
        let mut has_template = false;
        let mut has_policy = false;
        let mut bad_channel: Option<(usize, String)> = None;
        let allowed_channels = ["email", "push", "sms", "in_app"];

        index += 1;
        while index < lines.len() {
            let inner = lines[index];
            let inner_trimmed = inner.trim_start();
            let inner_leading = leading_spaces(inner);

            if inner_trimmed.is_empty() || inner_trimmed.starts_with('#') {
                index += 1;
                continue;
            }
            if inner_leading <= 2 {
                break;
            }
            if inner_leading == 4 {
                if let Some(rest) = inner_trimmed.strip_prefix("channel ") {
                    has_channel = true;
                    for ch in rest.split(',').map(|c| c.trim()) {
                        if !allowed_channels.contains(&ch) {
                            bad_channel = Some((index, inner.to_owned()));
                        }
                    }
                } else if inner_trimmed.starts_with("recipient ") {
                    has_recipient = true;
                } else if inner_trimmed.starts_with("trigger ") {
                    has_trigger = true;
                } else if inner_trimmed.starts_with("template ") {
                    has_template = true;
                } else if inner_trimmed.starts_with("policy ") {
                    has_policy = true;
                }
                // Notifications expanded bucket cycle — `digest` /
                // `throttle` sub-blocks are recognised here only so
                // the file-local diagnostic does not flag the
                // headers as malformed children. The structural
                // checks live in `tier3_notification_diagnostics`
                // (six `NOTIF-DIGEST-*` / `NOTIF-THROTTLE-*` codes)
                // and run against the typed IR.
            }
            index += 1;
        }

        for (label, present) in [
            ("`channel <email|push|sms|in_app>[, ...]`", has_channel),
            ("`recipient <expression>`", has_recipient),
            ("`trigger event <pattern>`", has_trigger),
            ("`template \"./path\"`", has_template),
            ("`policy @policy.<name>`", has_policy),
        ] {
            if !present {
                diagnostics.push(simple_canonical_diagnostic(
                    header_index,
                    lines[header_index],
                    DiagnosticSeverity::ERROR,
                    "notification-contract",
                    &format!("`notification` declarations must declare {label}."),
                ));
            }
        }

        if let Some((idx, owned_line)) = bad_channel {
            diagnostics.push(simple_canonical_diagnostic(
                idx,
                &owned_line,
                DiagnosticSeverity::ERROR,
                "notification-contract",
                "`channel` accepts a comma-separated subset of `email`, `push`, `sms`, `in_app`.",
            ));
        }
    }

    diagnostics
}
