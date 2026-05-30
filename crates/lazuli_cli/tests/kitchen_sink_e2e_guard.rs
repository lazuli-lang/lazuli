//! Guard A — end-to-end enforcement that the canonical `full-capsule`
//! fixture exercises EVERY cycle primitive AND that every primitive is
//! end-to-end usable: it passes BOTH `lazuli check` AND `lazuli doctor`
//! with no error-severity rejection of the primitive's own diagnostic
//! codes.
//!
//! This catches the "declared-but-inert / passes-check-but-fails-doctor"
//! drift class (the F1–F5 regressions) so it cannot silently reopen. A
//! primitive that the parser accepts (check passes) but the analyzer /
//! doctor pipeline rejects (doctor fails) — or vice versa — would flip
//! one of the two exit-code assertions and fail the build.
//!
//! The guard is intentionally mechanical:
//!
//! 1. `lazuli check <fixture>` MUST exit 0 (no parse / analyzer
//!    rejection of any primitive's surface syntax).
//! 2. `lazuli doctor <fixture> --format=json` MUST exit 0, the report
//!    `summary.errors` MUST be 0, `result` MUST be a pass variant, and
//!    NO error-severity finding may carry one of the primitives' own
//!    rule codes (the "passes-check-but-fails-doctor" half).
//! 3. The fixture source MUST still author each primitive's surface form
//!    (a sentinel table). Deleting / reverting any primitive from the
//!    fixture flips the corresponding sentinel and fails the build —
//!    this is what makes the guard go RED when a primitive regresses.
//!
//! See `docs/cli-exit-codes.md` for the doctor exit-code contract and
//! the `DoctorReport` JSON schema (`schema_version: 1`).
//!
//! ## Known §7a residual gaps (NOT yet end-to-end usable)
//!
//! Two §7a surface-UX primitives are intentionally absent from the
//! fixture because they are *not* end-to-end usable at HEAD — including
//! them would flip a gate, which is itself the finding:
//!
//! - `filters { <f>: date_range … }` — the typed `filters` block (and its
//!   `date_range` cardinality) parses only in the orphaned per-feature
//!   surface dialect; the experience-surface dialect that `lazuli doctor`
//!   walks (post-F5) rejects it with `LZX-PARSE` (the platform-view child
//!   catalog lists `filter`, not `filters`).
//! - `view.inline_table on_change @command.<x>` — parses in the surface
//!   dialect, but the `@command` namespace is absent from the LSP
//!   `namespace-catalog` (`lazuli check` / LSP), so the canonical `.lzx`
//!   LSP-contract test rejects it.
//!
//! Both are tracked as follow-up F-class items. When either lands, add the
//! primitive to the fixture and its sentinel below.

use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;

fn cli_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_lazuli"))
}

fn workspace_root() -> PathBuf {
    // CARGO_MANIFEST_DIR points at `crates/lazuli_cli`; the repo root is
    // the grandparent.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .map(PathBuf::from)
        .expect("workspace root")
}

fn fixture_dir() -> PathBuf {
    workspace_root().join("examples").join("full-capsule")
}

/// The cycle primitives this fixture exercises, paired with the
/// error-severity doctor rule code(s) that the corresponding declaration
/// would trip if it were authored wrong. The guard asserts none of these
/// codes appear at error severity in a clean run — so a primitive that
/// silently degrades into a rejection (the F-class drift) trips here even
/// if some *other* primitive still keeps the overall report at a pass.
///
/// Some of these codes are not yet wired into the package-doctor dispatch
/// at HEAD (e.g. the §7a `LZX-*` UX rules and several
/// `LIFECYCLE-TRANSITION-*` codes are dormant — defined + unit-tested but
/// not run against `.lzx`/`.lzi` in `lazuli doctor <dir>`). Listing them
/// here is forward-looking: the moment any is dispatched, a regression
/// that flips it to error is caught. The exit-0 + `summary.errors == 0`
/// assertions plus the presence sentinels cover the dormant ones today.
const PRIMITIVE_ERROR_CODES: &[&str] = &[
    // command-triggered lifecycle transitions
    "LIFECYCLE-TRANSITION-001",
    "LIFECYCLE-TRANSITION-002",
    "LIFECYCLE-TRANSITION-003",
    "LIFECYCLE-TRANSITION-004",
    "LIFECYCLE-TRANSITION-005",
    "LIFECYCLE-TRANSITION-006",
    // cross-feature audit materialize target
    "AUDIT-MATERIALIZE-TARGET-001",
    // cross-feature FK target
    "REF-CROSS-FEATURE-UNKNOWN-001",
    // W3/W4 computed-date + rule-driven schedule_rule
    "COMPUTED-DATE-EXPR-001",
    "SCHEDULE-RULE-001",
    // append-only resource + reorder command
    "RESOURCE-APPEND-ONLY-001",
    "REORDER-POSITION-FIELD-001",
    // §7a surface UX primitives (experience-surface dialect)
    "LZX-PARSE",
    "LZX-VIEW-MODE-001",
    "LZX-TAB-GROUP-CASE-001",
    "LZX-WIZARD-STEPS-EXPR-001",
    "LZX-TAB-VIEW-REF-001",
    "LZX-BOARD-LANES-001",
    "LZX-DATE-RANGE-001",
    "LZX-REPEATABLE-SUM-001",
];

