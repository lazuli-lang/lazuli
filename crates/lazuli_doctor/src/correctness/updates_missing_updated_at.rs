//! UPDATES-MISSING-UPDATED-AT-001 — `updates <Resource>` without an
//! explicit or implicit per-row update timestamp.
//!
//! Any resource targeted by an `Updates` effect needs a durable
//! `updated_at: DateTime` column so audit trails and read models can tell
//! when the row last changed. Authors can satisfy that contract in either
//! of two ways:
//!
//! - declare `updated_at: DateTime required` directly on the resource; or
//! - enable `timestamps` on the resource, or through `defaults.timestamps`,
//!   which makes the framework auto-emit and auto-stamp `created_at` /
//!   `updated_at` on every write.
//!
//! The rule is timestamps-aware: it skips resources whose effective
//! timestamp setting is true (`resource.timestamps.unwrap_or(
//! feature.defaults.timestamps)`), because those resources receive the
//! framework-managed `updated_at` column even when it is not authored as a
//! normal field.
//!
//! Severity: warning.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use lazuli_ir::{BuiltinType, CommandEffect, Feature, QualifiedName, Resource, TypeRef};

/// One UPDATES-MISSING-UPDATED-AT-001 finding — a resource targeted
/// by `updates` commands lacks an `updated_at: DateTime` column and
/// the implicit `timestamps` slot is not enabled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Feature owning the resource.
    pub feature: String,
    /// Resource that lacks the `updated_at` column.
    pub resource: String,
    /// Source `.lzi` file the resource lives in (diagnostic anchor).
    pub path: PathBuf,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "UPDATES-MISSING-UPDATED-AT-001";

    /// Render the "per-row change timestamps unavailable for audit"
    /// message naming the resource and offering both canonical fixes.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::correctness::updates_missing_updated_at::Finding;
    ///
    /// let f = Finding {
    ///     feature: "billing".into(),
    ///     resource: "Invoice".into(),
    ///     path: PathBuf::from("billing.lzi"),
    /// };
    /// assert!(f.message().contains("updated_at"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "resource '{}.{}' is targeted by 'updates' commands but declares no \
             'updated_at: DateTime' field. Per-row change timestamps are \
             unavailable for audit. Add 'updated_at: DateTime required' to the \
             resource (the framework auto-stamps it on every UPDATE) or set \
             'timestamps' on the resource/feature defaults.",
            self.feature, self.resource
        )
    }
}

/// Run UPDATES-MISSING-UPDATED-AT-001 over one feature.
///
/// Walks every `updates X` command and emits at most one finding per
/// distinct target resource. Resources that opt into `timestamps`
/// (either explicitly or via feature defaults) are skipped — the
/// framework auto-stamps `updated_at` for those.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::correctness::updates_missing_updated_at::check;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower a feature with `updates` commands");
/// let _ = check(&feature, Path::new("billing.lzi"));
/// ```
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    let mut seen_resources = BTreeSet::new();
    let mut findings = Vec::new();

    for command in &feature.commands {
        let CommandEffect::Updates(effect) = &command.effect else {
            continue;
        };

        let resource_name = &effect.resource.name;
        if !is_local_resource_reference(&effect.resource, &feature.name) {
            continue;
        }
        if !seen_resources.insert(resource_name.clone()) {
            continue;
        }

        let Some(resource) = feature
            .resources
            .iter()
            .find(|resource| resource.name == *resource_name)
        else {
            continue;
        };

        if effective_timestamps(feature, resource) || has_updated_at_datetime(resource) {
            continue;
        }

        findings.push(Finding {
            feature: feature.name.clone(),
            resource: resource.name.clone(),
            path: path.to_path_buf(),
        });
    }

    findings
}

fn is_local_resource_reference(resource: &QualifiedName, feature_name: &str) -> bool {
    resource
        .feature
        .as_deref()
        .is_none_or(|owner| owner == feature_name)
}

fn effective_timestamps(feature: &Feature, resource: &Resource) -> bool {
    resource.timestamps.unwrap_or(feature.defaults.timestamps)
}

