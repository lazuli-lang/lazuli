//! `lzx-list-cells-or-columns` — `view list` must declare table columns or a
//! grid cell slot.
//!
//! Fires on `view list` declarations lowered as table lists when they have
//! neither `columns <field>, ...` nor grid-form `cells @client.<slot>`.
//! Per-column bindings (`cells <field> @client.<slot>`) suppress this rule
//! because they are handled by the cell-slot orphan checks.
//!
//! Severity: `error` per `docs/proposals/lzx-terminal-grammar.md` §3.1.

use lazuli_ir::{ListRender, Module, View};

use super::sort_findings;

/// One `lzx-list-cells-or-columns` finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub feature: String,
    pub view: String,
    pub line: usize,
    pub message: String,
}

impl Finding {
    pub const CODE: &'static str = "lzx-list-cells-or-columns";

    pub fn message(view: &str) -> String {
        format!(
            "view list '{view}' has neither 'columns <field>, ...' nor \
             'cells @client.<slot>' — declare exactly one form"
        )
    }
}

/// Run `lzx-list-cells-or-columns` across all lowered `.lzx` surfaces.
pub fn check(module: &Module) -> Vec<Finding> {
    let mut out = Vec::new();
    for feature in &module.features {
        for surface in &feature.surfaces {
            for audience in &surface.audiences {
                for view in &audience.views {
                    let View::List(view) = view else {
                        continue;
                    };

                    match &view.render {
                        ListRender::Table { columns }
                            if columns.is_empty() && view.cells.is_empty() =>
                        {
                            out.push(Finding {
                                feature: feature.name.clone(),
                                view: view.name.clone(),
                                line: view.span_ref.map(|span| span.start).unwrap_or(0),
                                message: Finding::message(&view.name),
                            });
                        }
                        ListRender::Table { .. } | ListRender::Cells { .. } => {}
                    }
                }
            }
        }
    }

    sort_findings(&mut out, |f| {
        (f.feature.clone(), f.view.clone(), Finding::CODE, f.line)
    });
    out
}

#[cfg(test)]
mod tests {
    use lazuli_ir::{
        Audience, Defaults, Feature, ListRender, Module, Policies, QueryKind, QueryRef, Surface,
        SurfaceTarget, View, ViewList,
    };

    use super::*;

    fn list_view(name: &str, render: ListRender) -> View {
        View::List(ViewList {
            name: name.to_owned(),
            route: Some(format!("/{name}")),
            source: QueryRef {
                feature: "slug".into(),
                kind: QueryKind::List,
                name: "mine".into(),
            },
            render,
            search: None,
            filter: vec![],
            cells: vec![],
            actions: vec![],
            drawer: None,
            sort: None,
            selection: None,
            settings: vec![],
            redacted_fields: Vec::new(),
            span_ref: None,
        })
    }

    fn module_with_views(views: Vec<View>) -> Module {
        Module {
            workspace: None,
            contracts: vec![],
            app: None,
            registry: None,
            profiles: vec![],
            design: None,
            rbac: None,
            features: vec![Feature {
                name: "slug".into(),
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
                auth: None,
                surfaces: vec![Surface {
                    feature: "slug".into(),
                    target: SurfaceTarget::Web,
                    audiences: vec![Audience {
                        name: "admin".into(),
                        requires: vec![],
                        views,
                        span_ref: None,
                    }],
                    span_ref: None,
                }],
                extensions: vec![],
                escape_routes: vec![],
                agents: vec![],
                reports: vec![],
                channels: vec![],
                caches: vec![],
                aggregates: vec![],
                mcp_servers: vec![],
                previous_names: vec![],
                synth_origins: std::collections::BTreeMap::new(),
                span_ref: None,
            }],
        }
    }

    #[test]
    fn canonical_table_without_columns_or_cells_fires() {
        let module = module_with_views(vec![list_view(
            "slug_list",
            ListRender::Table { columns: vec![] },
        )]);

        let findings = check(&module);

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].feature, "slug");
        assert_eq!(findings[0].view, "slug_list");
        assert_eq!(
            findings[0].message,
            "view list 'slug_list' has neither 'columns <field>, ...' nor \
             'cells @client.<slot>' — declare exactly one form"
        );
    }

    #[test]
    fn columns_or_grid_cells_slot_satisfy_rule() {
        let module = module_with_views(vec![
            list_view(
                "table_list",
                ListRender::Table {
                    columns: vec!["key".into()],
                },
            ),
            list_view(
                "grid_list",
                ListRender::Cells {
                    slot: "item_card".into(),
                },
            ),
        ]);

        assert!(check(&module).is_empty());
    }
}
