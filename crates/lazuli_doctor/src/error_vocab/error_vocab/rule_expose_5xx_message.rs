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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expose5xxMessageFinding {
    pub path: PathBuf,
    pub feature: String,
    pub span: Option<SpanRef>,
}

impl Expose5xxMessageFinding {
    pub const CODE: &'static str = "ERR-VOCAB-EXPOSE-5XX-MESSAGE";

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
