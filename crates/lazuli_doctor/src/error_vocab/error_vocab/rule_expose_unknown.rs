//! ERR-VOCAB-EXPOSE-UNKNOWN — unknown field token in
//! `expose client 4xx <fields>` or `expose client 5xx <fields>`.
//!
//! Severity: error. Promotes the pre-existing LSP-only shape check
//! (`valid_error_exposure_line`) into a typed doctor diagnostic now that
//! the IR carries `FeatureErrors.exposure_4xx` / `exposure_5xx` slots.
//! The LSP keeps the inline editor shape check; doctor reports the same
//! constraint with the IR-driven span.
//!
//! Reference: `docs/proposals/ir-error-messages-vocab.md` §6
//! ERR-VOCAB-EXPOSE-UNKNOWN.

use std::path::{Path, PathBuf};

use lazuli_ir::{Feature, SpanRef};

use super::catalogs::{EXPOSE_4XX_FIELDS, EXPOSE_5XX_FIELDS};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExposeUnknownFinding {
    pub path: PathBuf,
    pub feature: String,
    /// `"4xx"` | `"5xx"` — which exposure axis carried the unknown field.
    pub axis: String,
    pub field: String,
    pub span: Option<SpanRef>,
}

impl ExposeUnknownFinding {
    pub const CODE: &'static str = "ERR-VOCAB-EXPOSE-UNKNOWN";

    pub fn message(&self) -> String {
        let allowed = match self.axis.as_str() {
            "4xx" => EXPOSE_4XX_FIELDS,
            "5xx" => EXPOSE_5XX_FIELDS,
            _ => &[] as &[&str],
        };
        let allowed_render = allowed
            .iter()
            .map(|f| format!("`{}`", f))
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "`expose client {}` field `{}` must be one of: {}.",
            self.axis, self.field, allowed_render
        )
    }
}

pub fn check_expose_unknown(feature: &Feature, path: &Path) -> Vec<ExposeUnknownFinding> {
    let Some(errors) = &feature.errors else {
        return Vec::new();
    };
    let mut findings = Vec::new();
    for field in &errors.exposure_4xx {
        if !is_known_4xx_field(field) {
            findings.push(ExposeUnknownFinding {
                path: path.to_path_buf(),
                feature: feature.name.clone(),
                axis: "4xx".to_owned(),
                field: field.clone(),
                span: errors.span_ref,
            });
        }
    }
    for field in &errors.exposure_5xx {
        // `message` on 5xx is rejected by ERR-VOCAB-EXPOSE-5XX-MESSAGE
        // (a separate, more specific rule). Suppress here so the same
        // line doesn't produce two diagnostics for the same authoring
        // mistake — the 5xx-message rule has the targeted prose.
        if field == "message" {
            continue;
        }
        if !is_known_5xx_field(field) {
            findings.push(ExposeUnknownFinding {
                path: path.to_path_buf(),
                feature: feature.name.clone(),
                axis: "5xx".to_owned(),
                field: field.clone(),
                span: errors.span_ref,
            });
        }
    }
    findings
}

fn is_known_4xx_field(field: &str) -> bool {
    EXPOSE_4XX_FIELDS.iter().any(|f| *f == field)
}

fn is_known_5xx_field(field: &str) -> bool {
    EXPOSE_5XX_FIELDS.iter().any(|f| *f == field)
}
