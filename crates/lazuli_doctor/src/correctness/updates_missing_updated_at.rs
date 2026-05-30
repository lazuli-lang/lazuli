//! UPDATES-MISSING-UPDATED-AT-001 — resource targeted by an `Updates`
//! command lacks an `updated_at: DateTime` audit-trail field AND has
//! `timestamps` opted out (no implicit framework stamping either).
//!
//! Codegen-correctness cycle 2 cell RU3. The framework auto-stamps
//! `updated_at = now()` on every row UPDATE when the resource
//! (effectively) carries `timestamps`. Effective timestamps =
//! `Resource.timestamps.unwrap_or(feature.defaults.timestamps)`.
//! When the effective value is `false` AND no manual
//! `updated_at: DateTime` field is declared, per-row change time is
//! unrecoverable for audit/replication/cache purposes.
//!
//! Sibling cell RU1 makes the runtime gracefully omit the
//! `updated_at = NOW()` append in that case rather than emit a SQL
//! column reference that does not exist; this diagnostic nudges the
//! author to either:
//!
//!   * Add `updated_at: DateTime required` to the resource, or
//!   * Restore feature/resource `timestamps` so the auto-stamp path
//!     kicks back in, or
//!   * Document why row-level change time isn't tracked (ignoring
//!     the warning IS the documentation — the author's intent
//!     surfaces in `lazuli doctor` output).
//!
//! Severity is `warning` rather than `error`: not every domain
//! needs row-level audit (lookup tables, derived materializations,
//! etc.), and the runtime degrades gracefully.
//!
//! Diagnostic ID: `@correctness.updates_missing_updated_at`
//! (catalog code `UPDATES-MISSING-UPDATED-AT-001`).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use lazuli_ir::{
    BuiltinType, Command, CommandEffect, Defaults, Feature, QualifiedName, Resource, TypeRef,
};

/// One missing `updated_at` finding, keyed by the targeted resource.
/// Reported once per resource per feature even if multiple `updates`
/// commands target it — the fix is a single field addition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub path: PathBuf,
    pub feature: String,
    /// Resource being updated. The diagnostic message anchors to
    /// `{feature}.{resource}` so the author can locate it across
    /// multi-feature packages.
    pub resource: String,
}

impl Finding {
    pub const CODE: &'static str = "UPDATES-MISSING-UPDATED-AT-001";
    pub const ID: &'static str = "@correctness.updates_missing_updated_at";

    pub fn message(&self) -> String {
        format!(
            "resource '{}.{}' is targeted by 'updates' commands but declares no \
             'updated_at: DateTime' field. Per-row change timestamps are \
             unavailable for audit. Add 'updated_at: DateTime required' to \
             the resource (the framework auto-stamps it on every UPDATE) or \
             document why row-level change time isn't tracked.",
            self.feature, self.resource
        )
    }
}

/// File-local single-feature check. Looks only at the feature's own
/// resources — used by the LSP for live squiggles where neighbor
/// features are not loaded. Cross-feature `updates Other.Customer`
/// effects produce no diagnostic here (the owning feature will surface
/// it on its own pass).
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    let no_neighbors: [&Feature; 0] = [];
    check_with_neighbors(feature, path, no_neighbors.iter().copied())
}

/// Multi-feature check. Walks `feature.commands` for every `Updates`
/// effect, locates the target resource (either in `feature` or any
/// `neighbors`), and emits a finding when no `updated_at: DateTime`
/// field is declared AND the resource's effective `timestamps` flag
/// is `false` (so the framework would not auto-stamp). De-duplicates
/// by (resource_feature, resource_name) so multiple `updates X`
/// commands against the same resource produce at most one diagnostic.
pub fn check_with_neighbors<'a, I>(feature: &Feature, path: &Path, neighbors: I) -> Vec<Finding>
where
    I: IntoIterator<Item = &'a Feature>,
{
    let neighbor_vec: Vec<&Feature> = neighbors.into_iter().collect();
    let resource_lookup = |feat_name: &str| -> Option<(&[Resource], &Defaults)> {
        if feat_name == feature.name {
            return Some((feature.resources.as_slice(), &feature.defaults));
        }
        neighbor_vec
            .iter()
            .find(|n| n.name == feat_name)
            .map(|n| (n.resources.as_slice(), &n.defaults))
    };

    check_inner(&feature.name, &feature.commands, resource_lookup, path)
}