fn has_updated_at_datetime(resource: &Resource) -> bool {
    resource.fields.iter().any(|field| {
        field.name == "updated_at"
            && matches!(field.type_ref, TypeRef::Builtin(BuiltinType::DateTime))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lower(source: &str) -> Feature {
        let features = lazuli_syntax::parse_feature_skeletons(source).expect("parse feature");
        assert_eq!(features.len(), 1, "fixture should contain one feature");
        lazuli_analyzer::lower_feature_skeleton(&features[0]).expect("lower feature")
    }

    fn check_source(source: &str) -> Vec<Finding> {
        let feature = lower(source);
        check(&feature, Path::new("billing.lzi"))
    }

    #[test]
    fn updated_at_field_without_updates_does_not_fire() {
        let findings = check_source(
            r#"
feature billing
  domain
    resource Invoice
      id: ID required
      updated_at: DateTime required
"#,
        );

        assert!(
            findings.is_empty(),
            "expected no findings, got {findings:?}"
        );
    }

    #[test]
    fn updates_without_updated_at_and_without_timestamps_fires() {
        let findings = check_source(
            r#"
feature billing
  domain
    resource Invoice
      id: ID required

  command update_invoice
    route id: ID
    updates Invoice
"#,
        );

        assert_eq!(findings.len(), 1, "expected one finding, got {findings:?}");
        let finding = &findings[0];
        assert_eq!(finding.feature, "billing");
        assert_eq!(finding.resource, "Invoice");
        assert_eq!(Finding::CODE, "UPDATES-MISSING-UPDATED-AT-001");
        let message = finding.message();
        assert!(
            message.contains("resource 'billing.Invoice'"),
            "msg: {message}"
        );
        assert!(message.contains("updated_at: DateTime"), "msg: {message}");
        assert!(message.contains("timestamps"), "msg: {message}");
    }

    #[test]
    fn feature_default_timestamps_suppresses_warning() {
        let findings = check_source(
            r#"
feature billing
  defaults
    timestamps
  domain
    resource Invoice
      id: ID required

  command update_invoice
    route id: ID
    updates Invoice
"#,
        );

        assert!(
            findings.is_empty(),
            "expected no findings, got {findings:?}"
        );
    }

    #[test]
    fn resource_timestamps_suppresses_warning() {
        let findings = check_source(
            r#"
feature billing
  domain
    resource Invoice
      timestamps
      id: ID required

  command update_invoice
    route id: ID
    updates Invoice
"#,
        );

        assert!(
            findings.is_empty(),
            "expected no findings, got {findings:?}"
        );
    }

    #[test]
    fn resource_no_timestamps_override_of_feature_default_fires() {
        let mut feature = lower(
            r#"
feature billing
  defaults
    timestamps
  domain
    resource Invoice
      id: ID required

  command update_invoice
    route id: ID
    updates Invoice
"#,
        );
        feature.resources[0].timestamps = Some(false);

        let findings = check(&feature, Path::new("billing.lzi"));

        assert_eq!(
            findings.len(),
            1,
            "expected override finding, got {findings:?}"
        );
        assert_eq!(findings[0].resource, "Invoice");
    }

    #[test]
    fn updated_at_with_wrong_type_still_fires() {
        let findings = check_source(
            r#"
feature billing
  domain
    resource Invoice
      id: ID required
      updated_at: Text required

  command update_invoice
    route id: ID
    updates Invoice
"#,
        );

        assert_eq!(
            findings.len(),
            1,
            "expected wrong-type finding, got {findings:?}"
        );
    }

    #[test]
    fn non_updates_effects_do_not_fire() {
        let findings = check_source(
            r#"
feature billing
  domain
    resource Invoice
      id: ID required

  command create_invoice
    creates Invoice

  command delete_invoice
    route id: ID
    deletes Invoice

  command preview_invoice
    returns Text
"#,
        );

        assert!(
            findings.is_empty(),
            "expected no findings, got {findings:?}"
        );
    }

    #[test]
    fn duplicate_updates_for_same_resource_emit_one_finding() {
        let findings = check_source(
            r#"
feature billing
  domain
    resource Invoice
      id: ID required

  command update_invoice
    route id: ID
    updates Invoice

  command rename_invoice
    route id: ID
    updates Invoice
"#,
        );

        assert_eq!(
            findings.len(),
            1,
            "same resource should not duplicate diagnostics: {findings:?}"
        );
    }
}
