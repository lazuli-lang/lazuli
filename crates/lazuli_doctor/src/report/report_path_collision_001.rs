//! REPORT-PATH-COLLISION-001 — report auto-mounted HTTP endpoints collide
//! with explicit `api` routes or agent `expose http` routes.
//!
//! Reports auto-mount `GET /api/reports/<name>.<format>` for each declared
//! format. This rule keeps those reserved paths from being shadowed by
//! authored HTTP routes in the same feature.

use std::path::{Path, PathBuf};

use lazuli_ir::{Feature, HttpMethod, ReportFormat};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub path: PathBuf,
    pub feature: String,
    pub report: String,
    pub mount_path: String,
    pub other_kind: &'static str,
    pub other_name: String,
}

impl Finding {
    pub const CODE: &'static str = "REPORT-PATH-COLLISION-001";

    pub fn message(&self) -> String {
        format!(
            "REPORT-PATH-COLLISION-001: report '{}' auto-mount path '{}' collides with {} '{}' at the same route.",
            self.report, self.mount_path, self.other_kind, self.other_name
        )
    }
}

pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    let mut findings = Vec::new();

    for report in &feature.reports {
        for format in &report.formats {
            let mount_path = report_mount_path(&report.name, *format);
            let mount_stem = report_mount_stem(&report.name);

            for api in &feature.apis {
                if api.method == HttpMethod::Get
                    && route_conflicts(&mount_path, &mount_stem, &api.path)
                {
                    findings.push(Finding {
                        path: path.to_path_buf(),
                        feature: feature.name.clone(),
                        report: report.name.clone(),
                        mount_path: mount_path.clone(),
                        other_kind: "api",
                        other_name: api.name.clone(),
                    });
                }
            }

            for agent in &feature.agents {
                let Some(exposure) = &agent.expose_http else {
                    continue;
                };
                if exposure.method == HttpMethod::Get
                    && route_conflicts(&mount_path, &mount_stem, &exposure.path)
                {
                    findings.push(Finding {
                        path: path.to_path_buf(),
                        feature: feature.name.clone(),
                        report: report.name.clone(),
                        mount_path: mount_path.clone(),
                        other_kind: "agent",
                        other_name: agent.name.clone(),
                    });
                }
            }
        }
    }

    findings
}

fn report_mount_path(report_name: &str, format: ReportFormat) -> String {
    format!("{}.{}", report_mount_stem(report_name), format.token())
}

fn report_mount_stem(report_name: &str) -> String {
    format!("/api/reports/{report_name}")
}

fn route_conflicts(mount_path: &str, mount_stem: &str, other_path: &str) -> bool {
    if other_path == mount_path {
        return true;
    }

    other_path
        .strip_prefix(mount_stem)
        .is_some_and(|suffix| suffix.starts_with('.') && !suffix[1..].contains('/'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        Agent, AgentOutputKind, Api, Defaults, FileVisibility, HttpExposure, PathRef, Policies,
        PolicyRef, QualifiedName, Report, ReportColumn, ReportColumnSource, ReportSource, TypeRef,
    };

    fn mk_feature(reports: Vec<Report>, apis: Vec<Api>, agents: Vec<Agent>) -> Feature {
        Feature {
            name: "customer".into(),
            purpose: None,
            non_goals: vec![],
            context_path: None,
            defaults: Defaults::default(),
            uses: vec![],
            uses_spans: Vec::new(),
            requirements: vec![],
            enums: vec![],
            resources: vec![],
            events: vec![],
            rules: vec![],
            policies: Policies::default(),
            commands: vec![],
            apis,
            records: vec![],
            queries: vec![],
            workflows: vec![],
            jobs: vec![],
            webhooks: vec![],
            notifications: vec![],
            event_groups: vec![],
            tenant_migrations: vec![],
            translation: None,
            auth: None,
            surfaces: vec![],
            extensions: vec![],
            escape_routes: vec![],
            agents,
            pollers: vec![],
            reports,
            channels: vec![],
            caches: vec![],
            aggregates: vec![],
            previous_names: vec![],
            span_ref: None,
        }
    }

    fn mk_report(name: &str, formats: Vec<ReportFormat>) -> Report {
        Report {
            name: name.into(),
            source: ReportSource::Query(QualifiedName {
                feature: None,
                name: "list".into(),
            }),
            columns: vec![ReportColumn {
                name: "id".into(),
                source: ReportColumnSource::RowField("id".into()),
                label: None,
                format: None,
                span_ref: None,
            }],
            formats,
            storage: None,
            visibility: FileVisibility::Signed,
            signed_ttl: Some("1h".into()),
            filename: None,
            policy: PolicyRef::None,
            policy_expr: None,
            rate_limit: None,
            audit: None,
            span_ref: None,
        }
    }

    fn mk_api(name: &str, path: &str) -> Api {
        Api {
            name: name.into(),
            method: HttpMethod::Get,
            path: path.into(),
            policy: PolicyRef::None,
            policy_expr: None,
            rate_limit: None,
            output: TypeRef::Builtin(lazuli_ir::BuiltinType::Text),
            handler: PathRef::authored("./api/download.go"),
            locale_negotiate: None,
            deprecated: None,
            span_ref: None,
        }
    }

    fn mk_agent(name: &str, path: &str) -> Agent {
        Agent {
            name: name.into(),
            feature: "customer".into(),
            input: vec![],
            context: None,
            policy: None,
            rate_limit: None,
            output_kind: AgentOutputKind::Text,
            output_type: Some(TypeRef::Builtin(lazuli_ir::BuiltinType::Text)),
            output_discriminator: None,
            model: None,
            temperature: None,
            max_tokens: None,
            top_p: None,
            seed: None,
            prompt_path: None,
            safety: vec![],
            tools: vec![],
            evals: vec![],
            expose_http: Some(HttpExposure {
                method: HttpMethod::Get,
                path: path.into(),
                route_slots: vec![],
                audience: None,
                rate_limit_override: None,
                span_ref: None,
            }),
            span_ref: None,
        }
    }

    #[test]
    fn non_overlapping_api_does_not_fire() {
        let feature = mk_feature(
            vec![mk_report("download_results", vec![ReportFormat::Csv])],
            vec![mk_api("download", "/api/download_results.csv")],
            vec![],
        );

        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }

    #[test]
    fn api_exact_path_collision_fires() {
        let feature = mk_feature(
            vec![mk_report("download_results", vec![ReportFormat::Csv])],
            vec![mk_api("download", "/api/reports/download_results.csv")],
            vec![],
        );

        let findings = check(&feature, Path::new("f.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].other_kind, "api");
        assert_eq!(findings[0].mount_path, "/api/reports/download_results.csv");
    }

    #[test]
    fn agent_expose_path_collision_fires() {
        let feature = mk_feature(
            vec![mk_report("foo", vec![ReportFormat::Xlsx])],
            vec![],
            vec![mk_agent("export_foo", "/api/reports/foo.xlsx")],
        );

        let findings = check(&feature, Path::new("f.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].other_kind, "agent");
        assert_eq!(findings[0].other_name, "export_foo");
    }
}
