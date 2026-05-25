//! ERR-VOCAB-CODE-UNKNOWN — `errors <code>` outside the closed catalog.
//!
//! Severity: error. The 12-code catalog is what the runtime can classify;
//! anything outside it would never fire and the override is dead code.
//!
//! Reference: `docs/proposals/ir-error-messages-vocab.md` §6
//! ERR-VOCAB-CODE-UNKNOWN.

use std::path::{Path, PathBuf};

use lazuli_ir::{Feature, FeatureErrorMessage, SpanRef};

use super::catalogs::FRAMEWORK_ERROR_CODES;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeUnknownFinding {
    pub path: PathBuf,
    pub feature: String,
    pub code: String,
    pub span: Option<SpanRef>,
}

impl CodeUnknownFinding {
    pub const CODE: &'static str = "ERR-VOCAB-CODE-UNKNOWN";

    pub fn message(&self) -> String {
        format!(
            "error code `{}` is not in the framework catalog: `policy_denied`, \
             `validation_failed`, `tenant_mismatch`, `not_found`, `rate_limited`, `bad_request`, \
             `method_not_allowed`, `integration_error`, `unique_violation`, `foreign_key_violation`, \
             `not_null_violation`, `check_violation`. To register a new code, propose an \
             addition to `crates/lazuli/runtime/go/lazuli/error.go`.",
            self.code
        )
    }
}

pub fn check_code_unknown(feature: &Feature, path: &Path) -> Vec<CodeUnknownFinding> {
    let Some(errors) = &feature.errors else {
        return Vec::new();
    };
    let mut findings = Vec::new();
    for entry in &errors.messages {
        if !is_known_error_code(&entry.code) {
            findings.push(CodeUnknownFinding {
                path: path.to_path_buf(),
                feature: feature.name.clone(),
                code: entry.code.clone(),
                span: span_of_message(entry),
            });
        }
    }
    findings
}

fn span_of_message(entry: &FeatureErrorMessage) -> Option<SpanRef> {
    entry.span_ref.or(entry.message.span_ref)
}

fn is_known_error_code(code: &str) -> bool {
    FRAMEWORK_ERROR_CODES.iter().any(|c| *c == code)
}
