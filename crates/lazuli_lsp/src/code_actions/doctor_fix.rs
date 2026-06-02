//! Doctor-finding → LSP code-action bridge.
//!
//! The Layer-2 doctor engine (`crate::doctor_engine`) publishes
//! `SCREAMING-KEBAB-NNN` rule-catalog findings as editor squiggles. Many
//! of those rules have a registered, mechanical fix in the shared
//! [`lazuli_fix`] crate — the same kernel `lazuli fix --apply` runs on the
//! CLI. Before this module, that fix was unreachable from the editor: the
//! `code_action` handler only knew the four hand-written file-local
//! families, and the engine dropped the fix entirely.
//!
//! ## How it works (two halves)
//!
//! 1. **Publish side** ([`fix_data_for_code`] + [`attach_fix_data`]) — when
//!    `doctor_engine::to_lsp_diagnostic` builds a finding whose code is in
//!    the [`lazuli_fix::FixRegistry`] catalogue, it stamps a small
//!    [`DoctorFixData`] envelope (`{ rule, path, line, column }`) into the
//!    diagnostic's `data` field. The data round-trips through the client
//!    untouched and comes back on the next `codeAction` request in
//!    `params.context.diagnostics`.
//!
//! 2. **Code-action side** ([`doctor_fix_code_actions`]) — for each
//!    diagnostic in the request context that carries a [`DoctorFixData`]
//!    envelope, synthesize a `quickfix` [`CodeAction`] whose `command` is
//!    `lazuli.applyFix`. The `execute_command` handler (in `backend_p1`)
//!    runs `lazuli_fix::execute(.., apply = true)` — byte-identical to
//!    `lazuli fix --apply` — then re-publishes diagnostics.
//!
//! ## Agent-first parity
//!
//! This is the single highest-leverage LSP capability per the
//! LSP-completeness audit (`docs/audits/overnight-2026-06-02/09-lsp.md`
//! gap #7): it turns *every* fixable doctor rule into a one-click in-editor
//! action with **no** new fix logic — both surfaces dispatch through the
//! one `lazuli_fix` kernel, so the CLI (`lazuli fix`) and the LSP
//! code-action stay in lockstep by construction.
//!
//! ## See also
//! * `crate::doctor_engine::to_lsp_diagnostic` — publish-side stamping.
//! * `backend_p1.rs::Backend::execute_command` — the `lazuli.applyFix`
//!   command handler that actually applies the fix.

use serde::{Deserialize, Serialize};
use tower_lsp::lsp_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, Command, Diagnostic, NumberOrString,
};

/// The command id the `applyFix` code-action invokes via
/// `workspace/executeCommand`. Registered in the server's
/// `execute_command_provider` capability and dispatched in
/// `Backend::execute_command`.
pub(crate) const APPLY_FIX_COMMAND: &str = "lazuli.applyFix";

/// Envelope stamped into a fixable doctor diagnostic's `data` field and
/// read back on the follow-up `codeAction` request. Mirrors the fields a
/// [`lazuli_fix::FixRequest`] needs so the command handler can reconstruct
/// the request without re-running the engine.
///
/// Serialized as the diagnostic's opaque `data` (JSON) — the client passes
/// it through untouched, which is exactly how LSP intends per-diagnostic
/// fix metadata to flow from `publishDiagnostics` to `codeAction`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct DoctorFixData {
    /// Marker so we only treat our own `data` envelopes as fixable; other
    /// producers may also use `data` for unrelated purposes.
    pub kind: String,
    /// Doctor rule code (selects the `lazuli_fix` action).
    pub rule: String,
    /// Absolute path to the `.lzi` / `.lzx` the fix applies to.
    pub path: String,
    /// 1-based line anchor (doctor convention).
    pub line: usize,
    /// 1-based column anchor.
    pub column: usize,
}

/// The `data.kind` discriminator value for our envelopes.
const DOCTOR_FIX_KIND: &str = "lazuli.doctorFix";

