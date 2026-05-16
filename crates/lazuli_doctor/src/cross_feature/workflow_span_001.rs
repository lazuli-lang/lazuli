//! CROSS-FEATURE-WORKFLOW-SPAN-001 — workflow touches resources owned by
//! multiple features.
//!
//! Severity: `warning`. Gated on `architecture mode microservices`.
//! Reference: docs/proposals/cross-feature-contracts.md §3.6 + §7.
//!
//! The current `Workflow` IR keeps the bound resource in `Workflow.on` and
//! transition-local resource references as authored strings (`emits`,
//! transition names / state labels). When the retired workflow compatibility
//! shape grows an explicit write-effect list, this rule should switch the
//! transition walk to that typed slot and keep the ownership check unchanged.

use std::collections::{BTreeMap, BTreeSet};
#[allow(unused_imports)]
use std::path::PathBuf;

use lazuli_ir::{AppManifest, Module};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub feature: String,
    pub workflow: String,
    pub spanned_features: Vec<String>,
}

impl Finding {
    pub const CODE: &'static str = "CROSS-FEATURE-WORKFLOW-SPAN-001";

    pub fn message(&self) -> String {
        format!(
            "workflow `{}` in feature `{}` touches resources owned by multiple features ({}). \
             Under `architecture mode microservices`, this becomes a distributed aggregate / saga; \
             consider declaring the workflow's coordinator (the feature/service that orchestrates) and \
             treating cross-feature steps as inter-service calls. \
             See docs/proposals/cross-feature-contracts.md §3.6.",
            self.workflow,
            self.feature,
            self.spanned_features.join(", "),
        )
    }
}

pub fn check(module: &Module, app: Option<&AppManifest>) -> Vec<Finding> {
    if !is_microservices(app) {
        return Vec::new();
    }

    let mut resource_owner = BTreeMap::new();
    for feature in &module.features {
        for resource in &feature.resources {
            resource_owner.insert(resource.name.clone(), feature.name.clone());
        }
    }

    let mut findings = Vec::new();
    for feature in &module.features {
        for workflow in &feature.workflows {
            let touched_features = collect_touched_features(workflow, &resource_owner);
            if touched_features.len() >= 2 {
                findings.push(Finding {
                    feature: feature.name.clone(),
                    workflow: workflow.name.clone(),
                    spanned_features: touched_features.into_iter().collect(),
                });
            }
        }
    }
    findings
}

fn collect_touched_features(
    workflow: &lazuli_ir::Workflow,
    resource_owner: &BTreeMap<String, String>,
) -> BTreeSet<String> {
    let mut touched = BTreeSet::new();

    insert_owner(&workflow.on.resource.name, resource_owner, &mut touched);

    for transition in &workflow.transitions {
        insert_resource_mentions(&transition.name, resource_owner, &mut touched);
        insert_resource_mentions(&transition.from, resource_owner, &mut touched);
        insert_resource_mentions(&transition.to, resource_owner, &mut touched);

        if let Some(requires) = &transition.requires {
            insert_resource_mentions(requires, resource_owner, &mut touched);
        }

        for emit in &transition.emits {
            insert_resource_mentions(emit, resource_owner, &mut touched);
        }
    }

    touched
}

fn insert_owner(
    resource_name: &str,
    resource_owner: &BTreeMap<String, String>,
    touched: &mut BTreeSet<String>,
) {
    if let Some(owner) = resource_owner.get(resource_name) {
        touched.insert(owner.clone());
    }
}

fn insert_resource_mentions(
    text: &str,
    resource_owner: &BTreeMap<String, String>,
    touched: &mut BTreeSet<String>,
) {
    for token in text.split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_') {
        if token.is_empty() {
            continue;
        }
        insert_owner(token, resource_owner, touched);
    }
}

