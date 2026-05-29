//! VOCAB-CONTEXT-CTXMD-001 — feature missing or stub `<feature>.ctx.md`
//! context sidecar.
//!
//! Feature context is resolved by CONVENTION (the `attach_ctx` keyword
//! was retired): the rule probes the SINGLE co-located base
//! `<dir-of-the-.lzi>/<feature>.ctx.md` — NO project-root fallback (the
//! determinism win: one resolution base, not two). It fires when either:
//!   1. The convention file `<feature>.ctx.md` does not exist next to the
//!      `.lzi` source (the `Missing` case).
//!   2. The file exists but its content is < 100 characters after
//!      trimming whitespace — empty / whitespace-only sidecars are not
//!      documentation (the `StubContent` case).
//!
//! The `tdd-iron-hand` coverage preset escalates this rule from warn to
//! error, gating CI on every feature carrying a non-stub context
//! sidecar.  Other presets emit it as a `warning`; `off` suppresses
//! the rule entirely.
//!
//! Severity (per preset):
//!   off            — suppressed
//!   tdd-strict     — warning (informational)
//!   tdd-mature     — warning (informational)
//!   tdd-iron-hand  — error   (gates CI)
//!
//! Reference: docs/canonical-semantics.md#feature-context-vocabulary

use std::path::{Path, PathBuf};

use lazuli_ir::Feature;

/// Minimum number of non-whitespace characters required in a context
/// sidecar before VOCAB-CONTEXT-CTXMD-001 stops firing. Anything
/// shorter is treated as a stub.
pub const MIN_CTX_CHARS: usize = 100;

// ── output ────────────────────────────────────────────────────────────────────

/// Why a feature's `<feature>.ctx.md` convention sidecar failed the lint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureReason {
    /// No `<feature>.ctx.md` sidecar exists next to the feature's `.lzi`.
    Missing,
    /// The convention sidecar exists but contains fewer than
    /// `MIN_CTX_CHARS` non-trivial characters.
    StubContent {
        /// Length of trimmed contents seen by the lint.
        len: usize,
    },
}

/// One VOCAB-CONTEXT-CTXMD-001 finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file.
    pub path: PathBuf,
    /// Name of the offending feature.
    pub feature: String,
    /// Specific failure category — drives the precise diagnostic.
    pub reason: FailureReason,
    /// Resolved convention path the lint expects / inspected (the
    /// `<feature>.ctx.md` next to the `.lzi`).
    pub attempted_path: Option<PathBuf>,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "VOCAB-CONTEXT-CTXMD-001";

    /// Render the per-reason diagnostic — different prose for the
    /// missing-convention-file and stub-content cases.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::vocab::vocab_context_ctxmd_001::{Finding, FailureReason};
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("f.lzi"),
    ///     feature: "billing".into(),
    ///     reason: FailureReason::Missing,
    ///     attempted_path: Some(PathBuf::from("billing.ctx.md")),
    /// };
    /// assert!(f.message().contains("billing.ctx.md"));
    /// ```
    pub fn message(&self) -> String {
        let where_ = self
            .attempted_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "<feature>.ctx.md".into());
        match &self.reason {
            FailureReason::Missing => format!(
                "feature `{}` has no co-located `{}` context sidecar — create it (a markdown \
                 file named `<feature>.ctx.md` next to the `.lzi`) with at least {} characters \
                 of real prose to give cold readers (humans + LLMs) a richer context anchor. \
                 The convention resolves at a single base (the `.lzi` directory); the \
                 `tdd-iron-hand` preset gates CI on this. See \
                 docs/canonical-semantics.md#feature-context-vocabulary.",
                self.feature, where_, MIN_CTX_CHARS
            ),
            FailureReason::StubContent { len } => format!(
                "feature `{}` has a `{}` context sidecar but it contains only {} \
                 non-whitespace characters — below the {}-char stub threshold. Expand the \
                 sidecar with real product context.",
                self.feature, where_, len, MIN_CTX_CHARS
            ),
        }
    }
}

// ── detection ─────────────────────────────────────────────────────────────────

/// Run VOCAB-CONTEXT-CTXMD-001 for one feature.
///
/// `lzi_path` is the absolute or project-relative path of the source
/// `.lzi` file. Context is resolved by CONVENTION at the SINGLE base
/// `<dir-of-the-.lzi>/<feature>.ctx.md` (no project-root fallback).
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::vocab::vocab_context_ctxmd_001::check;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower a feature");
/// let _ = check(&feature, Path::new("billing.lzi"));
/// ```
pub fn check(feature: &Feature, lzi_path: &Path) -> Vec<Finding> {
    // Single-base convention resolution: `<lzi-dir>/<feature>.ctx.md`.
    // This is the same rule the analyzer's `resolve_ctx_convention`
    // applies (the analyzer is the sole writer of `Feature.context_path`);
    // the doctor inlines it here so it carries no extra crate dependency.
    // The expected path is computed even when the file is absent so the
    // diagnostic can point the author at exactly where to create it.
    let dir = lzi_path.parent().unwrap_or_else(|| Path::new("."));
    let expected = dir.join(format!("{}.ctx.md", feature.name));

    match std::fs::read_to_string(&expected) {
        // Convention file absent / unreadable — Missing.
        Err(_) => vec![Finding {
            path: lzi_path.to_path_buf(),
            feature: feature.name.clone(),
            reason: FailureReason::Missing,
            attempted_path: Some(expected),
        }],
        // Convention file present — gate on stub-length.
        Ok(contents) => {
            let trimmed_len = trimmed_len(&contents);
            if trimmed_len < MIN_CTX_CHARS {
                vec![Finding {
                    path: lzi_path.to_path_buf(),
                    feature: feature.name.clone(),
                    reason: FailureReason::StubContent { len: trimmed_len },
                    attempted_path: Some(expected),
                }]
            } else {
                Vec::new()
            }
        }
    }
}