/// Per-primitive sentinel: a substring that MUST appear in the named
/// fixture file. Reverting / deleting a primitive from the fixture drops
/// its sentinel and fails the build — this is the "goes RED on revert"
/// half of the guard, independent of whether the deletion happens to keep
/// check/doctor green.
const FIXTURE_SENTINELS: &[(&str, &str, &str)] = &[
    // (primitive label, fixture file, authored substring)
    (
        "command triggers transition",
        "full-capsule.lzi",
        "triggers transition activate",
    ),
    (
        "approval chain […] sequential",
        "full-capsule.lzi",
        "chain [@role.sales_manager, @role.admin] sequential",
    ),
    (
        "approval then escalate",
        "full-capsule.lzi",
        "then escalate",
    ),
    (
        "cross-feature target @feature",
        "full-capsule.lzi",
        "target @feature.customer_audit.OperationLog",
    ),
    (
        "audit materialize @feature",
        "full-capsule.lzi",
        "materialize @feature.customer_audit.OperationLog",
    ),
    (
        "schedule_rule from @fn(...) offset",
        "full-capsule.lzi",
        "schedule_rule from @fn.next_review_rule(renewal_date) offset reminder_lead_days",
    ),
    (
        "computed_date from … offset",
        "full-capsule.lzi",
        "computed_date from renewal_date offset reminder_lead_days",
    ),
    (
        "append_only resource",
        "full-capsule.lzi",
        "append_only",
    ),
    (
        "many_through junction",
        "full-capsule.lzi",
        "many_through CustomerAccountManager to User",
    ),
    (
        "reorder command",
        "full-capsule.lzi",
        "reorder CustomerNote by position",
    ),
    (
        "polymorphic_ref",
        "full-capsule.lzi",
        "polymorphic_ref subject_type subject_id targets [Customer, CustomerNote]",
    ),
    (
        "unique … when",
        "full-capsule.lzi",
        "unique is_default when is_default = true",
    ),
    (
        "HexColor",
        "full-capsule.lzi",
        "HexColor",
    ),
    (
        "Percentage",
        "full-capsule.lzi",
        "Percentage",
    ),
    (
        "record (shadow struct)",
        "full-capsule.lzi",
        "record Address",
    ),
    (
        "report input/source/columns",
        "full-capsule.lzi",
        "report monthly_audit",
    ),
    // §7a surface UX primitives, experience-surface dialect (F5).
    (
        "view_mode toggle",
        "full-capsule.admin.web.lzx",
        "view_mode",
    ),
    (
        "audience tabs container",
        "full-capsule.admin.web.lzx",
        "tab \"Customers\" -> view list",
    ),
    (
        "audience wizard container",
        "full-capsule.admin.web.lzx",
        "wizard customer_review steps",
    ),
    (
        "tab_group derived_from <enum>",
        "full-capsule.admin.web.lzx",
        "tab_group derived_from operation",
    ),
    (
        "wizard_steps <n> current <enum>",
        "full-capsule.admin.web.lzx",
        "wizard_steps 2 current operation",
    ),
    (
        "view.board lanes derived_from",
        "full-capsule.admin.web.lzx",
        "lanes derived_from operation",
    ),
];