fn is_microservices(app: Option<&AppManifest>) -> bool {
    app.and_then(|app| app.architecture.as_ref())
        .and_then(|architecture| architecture.mode.as_deref())
        .is_some_and(|mode| mode == "microservices")
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        AppArchitecture, Defaults, Feature, FieldRef, Module, Policies, QualifiedName, Resource,
        Transition, Workflow,
    };

    fn qn(name: &str) -> QualifiedName {
        QualifiedName {
            feature: None,
            name: name.to_owned(),
        }
    }

    fn resource(name: &str) -> Resource {
        Resource {
            name: name.to_owned(),
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
        }
    }

    fn transition(name: &str, emits: Vec<&str>) -> Transition {
        Transition {
            name: name.to_owned(),
            from: "pending".to_owned(),
            to: "done".to_owned(),
            requires: None,
            emits: emits.into_iter().map(str::to_owned).collect(),
            tests: None,
            previous_names: vec![],
            span_ref: None,
        }
    }

    fn workflow(name: &str, resource_name: &str, transitions: Vec<Transition>) -> Workflow {
        Workflow {
            name: name.to_owned(),
            on: FieldRef {
                resource: qn(resource_name),
                field: "status".to_owned(),
            },
            default_policy: None,
            default_emits: vec![],
            transitions,
            previous_names: vec![],
            span_ref: None,
        }
    }

    fn feature(name: &str, resources: Vec<Resource>, workflows: Vec<Workflow>) -> Feature {
        Feature {
            name: name.to_owned(),
            purpose: None,
            non_goals: vec![],
            context_path: None,
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
            commands: vec![],
            apis: vec![],
            records: vec![],
            queries: vec![],
            workflows,
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
            previous_names: vec![],
            span_ref: None,
        }
    }

    fn module(features: Vec<Feature>) -> Module {
        Module {
            workspace: None,
            contracts: vec![],
            app: None,
            registry: None,
            profiles: vec![],
            design: None,
            rbac: None,
            features,
        }
    }

    fn app(mode: &str) -> AppManifest {
        AppManifest {
            name: "TestApp".to_owned(),
            title: None,
            version: None,
            lazuli_version: None,
            targets: vec![],
            default_locale: None,
            default_timezone: None,
            auth_failed_redirect: None,
            not_found: None,
            error_pages: vec![],
            uses: vec![],
            packs: vec![],
            bindings: vec![],
            architecture: Some(AppArchitecture {
                mode: Some(mode.to_owned()),
                service_ready: None,
                enforce_service_boundaries: None,
            }),
            services: vec![],
            communication: None,
            environments: vec![],
            urls: vec![],
            cors: None,
            env: vec![],
            integrations: vec![],
            capabilities: vec![],
            runtime: vec![],
            deploy: None,
            logging: None,
            tracing: None,
            observability: None,
            locale: None,
            encryption_bindings: vec![],
            cookie: None,
            proxy: None,
            limits: None,
            headers: None,
            span_ref: None,
        }
    }

    #[test]
    fn non_microservices_mode_does_not_fire() {
        let module = module(vec![
            feature(
                "host",
                vec![resource("Booking")],
                vec![workflow(
                    "checkout",
                    "Booking",
                    vec![transition("charge", vec!["updates Payment"])],
                )],
            ),
            feature("payments", vec![resource("Payment")], vec![]),
        ]);

        assert!(check(&module, Some(&app("modular_monolith"))).is_empty());
    }

    #[test]
    fn single_feature_workflow_does_not_fire() {
        let module = module(vec![feature(
            "host",
            vec![resource("Booking")],
            vec![workflow(
                "checkout",
                "Booking",
                vec![transition("confirm", vec!["updates Booking"])],
            )],
        )]);

        assert!(check(&module, Some(&app("microservices"))).is_empty());
    }

    #[test]
    fn cross_feature_workflow_emits_warning() {
        let module = module(vec![
            feature(
                "host",
                vec![resource("Booking")],
                vec![workflow(
                    "checkout",
                    "Booking",
                    vec![transition("charge", vec!["creates Payment"])],
                )],
            ),
            feature("payments", vec![resource("Payment")], vec![]),
        ]);

        let findings = check(&module, Some(&app("microservices")));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].feature, "host");
        assert_eq!(findings[0].workflow, "checkout");
        assert_eq!(findings[0].spanned_features, vec!["host", "payments"]);
        assert_eq!(Finding::CODE, "CROSS-FEATURE-WORKFLOW-SPAN-001");
    }

    #[test]
    fn three_feature_span_lists_all_features() {
        let module = module(vec![
            feature(
                "host",
                vec![resource("Booking")],
                vec![workflow(
                    "checkout",
                    "Booking",
                    vec![transition(
                        "charge_and_notify",
                        vec!["updates Payment", "creates LedgerEntry"],
                    )],
                )],
            ),
            feature("ledger", vec![resource("LedgerEntry")], vec![]),
            feature("payments", vec![resource("Payment")], vec![]),
        ]);

        let findings = check(&module, Some(&app("microservices")));
        assert_eq!(findings.len(), 1);
        assert_eq!(
            findings[0].spanned_features,
            vec!["host", "ledger", "payments"]
        );
    }
}
