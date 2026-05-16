//! AGGREGATE-ROOT-UNKNOWN — aggregate's `root` resolves to an
//! unknown resource.
//!
//! Fires when an `aggregate <Name>` block declares `root <Resource>`
//! and `<Resource>` is not a resource declared in the same feature.
//!
//! Severity: `error` (strict), `error` (production). The root is the
//! consistency-boundary anchor; pointing at a missing resource breaks
//! every downstream consumer (codegen, doctor cross-checks, inspect).
//!
//! Reference: spec wave-c-cl4 (roadmap §1.7 — `aggregate` kind).

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use lazuli_ir::Feature;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub path: PathBuf,
    pub feature: String,
    pub aggregate: String,
    pub unresolved_root: String,
}

impl Finding {
    pub const CODE: &'static str = "AGGREGATE-ROOT-UNKNOWN";

    pub fn message(&self) -> String {
        format!(
            "aggregate `{}` declares `root {}` but no resource named `{}` \
             is declared in feature `{}`. Either declare the resource or \
             point `root` at an existing one.",
            self.aggregate, self.unresolved_root, self.unresolved_root, self.feature
        )
    }
}

pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    let resources: HashSet<&str> =
        feature.resources.iter().map(|r| r.name.as_str()).collect();

    feature
        .aggregates
        .iter()
        .filter(|agg| agg.root.feature.is_none())
        .filter(|agg| !resources.contains(agg.root.name.as_str()))
        .map(|agg| Finding {
            path: path.to_path_buf(),
            feature: feature.name.clone(),
            aggregate: agg.name.clone(),
            unresolved_root: agg.root.name.clone(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        Aggregate, Defaults, EvalPredicate, Feature, Policies, QualifiedName, Resource,
    };

    fn mk_resource(name: &str) -> Resource {
        Resource {
            name: name.into(),
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
        }
    }

    fn mk_aggregate(name: &str, root: &str) -> Aggregate {
        Aggregate {
            name: name.into(),
            root: QualifiedName {
                feature: None,
                name: root.into(),
            },
            contains: vec![],
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
            defaults: Defaults::default(),
            uses: vec![],
            uses_spans: Vec::new(),
            requirements: vec![],
            enums: vec![],
            resources,
            events: vec![],
            rules: vec![],
            policies: Policies::default(),
            commands: vec![],
            apis: vec![],
            records: vec![],
            queries: vec![],
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
            previous_names: vec![],
            span_ref: None,
        }
    }

    // Silence unused import on EvalPredicate — kept for parity with
    // sibling tests that exercise full Invariant fixtures.
    #[allow(dead_code)]
    fn _ep_anchor() -> EvalPredicate {
        EvalPredicate::Unparsed("anchor".into())
    }

    #[test]
    fn positive_unknown_root_fires() {
        let feature = mk_feature(
            vec![mk_resource("Order")],
            vec![mk_aggregate("OrderBoundary", "Ghost")],
        );
        let findings = check(&feature, Path::new("features/billing/billing.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].aggregate, "OrderBoundary");
        assert_eq!(findings[0].unresolved_root, "Ghost");
        assert_eq!(Finding::CODE, "AGGREGATE-ROOT-UNKNOWN");
        assert!(findings[0].message().contains("Ghost"));
    }

    #[test]
    fn negative_known_root_does_not_fire() {
        let feature = mk_feature(
            vec![mk_resource("Order")],
            vec![mk_aggregate("OrderBoundary", "Order")],
        );
        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }
}
