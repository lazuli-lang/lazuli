//! AGGREGATE-CONTAINS-UNKNOWN — aggregate's `contains` lists an
//! unknown resource.
//!
//! Fires when an `aggregate <Name>` block declares `contains <R1>,
//! <R2>, ...` and one or more entries do not resolve to a resource in
//! the same feature.
//!
//! Severity: `error` (strict), `error` (production). The contains list
//! is the closed cluster boundary; an unknown member silently widens
//! the aggregate, which defeats the consistency contract.
//!
//! Reference: spec wave-c-cl4 (roadmap §1.7 — `aggregate` kind).

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use lazuli_ir::Feature;

/// One AGGREGATE-CONTAINS-UNKNOWN finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` path that hosts the aggregate.
    pub path: PathBuf,
    /// Feature containing the aggregate.
    pub feature: String,
    /// Aggregate whose `contains` list is unresolved.
    pub aggregate: String,
    /// Bare resource name that did not resolve in the same feature.
    pub unresolved_member: String,
}

impl Finding {
    /// Stable diagnostic code used by the dispatcher and JSON output.
    pub const CODE: &'static str = "AGGREGATE-CONTAINS-UNKNOWN";

    /// Render the user-facing diagnostic body — names the unknown
    /// resource and prompts for either declaration or removal.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use std::path::PathBuf;
    /// use lazuli_doctor::domain::aggregate_contains_unknown::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("billing.lzi"),
    ///     feature: "billing".into(),
    ///     aggregate: "OrderBoundary".into(),
    ///     unresolved_member: "Ghost".into(),
    /// };
    /// assert!(f.message().contains("Ghost"));
    /// assert!(f.message().contains("OrderBoundary"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "aggregate `{}` lists `contains {}` but no resource named `{}` \
             is declared in feature `{}`. Either declare the resource or \
             remove it from the contains list.",
            self.aggregate, self.unresolved_member, self.unresolved_member, self.feature
        )
    }
}

/// Run AGGREGATE-CONTAINS-UNKNOWN over one feature.
///
/// Cross-feature `contains` entries (those with `member.feature.is_some()`)
/// are skipped — they're handled by a separate cross-feature pass.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::domain::aggregate_contains_unknown::check;
///
/// let findings = check(&feature, Path::new("billing.lzi"));
/// for f in findings {
///     eprintln!("{}: contains {} (unknown)", f.aggregate, f.unresolved_member);
/// }
/// ```
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    let resources: HashSet<&str> = feature.resources.iter().map(|r| r.name.as_str()).collect();
    let mut findings = Vec::new();

    for agg in &feature.aggregates {
        for member in &agg.contains {
            if member.feature.is_some() {
                continue; // Cross-feature deferred.
            }
            if !resources.contains(member.name.as_str()) {
                findings.push(Finding {
                    path: path.to_path_buf(),
                    feature: feature.name.clone(),
                    aggregate: agg.name.clone(),
                    unresolved_member: member.name.clone(),
                });
            }
        }
    }

    findings
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{Aggregate, Defaults, Feature, Policies, QualifiedName, Resource};

    fn mk_resource(name: &str) -> Resource {
        Resource {
            name: name.into(),
            public_contract: None,
            tenancy: None,
            soft_delete: false,
            timestamps: None,
            fields: vec![],
            constraints: vec![],
            validate: None,
            validates: vec![],
            retention: None,
            previous_names: vec![],
            span_ref: None,
            lifecycle: None,
            invariants: vec![],

            lock: None,

            composite_key: None,
            conventions: Vec::new(),
            lifecycle_routes: None,
            polymorphic_refs: Vec::new(),
            many_through: Vec::new(),
            append_only: false,
        }
    }

    fn qn(name: &str) -> QualifiedName {
        QualifiedName {
            feature: None,
            name: name.into(),
        }
    }

    fn mk_aggregate(name: &str, root: &str, contains: &[&str]) -> Aggregate {
        Aggregate {
            name: name.into(),
            root: qn(root),
            contains: contains.iter().map(|m| qn(m)).collect(),
            invariants: vec![],
            span_ref: None,
        }
    }

    fn mk_feature(resources: Vec<Resource>, aggregates: Vec<Aggregate>) -> Feature {
        Feature {
            name: "billing".into(),
            purpose: None,
            non_goals: vec![],
            context_path: None,
            knowledge: None,
            defaults: Defaults::default(),
            uses: vec![],
            uses_spans: Vec::new(),
            uses_versions: Vec::new(),
            requirements: vec![],
            enums: vec![],
            resources,
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
            aggregates,
            mcp_servers: vec![],
            previous_names: vec![],
            synth_origins: std::collections::BTreeMap::new(),
            span_ref: None,
        }
    }

    #[test]
    fn positive_unknown_member_fires() {
        let feature = mk_feature(
            vec![mk_resource("Order"), mk_resource("OrderLine")],
            vec![mk_aggregate(
                "OrderBoundary",
                "Order",
                &["OrderLine", "Ghost"],
            )],
        );
        let findings = check(&feature, Path::new("f.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].unresolved_member, "Ghost");
        assert_eq!(Finding::CODE, "AGGREGATE-CONTAINS-UNKNOWN");
    }

    #[test]
    fn negative_all_resolved_does_not_fire() {
        let feature = mk_feature(
            vec![mk_resource("Order"), mk_resource("OrderLine")],
            vec![mk_aggregate("OrderBoundary", "Order", &["OrderLine"])],
        );
        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }
}
