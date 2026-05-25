//! VOCAB-CONTEXT-CTXMD-001 — feature missing or stub `attach_ctx` sidecar.
//!
//! Fires in any of three conditions:
//!   1. `attach_ctx` is absent (no path declared).
//!   2. The referenced file does not exist on disk relative to the
//!      `.lzi` source file's directory (or the project root, as a
//!      fallback).
//!   3. The file exists but its content is < 100 characters after
//!      trimming whitespace — empty / whitespace-only sidecars are not
//!      documentation.
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

/// Why a feature's `attach_ctx` failed the lint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureReason {
    /// No `attach_ctx` line declared on the feature.
    Missing,
    /// `attach_ctx` declared but the file is unreadable / not present.
    FileNotFound,
    /// File exists but contains fewer than `MIN_CTX_CHARS` non-trivial
    /// characters.
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
    /// Resolved path the lint attempted to read (only populated for
    /// `FileNotFound` / `StubContent`).
    pub attempted_path: Option<PathBuf>,
}

impl Finding {
    pub const CODE: &'static str = "VOCAB-CONTEXT-CTXMD-001";

    pub fn message(&self) -> String {
        match &self.reason {
            FailureReason::Missing => format!(
                "feature `{}` has no `attach_ctx` line — point at a markdown sidecar with \
                 `attach_ctx \"./ctx.md\"` to give cold readers (humans + LLMs) a richer \
                 context anchor. The `tdd-iron-hand` preset gates CI on this. See \
                 docs/canonical-semantics.md#feature-context-vocabulary.",
                self.feature
            ),
            FailureReason::FileNotFound => format!(
                "feature `{}` declares `attach_ctx` but the referenced file was not found \
                 ({}). Create the sidecar with at least {} characters of real prose.",
                self.feature,
                self.attempted_path
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "<no path>".into()),
                MIN_CTX_CHARS
            ),
            FailureReason::StubContent { len } => format!(
                "feature `{}` declares `attach_ctx` but the file ({}) contains only {} \
                 non-whitespace characters — below the {}-char stub threshold. Expand the \
                 sidecar with real product context.",
                self.feature,
                self.attempted_path
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "<no path>".into()),
                len,
                MIN_CTX_CHARS
            ),
        }
    }
}

// ── detection ─────────────────────────────────────────────────────────────────

/// Run VOCAB-CONTEXT-CTXMD-001 for one feature.
///
/// `lzi_path` is the absolute or project-relative path of the source
/// `.lzi` file. `project_root`, when supplied, is consulted as a
/// fallback resolution base when `attach_ctx` is not found relative to
/// the `.lzi` directory.
pub fn check(feature: &Feature, lzi_path: &Path, project_root: Option<&Path>) -> Vec<Finding> {
    let Some(rel) = feature.context_path.as_deref() else {
        return vec![Finding {
            path: lzi_path.to_path_buf(),
            feature: feature.name.clone(),
            reason: FailureReason::Missing,
            attempted_path: None,
        }];
    };

    let candidate = resolve_candidate(rel, lzi_path, project_root);
    match std::fs::read_to_string(&candidate) {
        Err(_) => vec![Finding {
            path: lzi_path.to_path_buf(),
            feature: feature.name.clone(),
            reason: FailureReason::FileNotFound,
            attempted_path: Some(candidate),
        }],
        Ok(contents) => {
            let trimmed_len = trimmed_len(&contents);
            if trimmed_len < MIN_CTX_CHARS {
                vec![Finding {
                    path: lzi_path.to_path_buf(),
                    feature: feature.name.clone(),
                    reason: FailureReason::StubContent { len: trimmed_len },
                    attempted_path: Some(candidate),
                }]
            } else {
                Vec::new()
            }
        }
    }
}

// ── internals ─────────────────────────────────────────────────────────────────