/// Doctor-side dispatch entry point. Takes pre-extracted `(feature_name,
/// commands, resources, defaults)` slices plus the cross-feature
/// `(feature_name, resources, defaults)` map — matches the
/// `Tier3FeatureFacts` shape so the CLI doctor can call this without
/// re-materializing whole `Feature` values. `Defaults` carries the
/// feature-level `timestamps` toggle the diagnostic needs to decide
/// whether the framework would auto-stamp.
pub fn check_from_facts(
    feature_name: &str,
    commands: &[Command],
    self_resources: &[Resource],
    self_defaults: &Defaults,
    neighbor_resources: &[(String, &[Resource], &Defaults)],
    path: &Path,
) -> Vec<Finding> {
    let resource_lookup = |feat_name: &str| -> Option<(&[Resource], &Defaults)> {
        if feat_name == feature_name {
            return Some((self_resources, self_defaults));
        }
        neighbor_resources
            .iter()
            .find(|(name, _, _)| name == feat_name)
            .map(|(_, r, d)| (*r, *d))
    };

    check_inner(feature_name, commands, resource_lookup, path)
}

fn check_inner<'a, F>(
    feature_name: &str,
    commands: &[Command],
    resource_lookup: F,
    path: &Path,
) -> Vec<Finding>
where
    F: Fn(&str) -> Option<(&'a [Resource], &'a Defaults)>,
{
    let mut seen: BTreeSet<(String, String)> = BTreeSet::new();
    let mut findings = Vec::new();

    for command in commands {
        let CommandEffect::Updates(effect) = &command.effect else {
            continue;
        };
        let resource_ref = &effect.resource;
        let resource_feature = resolved_feature(resource_ref, feature_name);
        let key = (resource_feature.clone(), resource_ref.name.clone());
        if !seen.insert(key) {
            continue;
        }

        let Some((resources, defaults)) = resource_lookup(&resource_feature) else {
            // Resource is declared in a feature we don't have facts for.
            // Skip rather than emit a noisy false-positive — the owning
            // feature's own doctor pass will catch it.
            continue;
        };

        let Some(resource) = resources.iter().find(|r| r.name == resource_ref.name) else {
            // Resource name doesn't resolve. A different diagnostic
            // (`channel_payload_unresolved_001`-style) surfaces the
            // dangling reference; we stay silent here.
            continue;
        };

        if has_updated_at_datetime(resource) {
            continue;
        }
        if effective_timestamps(resource, defaults) {
            // Framework auto-stamps `updated_at` for this resource —
            // the audit signal is preserved even though the source
            // doesn't spell the field. No diagnostic.
            continue;
        }

        findings.push(Finding {
            path: path.to_path_buf(),
            feature: resource_feature,
            resource: resource_ref.name.clone(),
        });
    }

    findings
}

fn resolved_feature(qn: &QualifiedName, default_feature: &str) -> String {
    qn.feature
        .clone()
        .unwrap_or_else(|| default_feature.to_owned())
}

/// `true` when the resource declares any field named `updated_at` with
/// builtin type `DateTime`. We deliberately do not accept other types
/// (Date, Text, Integer): an audit timestamp must carry instant
/// precision. The framework's auto-stamp path emits `time.Now()` (Go
/// `time.Time` -> `timestamptz`), which is the same shape as
/// `BuiltinType::DateTime`.
fn has_updated_at_datetime(resource: &Resource) -> bool {
    resource.fields.iter().any(|f| {
        f.name == "updated_at" && matches!(f.type_ref, TypeRef::Builtin(BuiltinType::DateTime))
    })
}

