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

/// One ERR-VOCAB-CODE-UNKNOWN finding — `errors <code>` lists an
/// identifier that isn't in the framework's closed-catalog of 12 codes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeUnknownFinding {
    /// Source `.lzi` file the offending `errors` entry lives in.
    pub path: PathBuf,
    /// Feature owning the `errors` block.
    pub feature: String,
    /// The unknown code the author wrote.
    pub code: String,
    /// Source span of the offending entry for IDE squiggles.
    pub span: Option<SpanRef>,
}

impl CodeUnknownFinding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "ERR-VOCAB-CODE-UNKNOWN";

    /// Render the "not in framework catalog" message listing the 12
    /// known codes and pointing at the runtime registration anchor.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::error_vocab::error_vocab::rule_code_unknown::CodeUnknownFinding;
    ///
    /// let f = CodeUnknownFinding {
    ///     path: PathBuf::from("f.lzi"),
    ///     feature: "billing".into(),
    ///     code: "made_up_code".into(),
    ///     span: None,
    /// };
    /// assert!(f.message().contains("framework catalog"));
    /// ```
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

/// Run ERR-VOCAB-CODE-UNKNOWN over one feature's `errors` block.
///
/// Emits one finding per unknown code; features without an `errors`
/// block are silent.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::error_vocab::error_vocab::rule_code_unknown::check_code_unknown;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower a feature with `errors`");
/// let _ = check_code_unknown(&feature, Path::new("billing.lzi"));
/// ```
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finding_code_is_stable() {
        assert_eq!(CodeUnknownFinding::CODE, "ERR-VOCAB-CODE-UNKNOWN");
    }

    #[test]
    fn message_names_offending_code_and_catalog() {
        let f = CodeUnknownFinding {
            path: PathBuf::from("billing.lzi"),
            feature: "billing".to_owned(),
            code: "made_up_code".to_owned(),
            span: None,
        };
        let msg = f.message();
        assert!(msg.contains("made_up_code"));
        assert!(msg.contains("framework catalog"));
        assert!(msg.contains("policy_denied"));
    }

    #[test]
    fn known_codes_pass_through() {
        for code in FRAMEWORK_ERROR_CODES {
            assert!(is_known_error_code(code));
        }
    }
}
