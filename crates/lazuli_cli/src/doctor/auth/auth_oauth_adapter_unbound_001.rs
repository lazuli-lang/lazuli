//! auth_oauth_adapter_unbound_001 — `auth oauth <provider> adapter @adapter.<X>`
//! references an adapter that is not declared in the feature's
//! `extensions adapter <name>` list.
//!
//! Same-feature resolution only: cross-feature resolution against
//! `registry.integrations` is the integration walker's responsibility
//! in `doctor.rs::auth_diagnostics`. This module pins the local axis
//! so a unit test catches feature-level drift without spinning up the
//! whole package pipeline.
//!
//! Severity: `error`. An unbound OAuth adapter crashes the runtime
//! emitter at codegen.
//!
//! Reference: docs/proposals/auth-lowering-scope.md §"Closed-cycle criterion"
//! Reference: docs/proposals/bucket-auth-cycle.md §IR.cross-refs

use std::path::{Path, PathBuf};

use lazuli_ir::{ExtensionContract, Feature};

// ── output ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub path: PathBuf,
    pub feature: String,
    /// `oauth <provider>` identifier.
    pub provider: String,
    /// Authored `@adapter.<name>` reference (verbatim).
    pub adapter_ref: String,
}

impl Finding {
    pub const CODE: &'static str = "auth_oauth_adapter_unbound_001";

    pub fn message(&self) -> String {
        format!(
            "auth.oauth.`{}`.adapter `{}` is not declared in `extensions` of \
             feature `{}` (consider an adapter line or wiring it under \
             `registry.integrations`).",
            self.provider, self.adapter_ref, self.feature,
        )
    }
}

// ── detection ─────────────────────────────────────────────────────────────────

/// Run auth_oauth_adapter_unbound_001 on a single feature. Emits one
/// finding per OAuth provider whose adapter does NOT resolve under
/// `feature.extensions` (kind `IntegrationAdapter`). The integration
/// walker may suppress a finding by also checking
/// `registry.integrations` — that's why the message says "consider":
/// the finding is a local hint, not a verdict.
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    let Some(auth) = feature.auth.as_ref() else {
        return Vec::new();
    };
    if auth.oauth.is_empty() {
        return Vec::new();
    }

    let local_adapters: Vec<&str> = feature
        .extensions
        .iter()
        .filter(|ext| matches!(ext.contract, ExtensionContract::IntegrationAdapter { .. }))
        .map(|ext| ext.name.as_str())
        .collect();

    let mut out = Vec::new();
    for provider in &auth.oauth {
        let adapter_ref = provider.adapter.as_str();
        let local_name = adapter_ref.strip_prefix("@adapter.").unwrap_or("");
        if local_name.is_empty() || !local_adapters.contains(&local_name) {
            out.push(Finding {
                path: path.to_path_buf(),
                feature: feature.name.clone(),
                provider: provider.provider.clone(),
                adapter_ref: adapter_ref.to_owned(),
            });
        }
    }
    out
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        Auth, AuthIdentity, AuthOAuthProvider, BuiltinType, Defaults, Extension, ExtensionContract,
        Feature, FieldRef, PathRef, Policies, QualifiedName, TypeRef,
    };

    fn qn(name: &str) -> QualifiedName {
        QualifiedName {
            feature: None,
            name: name.to_owned(),
        }
    }

    fn mk_adapter_extension(name: &str) -> Extension {
        Extension {
            name: name.to_owned(),
            contract: ExtensionContract::IntegrationAdapter {
                type_arg: TypeRef::UserDefined(qn("GoogleOAuth")),
            },
            resolved_path: PathRef::convention(format!("./adapters/{name}.go")),
            previous_names: vec![],
            span_ref: None,
        }
    }

    fn mk_feature(providers: Vec<(&str, &str)>, extensions: Vec<Extension>) -> Feature {
        Feature {
            name: "customer_auth".to_owned(),
            purpose: None,
            non_goals: vec![],
            context_path: None,
            defaults: Defaults::default(),
            uses: vec![],
            uses_spans: Vec::new(),
            uses_versions: Vec::new(),
            requirements: vec![],
            enums: vec![],
            resources: vec![],
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
            channels: vec![],
            caches: vec![],
            aggregates: vec![],
            mcp_servers: vec![],
            auth: Some(Auth {
                identity: AuthIdentity {
                    field: FieldRef {
                        resource: qn("Customer"),
                        field: "email".to_owned(),
                    },
                    public_contract: None,
                },
                password: None,
                sessions: None,
                mfa: None,
                oauth: providers
                    .into_iter()
                    .map(|(provider, adapter)| AuthOAuthProvider {
                        provider: provider.to_owned(),
                        adapter: adapter.to_owned(),
                    })
                    .collect(),
                span_ref: None,
            }),
            surfaces: vec![],
            extensions,
            escape_routes: vec![],
            agents: vec![],
            reports: vec![],
            previous_names: vec![],
            synth_origins: std::collections::BTreeMap::new(),
            span_ref: None,
        }
    }

    #[test]
    fn positive_fires_when_no_matching_adapter_extension() {
        let feature = mk_feature(vec![("google", "@adapter.bogus_google_oauth")], vec![]);
        let findings = check(&feature, Path::new("x.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(Finding::CODE, "auth_oauth_adapter_unbound_001");
        assert_eq!(findings[0].provider, "google");
        assert_eq!(findings[0].adapter_ref, "@adapter.bogus_google_oauth");
        assert!(findings[0].message().contains("customer_auth"));
    }

    #[test]
    fn negative_does_not_fire_when_adapter_declared_in_feature() {
        let feature = mk_feature(
            vec![("google", "@adapter.google_oauth")],
            vec![mk_adapter_extension("google_oauth")],
        );
        let findings = check(&feature, Path::new("x.lzi"));
        assert!(findings.is_empty(), "got: {findings:?}");
        // Sanity: verify the BuiltinType import isn't accidentally
        // dropped (used by sibling rule modules during refactors).
        let _ = TypeRef::Builtin(BuiltinType::Text);
    }

    #[test]
    fn edge_fires_per_provider_when_multiple_providers_unbound() {
        let feature = mk_feature(
            vec![
                ("google", "@adapter.google_oauth"),
                ("github", "@adapter.github_oauth"),
            ],
            vec![mk_adapter_extension("google_oauth")],
        );
        let findings = check(&feature, Path::new("x.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].provider, "github");
        assert_eq!(findings[0].adapter_ref, "@adapter.github_oauth");
    }
}
