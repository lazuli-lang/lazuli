//! Doctor's canonical diagnostic envelope + severity machinery.
//!
//! Every doctor rule, no matter where it lives (this crate,
//! `lazuli_doctor`, the LSP bridge, per-rule sibling files under
//! `doctor/auth/`, `doctor/folder/`, …) eventually lifts its native
//! finding into one of three shapes that live in this module:
//!
//! * `DoctorDiagnostic` — the row a `lazuli doctor` invocation emits.
//! * `DoctorSeverity` — the four-value severity ladder
//!   (Error / Warning / Info / Hint).
//! * `DoctorSeverityOverride` — the parsed `Lazurite.toml`
//!   `[doctor.<category>.severity_override.<CODE>]` row.
//!
//! Severity resolution lives here too (`doctor_severity_for` +
//! `doctor_rule_severity`) so the entire "rule code -> severity"
//! pipeline is one read away from a single file. Agent-first
//! `DoctorConstruct` / `DoctorFix` / `DoctorGroup` skeletons are
//! defined here as well (populated per-rule in Wave 1+).
//!
//! Wave R7-3 extract: lifted out of `doctor/mod.rs`.

use std::path::PathBuf;

use lazuli_doctor::RuleCategory;
use lazuli_doctor_config::{ResolvedDoctorConfig, SeverityOverride, effective_severity};
use lazuli_lsp::SecurityProfile;
use tower_lsp::lsp_types::DiagnosticSeverity;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DoctorSeverity {
    Error,
    Warning,
    Info,
    Hint,
}

/// Rails-style canonical envelope for every doctor finding.
///
/// One row in `lazuli doctor`'s output — the **and only the** shape a
/// diagnostic ever takes once it crosses out of an aggregator's per-rule
/// crate (`lazuli_doctor::correctness`, `lazuli_doctor::vocab`, …) and
/// into the CLI's dispatch pipeline. Every doctor rule, no matter where
/// it lives (this crate, `lazuli_doctor`, `lazuli_lsp`'s LSP bridge,
/// per-rule sibling files under `doctor/auth/`, `doctor/folder/`, …)
/// converts its native finding into this struct so the rest of the
/// system — JSON rendering (`build_doctor_report`), text printing
/// (`print()`), coverage rollup, `--fail-on` gate, dedup — has exactly
/// one type to walk.
///
/// Field-level visibility (Wave 4.3 R2): every field is `pub(crate)`
/// so sibling submodules under `doctor/` (`aggregators/*`,
/// `auth/*`, `route_guard/*`, `lzx/*`, …) can construct the envelope
/// directly. The Wave 0.5 agent-first fields (`category`, `feature_name`,
/// `construct`, `fix`, `group`) stay `Option<>` so every existing call
/// site keeps compiling. New rules populate them when they have the
/// information.
///
/// See `docs/proposals/tdd-bdd-first-2026-05-23.md` §Wave 0.5 for the
/// agent-first envelope contract.
#[derive(Debug, Clone)]
pub(crate) struct DoctorDiagnostic {
    pub(crate) path: PathBuf,
    pub(crate) line: usize,
    pub(crate) column: usize,
    pub(crate) severity: DoctorSeverity,
    pub(crate) code: String,
    pub(crate) message: String,
    // Wave 0.5 — agent-first JSON parity fields. Additive `Option<>` so
    // existing construction sites stay compiling; new rules and Wave 1+
    // migrations populate them explicitly. See
    // `docs/proposals/tdd-bdd-first-2026-05-23.md` §Wave 0.5.
    /// Rule taxonomy bucket; resolved via `RuleCategory::from_code_prefix`
    /// when not set explicitly. `None` means "fall back to prefix
    /// derivation at consumption time".
    #[allow(dead_code)]
    pub(crate) category: Option<RuleCategory>,
    /// Feature this diagnostic anchors to, when known (`None` for
    /// project-level / cross-feature / manifest checks).
    #[allow(dead_code)]
    pub(crate) feature_name: Option<String>,
    /// Construct the diagnostic anchors to (`resource Foo`, `command
    /// bar`, `policy baz`, …). Wave 1+ populates from authoring sites.
    #[allow(dead_code)]
    pub(crate) construct: Option<DoctorConstruct>,
    /// Suggested fix, when one exists.
    #[allow(dead_code)]
    pub(crate) fix: Option<DoctorFix>,
    /// JSON rollup grouping (e.g. per-feature, per-rule). Wave 2 populates.
    #[allow(dead_code)]
    pub(crate) group: Option<DoctorGroup>,
}

/// Wave 0.5 skeleton — populated per-rule in Wave 1+.
///
/// Identifies the authoring construct a diagnostic anchors to so the
/// agent-facing JSON can surface "this fires on `resource Post.title`"
/// rather than just a line/column.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct DoctorConstruct {
    /// `resource`, `command`, `policy`, `field`, etc.
    pub kind: String,
    /// Construct name (resource name, command name, …).
    pub name: String,
}

/// Wave 0.5 skeleton — populated per-rule in Wave 1+.
///
/// Carries a CLI-applicable fix preview alongside the diagnostic so
/// agents can act on output without a second tool roundtrip. See the
/// "Founding principle: Agent-first CLI parity" section of the
/// tdd-bdd-first proposal.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct DoctorFix {
    /// Short fix label (`add tests block`, `rename field`, …).
    pub action: String,
    /// Human-readable preview of the change.
    pub preview: Option<String>,
    /// `true` if the fix can be applied without human review.
    pub auto_applicable: bool,
    /// CLI invocation that would apply the fix.
    pub cli: Option<String>,
}

