//! CROSS-FEATURE-CONTRACT-VERSION-DRIFT-001 — consumer pinned to a different
//! contract version than the origin currently publishes.
//!
//! v0 scope: SCAFFOLDED but does not fire. The trigger requires a
//! consumer-side version pin (e.g., `uses account version v1`) that does
//! not yet exist in the grammar. When that syntax lands (tracked in
//! docs/next-checklist.md), this rule populates the detection without
//! changing its public surface.
//!
//! Gated on `architecture mode microservices` regardless.

use std::path::PathBuf;

use lazuli_ir::{AppManifest, Module};

type _PathAnchor = PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub consumer_feature: String,
    pub origin_feature: String,
    pub symbol: String,
    pub consumer_version: u16,
    pub origin_version: u16,
}

impl Finding {
    pub const CODE: &'static str = "CROSS-FEATURE-CONTRACT-VERSION-DRIFT-001";

    pub fn message(&self) -> String {
        format!(
            "feature `{}` references `{}.{}` at v{} but the origin currently publishes v{}. \
             Either migrate the consumer to v{} (and update affected call sites if the bump is breaking) \
             or pin the consumer explicitly. \
             See docs/proposals/cross-feature-contracts.md §5.4.",
            self.consumer_feature,
            self.origin_feature,
            self.symbol,
            self.consumer_version,
            self.origin_version,
            self.origin_version,
        )
    }
}

pub fn check(_module: &Module, app: Option<&AppManifest>) -> Vec<Finding> {
    // Gate: only fire under microservices mode.
    let is_microservices = is_microservices(app);
    if !is_microservices {
        return Vec::new();
    }

    // v0 scope: scaffolded but does not fire.
    // When `uses <feature> version v<N>` syntax lands, populate detection here.
    Vec::new()
}

fn is_microservices(app: Option<&AppManifest>) -> bool {
    app.and_then(|app| app.architecture.as_ref())
        .and_then(|architecture| architecture.mode.as_deref())
        .is_some_and(|mode| mode == "microservices")
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{AppArchitecture, Module};

    fn empty_module() -> Module {
        Module {
            workspace: None,
            contracts: Vec::new(),
            app: None,
            registry: None,
            profiles: Vec::new(),
            design: None,
            rbac: None,
            features: Vec::new(),
        }
    }

    fn app_with_mode(mode: Option<&str>) -> AppManifest {
        AppManifest {
            name: "TestApp".into(),
            title: None,
            version: None,
            lazuli_version: None,
            targets: Vec::new(),
            default_locale: None,
            default_timezone: None,
            auth_failed_redirect: None,
            not_found: None,
            error_pages: Vec::new(),
            uses: Vec::new(),
            packs: Vec::new(),
            bindings: Vec::new(),
            architecture: Some(AppArchitecture {
                mode: mode.map(str::to_owned),
                service_ready: None,
                enforce_service_boundaries: None,
            }),
            services: Vec::new(),
            communication: None,
            environments: Vec::new(),
            urls: Vec::new(),
            cors: None,
            env: Vec::new(),
            integrations: Vec::new(),
            capabilities: Vec::new(),
            runtime: Vec::new(),
            deploy: None,
            logging: None,
            tracing: None,
            observability: None,
            locale: None,
            encryption_bindings: Vec::new(),
            cookie: None,
            proxy: None,
            limits: None,
            headers: None,
            span_ref: None,
        }
    }

    #[test]
    fn non_microservices_mode_does_not_fire() {
        let module = empty_module();
        let app = app_with_mode(Some("modular_monolith"));

        assert!(check(&module, Some(&app)).is_empty());
        assert_eq!(Finding::CODE, "CROSS-FEATURE-CONTRACT-VERSION-DRIFT-001");
    }

    #[test]
    fn microservices_mode_returns_empty_pending_pin_syntax() {
        let module = empty_module();
        let app = app_with_mode(Some("microservices"));

        assert!(check(&module, Some(&app)).is_empty());
    }

    #[test]
    fn microservices_mode_gate_evaluates_correctly() {
        let app = app_with_mode(Some("microservices"));

        assert!(is_microservices(Some(&app)));
        assert!(!is_microservices(None));
    }
}