/// Build a [`DoctorFixData`] for `code` if (and only if) the shared
/// [`lazuli_fix::FixRegistry`] has a registered action for it. Returns
/// `None` for rules with no mechanical fix so non-fixable findings stay
/// plain squiggles.
///
/// Cheap: builds the default registry (a small `HashMap`) and checks
/// membership. The registry is the single source of truth for "does this
/// rule have a fix", so the LSP never drifts from `lazuli fix --list`.
pub(crate) fn fix_data_for_code(
    code: &str,
    path: &std::path::Path,
    line: usize,
    column: usize,
) -> Option<DoctorFixData> {
    if !lazuli_fix::FixRegistry::default()
        .supported_rules()
        .iter()
        .any(|rule| rule == code)
    {
        return None;
    }
    Some(DoctorFixData {
        kind: DOCTOR_FIX_KIND.to_owned(),
        rule: code.to_owned(),
        path: path.to_string_lossy().into_owned(),
        line,
        column,
    })
}

/// Stamp a [`DoctorFixData`] envelope into `diagnostic.data` when `code`
/// is fixable. No-op for non-fixable codes. Called publish-side from
/// `doctor_engine::to_lsp_diagnostic`.
pub(crate) fn attach_fix_data(
    diagnostic: &mut Diagnostic,
    code: &str,
    path: &std::path::Path,
    line: usize,
    column: usize,
) {
    if let Some(data) = fix_data_for_code(code, path, line, column) {
        diagnostic.data = serde_json::to_value(&data).ok();
    }
}

/// Try to read our [`DoctorFixData`] envelope back out of a diagnostic's
/// `data`. Returns `None` for diagnostics that don't carry one (or carry a
/// different producer's `data`), which is the common case.
fn fix_data_from_diagnostic(diagnostic: &Diagnostic) -> Option<DoctorFixData> {
    let data = diagnostic.data.as_ref()?;
    let parsed: DoctorFixData = serde_json::from_value(data.clone()).ok()?;
    if parsed.kind != DOCTOR_FIX_KIND {
        return None;
    }
    Some(parsed)
}

/// Synthesize `lazuli.applyFix` code-actions for every fixable doctor
/// diagnostic in the `codeAction` request context.
///
/// The diagnostics here are the ones the client echoes back from the
/// document's published set, filtered to the request range — so a
/// code-action only offers a fix when the cursor / selection actually
/// overlaps a fixable squiggle. Each action carries the originating
/// diagnostic (so the client can clear it) and a `Command` the editor
/// dispatches through `workspace/executeCommand`.
pub(crate) fn doctor_fix_code_actions(context_diagnostics: &[Diagnostic]) -> Vec<CodeAction> {
    context_diagnostics
        .iter()
        .filter_map(|diagnostic| {
            let data = fix_data_from_diagnostic(diagnostic)?;
            let rule = code_string(diagnostic).unwrap_or_else(|| data.rule.clone());
            let title = format!("Apply doctor fix for {rule}");
            let argument = serde_json::to_value(&data).ok()?;
            Some(CodeAction {
                title,
                kind: Some(CodeActionKind::QUICKFIX),
                diagnostics: Some(vec![diagnostic.clone()]),
                edit: None,
                command: Some(Command {
                    title: "Apply doctor fix".to_owned(),
                    command: APPLY_FIX_COMMAND.to_owned(),
                    arguments: Some(vec![argument]),
                }),
                is_preferred: Some(true),
                disabled: None,
                data: None,
            })
        })
        .collect()
}

/// Pull the string code off a diagnostic, if present (numeric codes and
/// missing codes return `None`).
fn code_string(diagnostic: &Diagnostic) -> Option<String> {
    match diagnostic.code.as_ref()? {
        NumberOrString::String(value) => Some(value.clone()),
        NumberOrString::Number(_) => None,
    }
}

