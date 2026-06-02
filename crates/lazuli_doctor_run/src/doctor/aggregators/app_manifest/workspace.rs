//! `WS-*` aggregator — multi-app workspace contract checks
//! (`workspace.lzi`).
//!
//! Covers: WS-APP-001/002, WS-BOUNDARY-001, WS-EVENT-001, WS-GW-001/002/003/004,
//! WS-COMM-001.

use std::collections::BTreeSet;

use crate::doctor::{DoctorAppWorkspace, DoctorDiagnostic, DoctorSeverity};

pub(crate) fn workspace_contract_diagnostics(
    workspace: Option<&DoctorAppWorkspace>,
) -> Vec<DoctorDiagnostic> {
    let Some(workspace) = workspace else {
        return Vec::new();
    };

    let mut diagnostics = Vec::new();
    let mut app_names = BTreeSet::new();
    let mut published = Vec::new();

    for app in &workspace.manifest.apps {
        if !app_names.insert(app.name.as_str()) {
            diagnostics.push(DoctorDiagnostic {
                path: workspace.path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "WS-APP-001".to_owned(),
                message: format!(
                    "workspace declares app `{}` more than once; app ids must be unique.",
                    app.name
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }

        if app.kind == "local"
            && app
                .path
                .as_deref()
                .is_some_and(|path| !path.ends_with(".lzi"))
        {
            diagnostics.push(DoctorDiagnostic {
                path: workspace.path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Warning,
                code: "WS-APP-002".to_owned(),
                message: format!(
                    "workspace local app `{}` should point at an `app.lzi` entrypoint.",
                    app.name
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
    }

    for boundary in &workspace.manifest.boundaries {
        if !app_names.contains(boundary.app.as_str()) {
            diagnostics.push(DoctorDiagnostic {
                path: workspace.path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "WS-BOUNDARY-001".to_owned(),
                message: format!(
                    "workspace boundary references unknown app `{}`.",
                    boundary.app
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }

        if boundary.direction == "publishes" {
            published.push(boundary.pattern.as_str());
        }
    }

    for boundary in &workspace.manifest.boundaries {
        if boundary.direction != "consumes" {
            continue;
        }
        if !published
            .iter()
            .any(|published| event_pattern_covers(published, &boundary.pattern))
        {
            diagnostics.push(DoctorDiagnostic {
                path: workspace.path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "WS-EVENT-001".to_owned(),
                message: format!(
                    "workspace app `{}` consumes `{}`, but no workspace app publishes a compatible event pattern.",
                    boundary.app, boundary.pattern
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
    }

    for gateway in &workspace.manifest.gateways {
        for route in &gateway.routes {
            if route.target_kind != "app" {
                diagnostics.push(DoctorDiagnostic {
                    path: workspace.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "WS-GW-001".to_owned(),
                    message: format!(
                        "workspace gateway `{}` route `{}` targets `{}`; only `to app <name>` is supported in the language contract.",
                        gateway.name, route.path, route.target_kind
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            } else if !app_names.contains(route.target.as_str()) {
                diagnostics.push(DoctorDiagnostic {
                    path: workspace.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "WS-GW-002".to_owned(),
                    message: format!(
                        "workspace gateway `{}` route `{}` targets unknown app `{}`.",
                        gateway.name, route.path, route.target
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }

            if route.auth.as_deref() != Some("propagate") {
                diagnostics.push(DoctorDiagnostic {
                    path: workspace.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Warning,
                    code: "WS-GW-003".to_owned(),
                    message: format!(
                        "workspace gateway `{}` route `{}` should declare `auth propagate` so the runtime does not infer auth context.",
                        gateway.name, route.path
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }

            if route.tenant.as_deref() != Some("propagate") {
                diagnostics.push(DoctorDiagnostic {
                    path: workspace.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Warning,
                    code: "WS-GW-004".to_owned(),
                    message: format!(
                        "workspace gateway `{}` route `{}` should declare `tenant propagate` so tenant context crosses app boundaries explicitly.",
                        gateway.name, route.path
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }
        }
    }

    if !workspace.manifest.gateways.is_empty() {
        let propagated: BTreeSet<_> = workspace
            .manifest
            .communication
            .as_ref()
            .map(|communication| {
                communication
                    .propagate
                    .iter()
                    .map(String::as_str)
                    .collect::<BTreeSet<_>>()
            })
            .unwrap_or_default();
        for required in ["tenant", "trace_id", "request_id"] {
            if !propagated.contains(required) {
                diagnostics.push(DoctorDiagnostic {
                    path: workspace.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Warning,
                    code: "WS-COMM-001".to_owned(),
                    message: format!(
                        "workspace gateways should propagate `{required}` in the `communication` block."
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }
        }
    }

    diagnostics
}

pub(crate) fn event_pattern_covers(published: &str, consumed: &str) -> bool {
    if published == consumed {
        return true;
    }

    published
        .strip_suffix('*')
        .is_some_and(|prefix| consumed.starts_with(prefix))
}

#[cfg(test)]
mod tests {
    //! W3-3: the WS-* workspace aggregator (the cross-app boundary +
    //! gateway contract) had no inline test. These pin the correctness/
    //! security-critical codes: WS-APP-001 (duplicate app id), WS-APP-002
    //! (local app entrypoint), WS-BOUNDARY-001 (boundary on unknown app),
    //! WS-EVENT-001 (consumes with no compatible publisher — a silent
    //! cross-app contract break), WS-GW-001/002 (gateway route target
    //! errors), and WS-GW-003 (missing `auth propagate` — a gateway auth-
    //! context inference footgun). A clean workspace stays quiet.
    use super::*;
    use crate::doctor::DoctorAppWorkspace;
    use lazuli_ir::{
        AppWorkspace, WorkspaceApp, WorkspaceBoundary, WorkspaceCommunication, WorkspaceGateway,
        WorkspaceGatewayRoute,
    };
    use std::path::PathBuf;

    fn app(name: &str, kind: &str, path: Option<&str>) -> WorkspaceApp {
        WorkspaceApp {
            name: name.to_owned(),
            kind: kind.to_owned(),
            path: path.map(str::to_owned),
            contract: None,
        }
    }

    fn boundary(app: &str, direction: &str, pattern: &str) -> WorkspaceBoundary {
        WorkspaceBoundary {
            app: app.to_owned(),
            direction: direction.to_owned(),
            pattern: pattern.to_owned(),
        }
    }

    fn route(path: &str, kind: &str, target: &str, propagate: bool) -> WorkspaceGatewayRoute {
        WorkspaceGatewayRoute {
            path: path.to_owned(),
            target_kind: kind.to_owned(),
            target: target.to_owned(),
            auth: propagate.then(|| "propagate".to_owned()),
            tenant: propagate.then(|| "propagate".to_owned()),
            timeout: None,
            rate_limit: None,
        }
    }

    fn ws(workspace: AppWorkspace) -> DoctorAppWorkspace {
        DoctorAppWorkspace {
            path: PathBuf::from("workspace.lzi"),
            manifest: workspace,
        }
    }

    fn full_propagation() -> Option<WorkspaceCommunication> {
        Some(WorkspaceCommunication {
            propagate: vec!["tenant".into(), "trace_id".into(), "request_id".into()],
            sync_default: None,
            async_default: None,
        })
    }

    fn codes(diags: &[DoctorDiagnostic]) -> Vec<&str> {
        diags.iter().map(|d| d.code.as_str()).collect()
    }

    fn run(workspace: AppWorkspace) -> Vec<DoctorDiagnostic> {
        workspace_contract_diagnostics(Some(&ws(workspace)))
    }

    #[test]
    fn clean_workspace_emits_nothing() {
        let diags = run(AppWorkspace {
            name: "main".into(),
            apps: vec![
                app("api", "local", Some("api/app.lzi")),
                app("web", "local", Some("web/app.lzi")),
            ],
            shared_registry: None,
            boundaries: vec![
                boundary("api", "publishes", "order.*"),
                boundary("web", "consumes", "order.created"),
            ],
            communication: full_propagation(),
            gateways: vec![WorkspaceGateway {
                name: "edge".into(),
                routes: vec![route("/api", "app", "api", true)],
            }],
            span_ref: None,
        });
        assert!(diags.is_empty(), "clean workspace, got {:?}", codes(&diags));
    }

    #[test]
    fn ws_app_001_fires_on_duplicate_app_id() {
        let diags = run(AppWorkspace {
            name: "main".into(),
            apps: vec![
                app("api", "local", Some("a/app.lzi")),
                app("api", "local", Some("b/app.lzi")),
            ],
            shared_registry: None,
            boundaries: vec![],
            communication: None,
            gateways: vec![],
            span_ref: None,
        });
        let hits: Vec<_> = diags.iter().filter(|d| d.code == "WS-APP-001").collect();
        assert_eq!(
            hits.len(),
            1,
            "want one WS-APP-001, got {:?}",
            codes(&diags)
        );
        assert_eq!(hits[0].severity, DoctorSeverity::Error);
    }

    #[test]
    fn ws_app_002_fires_when_local_app_path_not_lzi() {
        let diags = run(AppWorkspace {
            name: "main".into(),
            apps: vec![app("api", "local", Some("api/"))],
            shared_registry: None,
            boundaries: vec![],
            communication: None,
            gateways: vec![],
            span_ref: None,
        });
        let hits: Vec<_> = diags.iter().filter(|d| d.code == "WS-APP-002").collect();
        assert_eq!(
            hits.len(),
            1,
            "want one WS-APP-002, got {:?}",
            codes(&diags)
        );
    }

    #[test]
    fn ws_boundary_001_fires_on_unknown_app() {
        let diags = run(AppWorkspace {
            name: "main".into(),
            apps: vec![app("api", "local", Some("api/app.lzi"))],
            shared_registry: None,
            boundaries: vec![boundary("ghost", "publishes", "x.*")],
            communication: None,
            gateways: vec![],
            span_ref: None,
        });
        let hits: Vec<_> = diags
            .iter()
            .filter(|d| d.code == "WS-BOUNDARY-001")
            .collect();
        assert_eq!(
            hits.len(),
            1,
            "want one WS-BOUNDARY-001, got {:?}",
            codes(&diags)
        );
        assert_eq!(hits[0].severity, DoctorSeverity::Error);
    }

    #[test]
    fn ws_event_001_fires_when_consume_has_no_publisher() {
        let diags = run(AppWorkspace {
            name: "main".into(),
            apps: vec![
                app("api", "local", Some("api/app.lzi")),
                app("web", "local", Some("web/app.lzi")),
            ],
            shared_registry: None,
            // web consumes an event nobody publishes.
            boundaries: vec![boundary("web", "consumes", "order.created")],
            communication: None,
            gateways: vec![],
            span_ref: None,
        });
        let hits: Vec<_> = diags.iter().filter(|d| d.code == "WS-EVENT-001").collect();
        assert_eq!(
            hits.len(),
            1,
            "want one WS-EVENT-001, got {:?}",
            codes(&diags)
        );
        assert_eq!(hits[0].severity, DoctorSeverity::Error);
    }

    #[test]
    fn ws_gw_001_fires_when_route_target_kind_not_app() {
        let diags = run(AppWorkspace {
            name: "main".into(),
            apps: vec![app("api", "local", Some("api/app.lzi"))],
            shared_registry: None,
            boundaries: vec![],
            communication: full_propagation(),
            gateways: vec![WorkspaceGateway {
                name: "edge".into(),
                routes: vec![route("/api", "feature", "api.thing", true)],
            }],
            span_ref: None,
        });
        let hits: Vec<_> = diags.iter().filter(|d| d.code == "WS-GW-001").collect();
        assert_eq!(hits.len(), 1, "want one WS-GW-001, got {:?}", codes(&diags));
        assert_eq!(hits[0].severity, DoctorSeverity::Error);
    }

    #[test]
    fn ws_gw_002_fires_on_unknown_target_app() {
        let diags = run(AppWorkspace {
            name: "main".into(),
            apps: vec![app("api", "local", Some("api/app.lzi"))],
            shared_registry: None,
            boundaries: vec![],
            communication: full_propagation(),
            gateways: vec![WorkspaceGateway {
                name: "edge".into(),
                routes: vec![route("/admin", "app", "ghost", true)],
            }],
            span_ref: None,
        });
        let hits: Vec<_> = diags.iter().filter(|d| d.code == "WS-GW-002").collect();
        assert_eq!(hits.len(), 1, "want one WS-GW-002, got {:?}", codes(&diags));
    }

    #[test]
    fn ws_gw_003_fires_when_route_missing_auth_propagate() {
        let diags = run(AppWorkspace {
            name: "main".into(),
            apps: vec![app("api", "local", Some("api/app.lzi"))],
            shared_registry: None,
            boundaries: vec![],
            communication: full_propagation(),
            gateways: vec![WorkspaceGateway {
                name: "edge".into(),
                // propagate=false -> no `auth propagate` -> WS-GW-003.
                routes: vec![route("/api", "app", "api", false)],
            }],
            span_ref: None,
        });
        let hits: Vec<_> = diags.iter().filter(|d| d.code == "WS-GW-003").collect();
        assert_eq!(hits.len(), 1, "want one WS-GW-003, got {:?}", codes(&diags));
    }
}
