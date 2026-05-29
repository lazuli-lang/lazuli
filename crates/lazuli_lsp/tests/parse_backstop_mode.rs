//! BUG 2 Part B — the CLI/batch diagnostic entry runs the real
//! parser + analyzer lower as a backstop over canonical `.lzi` sources;
//! the editor's per-keystroke entry does NOT.
//!
//! A STRICT block with no text-pattern producer (here: an unknown child of
//! a `job` block) is the confirmed exit-0 regression: the ~70 producers the
//! editor pass runs never see it, so `lazuli check` used to pass. The CLI
//! entry now surfaces it via the parser backstop. The editor entry stays
//! silent (it receives parse/lower failures from the debounced Layer-2
//! `run_package` stream instead — running the parser per keystroke would
//! double-fire and flicker).

use lazuli_lsp::{
    SecurityProfile, diagnostics_for_source_with_profile, diagnostics_for_source_with_profile_cli,
};
use tower_lsp::lsp_types::DiagnosticSeverity;
// `NumberOrString` is referenced via its fully-qualified path below.

/// A canonical feature whose `job` block carries an unknown child. No
/// text-pattern producer covers `job` children, so only the real parser
/// catches it.
const JOB_UNKNOWN_CHILD: &str = "\
feature billing
  resource Invoice
    field amount: Money

  job recompute
    trigger event billing.invoice_paid
    bogus_unknown_child foo
    tenant_from payload.org_id
";

fn error_count(diagnostics: &[tower_lsp::lsp_types::Diagnostic]) -> usize {
    diagnostics
        .iter()
        .filter(|d| d.severity == Some(DiagnosticSeverity::ERROR))
        .count()
}

#[test]
fn cli_entry_surfaces_producerless_job_parse_error() {
    let diagnostics =
        diagnostics_for_source_with_profile_cli(JOB_UNKNOWN_CHILD, SecurityProfile::Strict);
    assert!(
        error_count(&diagnostics) >= 1,
        "CLI backstop must surface the unknown `job` child as an error; got {diagnostics:?}"
    );
}

#[test]
fn editor_entry_does_not_run_parser_backstop() {
    let diagnostics =
        diagnostics_for_source_with_profile(JOB_UNKNOWN_CHILD, SecurityProfile::Strict);
    // The editor's synchronous Layer-1 pass deliberately does NOT run the
    // parser/lower backstop for canonical sources — it would double-fire
    // against the Layer-2 `run_package` stream. Since no text-pattern
    // producer covers `job` children, the editor pass reports zero errors
    // for this source. (If a future producer ever covers `job` children,
    // this asserts the editor count stays strictly below the CLI count,
    // which is the property that actually matters.)
    let editor_errors = error_count(&diagnostics);
    let cli_errors = error_count(&diagnostics_for_source_with_profile_cli(
        JOB_UNKNOWN_CHILD,
        SecurityProfile::Strict,
    ));
    assert!(
        editor_errors < cli_errors,
        "editor pass ({editor_errors}) must report fewer errors than the CLI \
         backstop ({cli_errors}) for a producer-less block — proves the \
         parser backstop is CLI-only"
    );
}

/// Dedup: a block that DOES have a text-pattern producer (`command`) must
/// not double-fire under the CLI entry. The `command-statement-unknown`
/// producer reports the typo on its line; the parser backstop sees the same
/// failure but is deduped against the producer's same-line ERROR, so no
/// extra `lazuli-syntax` / `lazuli-analyzer` backstop diagnostic lands on a
/// line a producer already claimed.
const COMMAND_UNKNOWN_CHILD: &str = "\
feature billing
  command create
    input
      amount: Money required
    policy @policy.create
    rate_limit none
      reason \"internal\"
    bogus_command_child foo
    creates Invoice
      amount = input.amount
";

#[test]
fn cli_entry_dedups_command_producer_and_backstop() {
    let diagnostics =
        diagnostics_for_source_with_profile_cli(COMMAND_UNKNOWN_CHILD, SecurityProfile::Strict);

    // Exactly one producer error names the unknown command statement.
    let producer_hits: Vec<_> = diagnostics
        .iter()
        .filter(|d| {
            d.code
                == Some(tower_lsp::lsp_types::NumberOrString::String(
                    "command-statement-unknown".to_owned(),
                ))
        })
        .collect();
    assert_eq!(
        producer_hits.len(),
        1,
        "the command-statement-unknown producer must fire exactly once; got {producer_hits:?}"
    );
    let producer_line = producer_hits[0].range.start.line;

    // The parser backstop emits its failures under the `lazuli-syntax` /
    // `lazuli-analyzer` sources. None of them may land on the producer's
    // line — that is the dedup contract (no double-fire).
    let backstop_on_producer_line = diagnostics.iter().any(|d| {
        matches!(
            d.source.as_deref(),
            Some("lazuli-syntax") | Some("lazuli-analyzer")
        ) && d.range.start.line == producer_line
    });
    assert!(
        !backstop_on_producer_line,
        "parser/lower backstop double-fired on the producer's line {producer_line}; \
         dedup_by_line failed: {diagnostics:?}"
    );
}