fn resolve_candidate(rel: &str, lzi_path: &Path, project_root: Option<&Path>) -> PathBuf {
    let base = lzi_path.parent().unwrap_or_else(|| Path::new("."));
    let candidate = base.join(rel);
    if candidate.exists() {
        return candidate;
    }
    if let Some(root) = project_root {
        let alt = root.join(rel);
        if alt.exists() {
            return alt;
        }
    }
    // Return the lzi-relative candidate so the diagnostic points at the
    // most likely intended location.
    candidate
}

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

    fn mk_feature(name: &str, context_path: Option<&str>) -> Feature {
        Feature {
            name: name.into(),
            purpose: None,
            non_goals: vec![],
            context_path: context_path.map(|s| s.to_owned()),
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

    /// Helper: create a temp dir and return (lzi_path, project_root).
    /// The `.lzi` file is written so resolution against its parent
    /// directory matches what the doctor walker sees in production.
    fn temp_setup(name: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("tmpdir");
        let lzi = dir.path().join(format!("{name}.lzi"));
        std::fs::write(&lzi, "feature dummy\n").expect("seed lzi");
        (dir, lzi)
    }

    #[test]
    fn missing_attach_ctx_fires_with_missing_reason() {
        let (_dir, lzi) = temp_setup("catalog");
        let feature = mk_feature("catalog", None);
        let findings = check(&feature, &lzi, None);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].reason, FailureReason::Missing);
        assert!(findings[0].message().contains("attach_ctx"));
        assert_eq!(Finding::CODE, "VOCAB-CONTEXT-CTXMD-001");
    }

    #[test]
    fn file_not_found_fires_with_path_in_diagnostic() {
        let (_dir, lzi) = temp_setup("catalog");
        let feature = mk_feature("catalog", Some("./missing.md"));
        let findings = check(&feature, &lzi, None);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].reason, FailureReason::FileNotFound);
        assert!(findings[0].message().contains("not found"));
    }

    #[test]
    fn stub_file_under_threshold_fires() {
        let (dir, lzi) = temp_setup("catalog");
        let ctx = dir.path().join("ctx.md");
        std::fs::write(&ctx, "tiny").expect("write ctx");
        let feature = mk_feature("catalog", Some("./ctx.md"));
        let findings = check(&feature, &lzi, None);
        assert_eq!(findings.len(), 1);
        match findings[0].reason {
            FailureReason::StubContent { len } => assert_eq!(len, 4),
            ref other => panic!("expected StubContent, got {other:?}"),
        }
    }

    #[test]
    fn whitespace_only_file_is_treated_as_stub() {
        let (dir, lzi) = temp_setup("catalog");
        let ctx = dir.path().join("ctx.md");
        // 500 bytes of whitespace — but zero non-whitespace chars.
        std::fs::write(&ctx, " \n \t ".repeat(100)).expect("write ctx");
        let feature = mk_feature("catalog", Some("./ctx.md"));
        let findings = check(&feature, &lzi, None);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].reason, FailureReason::StubContent { len: 0 });
    }

    #[test]
    fn rich_file_above_threshold_passes() {
        let (dir, lzi) = temp_setup("catalog");
        let ctx = dir.path().join("ctx.md");
        // 150 non-whitespace characters.
        let body = "a".repeat(150);
        std::fs::write(&ctx, body).expect("write ctx");
        let feature = mk_feature("catalog", Some("./ctx.md"));
        assert!(check(&feature, &lzi, None).is_empty());
    }

    #[test]
    fn resolves_via_project_root_when_lzi_relative_missing() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let sub = dir.path().join("features").join("catalog");
        std::fs::create_dir_all(&sub).expect("mkdir");
        let lzi = sub.join("catalog.lzi");
        std::fs::write(&lzi, "feature dummy\n").expect("seed lzi");
        // Sidecar lives at project root, not next to the .lzi.
        let root_ctx = dir.path().join("ctx.md");
        std::fs::write(&root_ctx, "x".repeat(200)).expect("write ctx");

        let feature = mk_feature("catalog", Some("ctx.md"));
        // Without project_root the lookup misses.
        let f1 = check(&feature, &lzi, None);
        assert_eq!(f1.len(), 1);
        assert_eq!(f1[0].reason, FailureReason::FileNotFound);

        // With project_root the lookup succeeds.
        let f2 = check(&feature, &lzi, Some(dir.path()));
        assert!(f2.is_empty(), "expected pass, got {f2:?}");
    }

    /// Tabled coverage — testify-style. One row per failure mode.
    #[test]
    fn tabled_cases() {
        let cases: &[(&str, Option<&str>, Option<&str>, bool)] = &[
            // (label, attach_ctx, sidecar_contents, expect_finding)
            ("missing_attach_ctx", None, None, true),
            ("file_not_found", Some("./absent.md"), None, true),
            ("stub_too_short", Some("./ctx.md"), Some("short"), true),
            (
                "at_threshold_passes",
                Some("./ctx.md"),
                Some(&"a".repeat(100)),
                false,
            ),
            (
                "above_threshold_passes",
                Some("./ctx.md"),
                Some(&"a".repeat(500)),
                false,
            ),
        ];
        for (label, attach, body, expect_finding) in cases {
            let (dir, lzi) = temp_setup(label);
            if let Some(content) = body {
                std::fs::write(dir.path().join("ctx.md"), content).expect("seed");
            }
            let feature = mk_feature("f", *attach);
            let findings = check(&feature, &lzi, None);
            let got_finding = !findings.is_empty();
            assert_eq!(
                got_finding, *expect_finding,
                "case `{label}`: expected finding={expect_finding}, got {got_finding}",
            );
        }
    }
}