// ── internals ─────────────────────────────────────────────────────────────────

/// Number of non-whitespace characters in the file (LLM-tokenization
/// proxy — strips spaces, tabs, newlines, CRs so a 200-char file of
/// newlines doesn't pass).
fn trimmed_len(contents: &str) -> usize {
    contents.chars().filter(|c| !c.is_whitespace()).count()
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{Defaults, Feature, Policies};

    fn mk_feature(name: &str) -> Feature {
        Feature {
            name: name.into(),
            purpose: None,
            non_goals: vec![],
            // `context_path` is convention-resolved (analyzer is the sole
            // writer); the rule resolves `<feature>.ctx.md` from the
            // `.lzi` path directly, so this stays `None` in the fixtures.
            context_path: None,
            knowledge: None,
            defaults: Defaults::default(),
            uses: vec![],
            uses_spans: vec![],
            uses_versions: vec![],
            requirements: vec![],
            enums: vec![],
            resources: vec![],
            events: vec![],
            rules: vec![],
            policies: Policies::default(),
            errors: None,
            commands: vec![],
            apis: vec![],
            records: vec![],
            queries: vec![],
            resume_routers: vec![],
            workflows: vec![],
            jobs: vec![],
            webhooks: vec![],
            notifications: vec![],
            event_groups: vec![],
            tenant_migrations: vec![],
            translation: None,
            pollers: vec![],
            auth: None,
            surfaces: vec![],
            extensions: vec![],
            escape_routes: vec![],
            agents: vec![],
            reports: vec![],
            channels: vec![],
            caches: vec![],
            aggregates: vec![],
            mcp_servers: vec![],
            previous_names: vec![],
            synth_origins: std::collections::BTreeMap::new(),
            span_ref: None,
        }
    }

    /// Helper: create a temp dir and return (tempdir, lzi_path) where the
    /// `.lzi` file is named `<feature>.lzi` so the co-located convention
    /// sidecar resolves the way the doctor walker sees it in production.
    fn temp_setup(feature: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("tmpdir");
        let lzi = dir.path().join(format!("{feature}.lzi"));
        std::fs::write(&lzi, "feature dummy\n").expect("seed lzi");
        (dir, lzi)
    }

    #[test]
    fn missing_convention_file_fires_with_missing_reason() {
        let (_dir, lzi) = temp_setup("catalog");
        let feature = mk_feature("catalog");
        let findings = check(&feature, &lzi);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].reason, FailureReason::Missing);
        // The diagnostic names the expected convention path.
        assert!(findings[0].message().contains("catalog.ctx.md"));
        assert_eq!(Finding::CODE, "VOCAB-CONTEXT-CTXMD-001");
    }

    #[test]
    fn stub_file_under_threshold_fires() {
        let (dir, lzi) = temp_setup("catalog");
        // Co-located convention sidecar `<feature>.ctx.md`.
        std::fs::write(dir.path().join("catalog.ctx.md"), "tiny").expect("write ctx");
        let feature = mk_feature("catalog");
        let findings = check(&feature, &lzi);
        assert_eq!(findings.len(), 1);
        match findings[0].reason {
            FailureReason::StubContent { len } => assert_eq!(len, 4),
            ref other => panic!("expected StubContent, got {other:?}"),
        }
    }

    #[test]
    fn whitespace_only_file_is_treated_as_stub() {
        let (dir, lzi) = temp_setup("catalog");
        // 500 bytes of whitespace — but zero non-whitespace chars.
        std::fs::write(dir.path().join("catalog.ctx.md"), " \n \t ".repeat(100)).expect("write ctx");
        let feature = mk_feature("catalog");
        let findings = check(&feature, &lzi);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].reason, FailureReason::StubContent { len: 0 });
    }

    #[test]
    fn rich_file_above_threshold_passes() {
        let (dir, lzi) = temp_setup("catalog");
        // 150 non-whitespace characters.
        std::fs::write(dir.path().join("catalog.ctx.md"), "a".repeat(150)).expect("write ctx");
        let feature = mk_feature("catalog");
        assert!(check(&feature, &lzi).is_empty());
    }

    /// Tabled coverage — testify-style. One row per failure mode. The
    /// convention sidecar is always `<feature>.ctx.md` next to the `.lzi`
    /// (single base, no project-root fallback).
    #[test]
    fn tabled_cases() {
        let cases: &[(&str, Option<&str>, bool)] = &[
            // (label, sidecar_contents, expect_finding)
            ("missing_convention_file", None, true),
            ("stub_too_short", Some("short"), true),
            ("at_threshold_passes", Some(&"a".repeat(100)), false),
            ("above_threshold_passes", Some(&"a".repeat(500)), false),
        ];
        for (label, body, expect_finding) in cases {
            let (dir, lzi) = temp_setup(label);
            if let Some(content) = body {
                std::fs::write(dir.path().join(format!("{label}.ctx.md")), content).expect("seed");
            }
            let feature = mk_feature(label);
            let findings = check(&feature, &lzi);
            let got_finding = !findings.is_empty();
            assert_eq!(
                got_finding, *expect_finding,
                "case `{label}`: expected finding={expect_finding}, got {got_finding}",
            );
        }
    }
}