/// Wave 0.5 skeleton — populated per-rule in Wave 1+.
///
/// JSON rollup grouping key so the agent-facing output can build
/// `summary.by_feature`, `summary.by_rule`, `summary.by_category`.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct DoctorGroup {
    /// Group key (feature name, rule code, category, …).
    pub key: String,
    /// What kind of group this is (`feature`, `rule`, `category`).
    pub kind: String,
}

impl DoctorDiagnostic {
    pub(super) fn from_lsp(path: PathBuf, diagnostic: &tower_lsp::lsp_types::Diagnostic) -> Self {
        let severity = match diagnostic.severity {
            Some(DiagnosticSeverity::ERROR) => DoctorSeverity::Error,
            Some(DiagnosticSeverity::WARNING) => DoctorSeverity::Warning,
            Some(DiagnosticSeverity::INFORMATION) => DoctorSeverity::Info,
            Some(DiagnosticSeverity::HINT) => DoctorSeverity::Hint,
            _ => DoctorSeverity::Warning,
        };
        let code = diagnostic
            .code
            .as_ref()
            .map(|code| match code {
                tower_lsp::lsp_types::NumberOrString::String(value) => value.clone(),
                tower_lsp::lsp_types::NumberOrString::Number(value) => value.to_string(),
            })
            .unwrap_or_else(|| "diagnostic".to_owned());

        Self {
            path,
            line: diagnostic.range.start.line as usize + 1,
            column: diagnostic.range.start.character as usize + 1,
            severity,
            code,
            message: diagnostic.message.clone(),
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        }
    }

    pub(super) fn print(&self) {
        let severity = match self.severity {
            DoctorSeverity::Error => "error",
            DoctorSeverity::Warning => "warning",
            DoctorSeverity::Info => "info",
            DoctorSeverity::Hint => "hint",
        };
        println!(
            "{}:{}:{}: {severity} [{}]: {}",
            self.path.display(),
            self.line,
            self.column,
            self.code,
            self.message
        );
    }
}

/// Per-rule severity override as authored in `Lazurite.toml`.
///
/// `[doctor.test_discipline.severity_override]` table entries lift into
/// this shape. `DOCTOR-OVERRIDE-NEEDS-REASON-001` enforces the
/// `reason` field is present and non-blank.
#[derive(Debug, Clone)]
pub struct DoctorSeverityOverride {
    /// Author-supplied severity (`warning`, `error`, `info`, `hint`).
    pub severity: String,
    /// Optional human justification. Required (non-blank) per
    /// `DOCTOR-OVERRIDE-NEEDS-REASON-001`.
    pub reason: Option<String>,
}

/// Wave 0.5 — category-aware severity resolver.
///
/// Replaces the pre-Wave-0.5 global `doctor_rule_severity()` which
/// applied a single mapping (production → error, everything else →
/// warning) to every rule. With this function in place the framework
/// can express category-specific posture, e.g. "test-discipline rules
/// are info at prototype, warning at strict, error at production"
/// independently of vocab/correctness defaults.
///
/// Resolution order:
/// 1. Per-rule TOML override (`[doctor.<category>].severity_override.<CODE>`)
///    wins absolutely.
/// 2. Category default per profile (see match below).
/// 3. Fallback to the legacy global mapping for backward compat.
///
/// `overrides` is the parsed `Lazurite.toml [doctor]` block; pass an
/// empty map when no manifest is present.
pub(crate) fn doctor_severity_for(
    code: &str,
    category: RuleCategory,
    security_profile: SecurityProfile,
    overrides: &std::collections::BTreeMap<String, DoctorSeverityOverride>,
) -> DoctorSeverity {
    // W1 — delegate to the single shared resolver in
    // `lazuli_doctor_config`. This site supplies only the profile +
    // per-rule overrides (no coverage / category preset), so
    // `effective_severity` exercises precedence levels 1 (override) and 4
    // (category default per profile) — exactly the two this function used
    // to inline. The shared resolver's level-4 match is the verbatim copy
    // of the match that lived here.
    let config = ResolvedDoctorConfig {
        profile: security_profile.into(),
        overrides: overrides
            .iter()
            .map(|(code, ov)| {
                (
                    code.clone(),
                    SeverityOverride {
                        severity: ov.severity.clone(),
                        reason: ov.reason.clone(),
                    },
                )
            })
            .collect(),
        ..ResolvedDoctorConfig::default()
    };
    // The category default always has an opinion for every profile, so
    // the resolver never returns `None` here; `base_severity` is the
    // unreachable level-4 fallback. `Warning` matches the historical
    // global default for that unreachable arm.
    effective_severity(
        code,
        lazuli_doctor::DoctorSeverity::Warning,
        category,
        &config,
    )
    .map(DoctorSeverity::from)
    .unwrap_or(DoctorSeverity::Warning)
}

/// Legacy alias — kept as a thin shim so existing call sites compile
/// unchanged. New rules SHOULD call `doctor_severity_for` directly with
/// their declared `RuleCategory`. Existing sites get the same behavior
/// they had before Wave 0.5 because `from_code_prefix` recovers the
/// category and the non-TestDiscipline branches reproduce the original
/// mapping verbatim.
pub(crate) fn doctor_rule_severity(security_profile: SecurityProfile) -> DoctorSeverity {
    doctor_severity_for(
        "",
        RuleCategory::Vocabulary,
        security_profile,
        &std::collections::BTreeMap::new(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_for_strict_vocabulary_is_warning_or_higher() {
        let severity = doctor_severity_for(
            "VOCAB-CLIENT-SRC-001",
            RuleCategory::Vocabulary,
            SecurityProfile::Strict,
            &std::collections::BTreeMap::new(),
        );
        // Severity must be at least Warning under Strict — exact value
        // depends on the closed catalog, but Hint is the wrong floor.
        assert!(matches!(
            severity,
            DoctorSeverity::Error | DoctorSeverity::Warning
        ));
    }
}