/// Wrap the bare [`CodeAction`] list as the `CodeActionOrCommand` the
/// `code_action` dispatch trunk collects.
pub(crate) fn doctor_fix_code_actions_or_commands(
    context_diagnostics: &[Diagnostic],
) -> Vec<CodeActionOrCommand> {
    doctor_fix_code_actions(context_diagnostics)
        .into_iter()
        .map(CodeActionOrCommand::CodeAction)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp::lsp_types::{Position, Range};

    fn diag_with_data(code: &str, data: Option<serde_json::Value>) -> Diagnostic {
        Diagnostic {
            range: Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 0,
                    character: 1,
                },
            },
            severity: None,
            code: Some(NumberOrString::String(code.to_owned())),
            code_description: None,
            source: Some("lazuli-doctor".to_owned()),
            message: "x".to_owned(),
            related_information: None,
            tags: None,
            data,
        }
    }

    #[test]
    fn registered_rule_gets_fix_data() {
        // `TEST-MISSING-AUTHORED-001` is a built-in fix action.
        let data = fix_data_for_code(
            "TEST-MISSING-AUTHORED-001",
            std::path::Path::new("/tmp/feature.lzi"),
            12,
            3,
        );
        let data = data.expect("registered rule must yield fix data");
        assert_eq!(data.rule, "TEST-MISSING-AUTHORED-001");
        assert_eq!(data.line, 12);
        assert_eq!(data.column, 3);
        assert_eq!(data.kind, DOCTOR_FIX_KIND);
    }

    #[test]
    fn unregistered_rule_gets_no_fix_data() {
        assert!(
            fix_data_for_code(
                "REF-CROSS-FEATURE-UNKNOWN-001",
                std::path::Path::new("/tmp/feature.lzi"),
                1,
                1,
            )
            .is_none(),
            "a rule with no registered fix action must not be marked fixable"
        );
    }

    #[test]
    fn fixable_diagnostic_yields_apply_fix_command() {
        let mut diag = diag_with_data("TEST-MISSING-AUTHORED-001", None);
        attach_fix_data(
            &mut diag,
            "TEST-MISSING-AUTHORED-001",
            std::path::Path::new("/tmp/feature.lzi"),
            12,
            3,
        );
        assert!(diag.data.is_some(), "fixable diag should carry data");

        let actions = doctor_fix_code_actions(std::slice::from_ref(&diag));
        assert_eq!(actions.len(), 1, "one fixable diag -> one code action");
        let action = &actions[0];
        let command = action.command.as_ref().expect("action carries a command");
        assert_eq!(command.command, APPLY_FIX_COMMAND);
        assert!(action.title.contains("TEST-MISSING-AUTHORED-001"));
        // The command argument round-trips back into a FixRequest shape.
        let arg = &command.arguments.as_ref().expect("args")[0];
        let parsed: DoctorFixData = serde_json::from_value(arg.clone()).expect("arg parses");
        assert_eq!(parsed.rule, "TEST-MISSING-AUTHORED-001");
        assert_eq!(parsed.line, 12);
    }

    #[test]
    fn non_fixable_diagnostic_yields_no_action() {
        // Has a (non-fix) data envelope from some other producer and a
        // non-registered code — must produce no action.
        let diag = diag_with_data(
            "REF-CROSS-FEATURE-UNKNOWN-001",
            Some(serde_json::json!({"some": "other-producer-data"})),
        );
        let actions = doctor_fix_code_actions(std::slice::from_ref(&diag));
        assert!(
            actions.is_empty(),
            "a diag without our fix envelope yields no apply-fix action"
        );
    }

    #[test]
    fn diagnostic_without_data_yields_no_action() {
        let diag = diag_with_data("TEST-MISSING-AUTHORED-001", None);
        // No data attached -> not actionable even though the code is fixable
        // (the publish side is responsible for stamping data).
        let actions = doctor_fix_code_actions(std::slice::from_ref(&diag));
        assert!(actions.is_empty());
    }
}