fn read_fixture_file(name: &str) -> String {
    let path = fixture_dir().join(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read fixture file {}: {e}", path.display()))
}

/// `lazuli check <fixture>` MUST exit 0 — every primitive's surface syntax
/// parses + analyzes without rejection.
#[test]
fn check_exits_zero_on_kitchen_sink_fixture() {
    let output = Command::new(cli_bin())
        .args(["check", fixture_dir().to_str().expect("fixture path utf-8")])
        .output()
        .expect("run lazuli check");
    assert!(
        output.status.success(),
        "lazuli check must exit 0 on the kitchen-sink fixture — a primitive \
         no longer parses/analyzes.\nstatus: {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

/// `lazuli doctor <fixture>` MUST exit 0 with zero error-severity
/// findings, AND no error-severity finding may carry a primitive's own
/// rule code. This is the "passes-check-but-fails-doctor" half of the
/// drift guard.
#[test]
fn doctor_exits_zero_with_no_primitive_error_findings() {
    let output = Command::new(cli_bin())
        .args([
            "doctor",
            fixture_dir().to_str().expect("fixture path utf-8"),
            "--format=json",
        ])
        .output()
        .expect("run lazuli doctor");

    // Exit-code contract (docs/cli-exit-codes.md): 0 = pass (no findings
    // matched the failure gate, which defaults to `error`).
    assert!(
        output.status.success(),
        "lazuli doctor must exit 0 on the kitchen-sink fixture — a primitive \
         passes check but is rejected by doctor (the F-class drift).\nstatus: {:?}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr),
    );

    let report: Value =
        serde_json::from_slice(&output.stdout).expect("doctor --format=json emits valid JSON");

    // `result` must be a pass variant (no `fail`).
    let result = report["result"].as_str().unwrap_or("");
    assert!(
        result == "pass" || result == "pass_with_warnings",
        "doctor `result` must be a pass variant, got `{result}`"
    );

    // `summary.errors` must be exactly 0.
    let errors = report["summary"]["errors"].as_u64().unwrap_or(u64::MAX);
    assert_eq!(
        errors, 0,
        "doctor reported {errors} error-severity finding(s); the fixture must \
         carry zero errors. Findings:\n{}",
        render_error_findings(&report)
    );

    // Defense-in-depth: no error-severity finding may carry one of the
    // primitives' own codes. Catches a regression where a primitive flips
    // to error but the overall gate is (mis)configured to a softer
    // threshold.
    let offending: Vec<String> = report["findings"]
        .as_array()
        .map(|fs| {
            fs.iter()
                .filter(|f| f["severity"].as_str() == Some("error"))
                .filter(|f| {
                    let code = f["rule"].as_str().unwrap_or("");
                    PRIMITIVE_ERROR_CODES
                        .iter()
                        .any(|pc| code.eq_ignore_ascii_case(pc))
                })
                .map(|f| {
                    format!(
                        "{} :: {}",
                        f["rule"].as_str().unwrap_or("?"),
                        f["message"].as_str().unwrap_or("")
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    assert!(
        offending.is_empty(),
        "doctor produced error-severity findings carrying primitive codes \
         (a primitive regressed to passing check but failing doctor):\n{}",
        offending.join("\n")
    );
}

/// Every cycle primitive must still be AUTHORED in the fixture. This is
/// the "goes RED on revert" half: deleting a primitive drops its sentinel
/// substring and fails here even if check/doctor would otherwise stay
/// green without it.
#[test]
fn fixture_authors_every_primitive() {
    let lzi = read_fixture_file("full-capsule.lzi");
    let admin_lzx = read_fixture_file("full-capsule.admin.web.lzx");

    let mut missing: Vec<String> = Vec::new();
    for (label, file, needle) in FIXTURE_SENTINELS {
        let haystack = match *file {
            "full-capsule.lzi" => &lzi,
            "full-capsule.admin.web.lzx" => &admin_lzx,
            other => panic!("unknown fixture file in sentinel table: {other}"),
        };
        if !haystack.contains(needle) {
            missing.push(format!("{label} (expected `{needle}` in {file})"));
        }
    }
    assert!(
        missing.is_empty(),
        "the kitchen-sink fixture no longer authors these primitives \
         (revert / deletion detected):\n{}",
        missing.join("\n")
    );
}

fn render_error_findings(report: &Value) -> String {
    report["findings"]
        .as_array()
        .map(|fs| {
            fs.iter()
                .filter(|f| f["severity"].as_str() == Some("error"))
                .map(|f| {
                    format!(
                        "  {} :: {}",
                        f["rule"].as_str().unwrap_or("?"),
                        f["message"].as_str().unwrap_or("")
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}
