//! ERR-VOCAB-EXPOSE-5XX-MESSAGE — `expose client 5xx message`. Runtime
//! force-hides regardless; analyzer flags the authoring mistake so the
//! intent is corrected at the source instead of silently rewritten.
//!
//! Severity: error. 5xx errors are framework-internal and their messages
//! contain stack traces or implementation details; the wire payload
//! must use `code` (and optionally `data`).
//!
//! Reference: `docs/proposals/ir-error-messages-vocab.md` §6
//! ERR-VOCAB-EXPOSE-5XX-MESSAGE.

use std::path::{Path, PathBuf};

use lazuli_ir::{Feature, SpanRef};

/// One ERR-VOCAB-EXPOSE-5XX-MESSAGE finding — author declared
/// `expose client 5xx message` despite the runtime always hiding it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expose5xxMessageFinding {
    /// Source `.lzi` file the offending feature lives in.
    pub path: PathBuf,
    /// Feature owning the `expose` declaration.
    pub feature: String,
    /// Source span of the offending declaration for IDE squiggles.
    pub span: Option<SpanRef>,
}

impl Expose5xxMessageFinding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "ERR-VOCAB-EXPOSE-5XX-MESSAGE";

    /// Render the "5xx messages are framework-internal" message and
    /// prompt the author to switch to `expose client 5xx code`.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::error_vocab::error_vocab::rule_expose_5xx_message::Expose5xxMessageFinding;
    ///
    /// let f = Expose5xxMessageFinding {
    ///     path: PathBuf::from("f.lzi"),
    ///     feature: "billing".into(),
    ///     span: None,
    /// };
    /// assert!(f.message().contains("expose client 5xx code"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "`expose client 5xx message` (in feature `{}`) is rejected — 5xx errors are \
             framework-internal and their messages contain stack traces or implementation \
             details. Use `expose client 5xx code` (and optionally `data`) so the wire payload \
             stays safe; the server-side log still captures the full message for operators.",
            self.feature
        )
    }
}

/// Run ERR-VOCAB-EXPOSE-5XX-MESSAGE over one feature.
///
/// Returns at most one finding per feature, anchored at the `errors`
/// block's source span.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::error_vocab::error_vocab::rule_expose_5xx_message::check_expose_5xx_message;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower a feature with `errors`");
/// let _ = check_expose_5xx_message(&feature, Path::new("billing.lzi"));
/// ```
pub fn check_expose_5xx_message(feature: &Feature, path: &Path) -> Vec<Expose5xxMessageFinding> {
    let Some(errors) = &feature.errors else {
        return Vec::new();
    };
    if errors.exposure_5xx.iter().any(|f| f == "message") {
        return vec![Expose5xxMessageFinding {
            path: path.to_path_buf(),
            feature: feature.name.clone(),
            span: errors.span_ref,
        }];
    }
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finding_code_is_stable() {
        assert_eq!(
            Expose5xxMessageFinding::CODE,
            "ERR-VOCAB-EXPOSE-5XX-MESSAGE"
        );
    }

    #[test]
    fn message_steers_to_expose_code() {
        let f = Expose5xxMessageFinding {
            path: PathBuf::from("billing.lzi"),
            feature: "billing".to_owned(),
            span: None,
        };
        let msg = f.message();
        assert!(msg.contains("billing"));
        assert!(msg.contains("framework-internal"));
        assert!(msg.contains("expose client 5xx code"));
    }
}