/// Resolve whether the framework auto-stamps `updated_at` for this
/// resource. Per `Resource.timestamps` doc: `Some(true)` = explicit
/// opt-in, `Some(false)` = explicit `no_timestamps` opt-out, `None`
/// = inherit feature `defaults`.
fn effective_timestamps(resource: &Resource, defaults: &Defaults) -> bool {
    resource.timestamps.unwrap_or(defaults.timestamps)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lower(source: &str) -> Feature {
        let features = lazuli_syntax::parse_feature_skeletons(source).expect("parse feature");
        lazuli_analyzer::lower_feature_skeleton(&features[0]).expect("lower feature")
    }

    #[test]
    fn resource_with_updated_at_does_not_fire() {
        // Resource opts out of framework timestamps but declares the
        // field manually — author-owned audit. No diagnostic.
        let feature = lower(
            r#"
feature billing
  domain
    resource Customer
      id: ID required
      updated_at: DateTime required

  command update_customer
    route id: ID
    updates Customer
"#,
        );

        let findings = check(&feature, Path::new("billing.lzi"));
        assert!(
            findings.is_empty(),
            "expected no diagnostic when updated_at: DateTime exists, got: {:?}",
            findings
        );
    }

    #[test]
    fn resource_without_updated_at_fires_warning() {
        // `no_timestamps` opt-out + no manual `updated_at` field +
        // `Updates` command = audit gap. Warning fires.
        let feature = lower(
            r#"
feature billing
  domain
    resource Customer
      id: ID required

  command update_customer
    route id: ID
    updates Customer
"#,
        );

        let findings = check(&feature, Path::new("billing.lzi"));
        assert_eq!(
            findings.len(),
            1,
            "expected one finding, got: {:?}",
            findings
        );
        let f = &findings[0];
        assert_eq!(f.feature, "billing");
        assert_eq!(f.resource, "Customer");
        assert_eq!(Finding::CODE, "UPDATES-MISSING-UPDATED-AT-001");
        assert_eq!(Finding::ID, "@correctness.updates_missing_updated_at");
        let msg = f.message();
        assert!(msg.contains("resource 'billing.Customer'"), "msg: {msg}");
        assert!(msg.contains("updated_at: DateTime"), "msg: {msg}");
        assert!(msg.contains("auto-stamps"), "msg: {msg}");
    }

    #[test]
    fn resource_without_updated_at_but_no_updates_command_stays_silent() {
        let feature = lower(
            r#"
feature billing
  domain
    resource Customer
      id: ID required

  command create_customer
    creates Customer
"#,
        );

        let findings = check(&feature, Path::new("billing.lzi"));
        assert!(
            findings.is_empty(),
            "Creates effect without updated_at must not fire: {:?}",
            findings
        );
    }

    #[test]
    fn timestamps_enabled_suppresses_the_warning() {
        // Resource (or feature default) carries `timestamps`. The
        // framework auto-stamps `updated_at = NOW()` on UPDATE even
        // though no field is spelled in source — audit signal is
        // preserved. The diagnostic must stay silent.
        let feature = lower(
            r#"
feature billing
  defaults
    timestamps

  domain
    resource Customer
      id: ID required

  command update_customer
    route id: ID
    updates Customer
"#,
        );

        let findings = check(&feature, Path::new("billing.lzi"));
        assert!(
            findings.is_empty(),
            "feature defaults.timestamps must suppress: {:?}",
            findings
        );
    }

    #[test]
    fn multiple_updates_against_same_resource_emit_one_finding() {
        let feature = lower(
            r#"
feature billing
  domain
    resource Customer
      id: ID required

  command update_customer_name
    route id: ID
    updates Customer

  command update_customer_email
    route id: ID
    updates Customer
"#,
        );

        let findings = check(&feature, Path::new("billing.lzi"));
        assert_eq!(
            findings.len(),
            1,
            "expected dedupe by resource, got: {:?}",
            findings
        );
        assert_eq!(findings[0].resource, "Customer");
    }

    #[test]
    fn updated_at_with_non_datetime_type_still_fires() {
        // Defensive: `updated_at: Text` is not a real timestamp. The
        // framework can't auto-stamp it and the audit signal is wrong;
        // the diagnostic must still surface so the author retypes it.
        let feature = lower(
            r#"
feature billing
  domain
    resource Customer
      id: ID required
      updated_at: Text required

  command update_customer
    route id: ID
    updates Customer
"#,
        );

        let findings = check(&feature, Path::new("billing.lzi"));
        assert_eq!(findings.len(), 1, "non-DateTime updated_at must still fire");
    }

    #[test]
    fn delete_command_does_not_trigger() {
        // The diagnostic targets `Updates` effects only. `Deletes` removes
        // the row entirely, so a row-level change timestamp is moot.
        let feature = lower(
            r#"
feature billing
  domain
    resource Customer
      id: ID required

  command delete_customer
    route id: ID
    deletes Customer
"#,
        );

        let findings = check(&feature, Path::new("billing.lzi"));
        assert!(findings.is_empty());
    }

    #[test]
    fn cross_feature_updates_resolves_against_neighbor() {
        let owner = lower(
            r#"
feature billing
  domain
    resource Customer
      id: ID required
"#,
        );

        let writer = lower(
            r#"
feature crm
  uses billing
  command update_customer
    route id: ID
    updates billing.Customer
"#,
        );

        let findings = check_with_neighbors(&writer, Path::new("crm.lzi"), [&owner]);
        assert_eq!(
            findings.len(),
            1,
            "cross-feature updates must surface the gap, got: {:?}",
            findings
        );
        assert_eq!(findings[0].feature, "billing");
        assert_eq!(findings[0].resource, "Customer");
    }
}
