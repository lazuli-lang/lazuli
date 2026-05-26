//! `lzx-cells-mixed-form` — `view list` declares both grid-row cells and
//! per-column cell bindings.
//!
//! Fires when a list view uses the L0 #6 grid form (`cells @client.<slot>`,
//! carried as `ListRender::Cells`) and also carries v1 per-column bindings
//! (`cells <field> @client.<slot>`, carried in `view.cells`).
//!
//! Severity: `error`. The two forms describe different view shapes: one
//! client slot per row vs one client slot per column.

use super::ir_stub::{ListRender, Module, View};
use super::sort_findings;

/// One `lzx-cells-mixed-form` finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub feature: String,
    pub view: String,
    pub line: usize,
    pub message: String,
}

impl Finding {
    /// Stable doctor rule code surfaced to the user.
    pub const CODE: &'static str = "lzx-cells-mixed-form";

    /// Render the canonical diagnostic message.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// // let msg = Finding::message(/* ... */);
    /// ```
    pub fn message(view: &str) -> String {
        format!(
            "view list '{view}' declares both grid-cell slot (cells \
             @client.<slot>) and per-column cell bindings (cells <field> \
             @client.<slot>) — these are mutually exclusive; the grid form is \
             for one slot per row, the per-column form is for one slot per \
             column. Fix by using either `cells @client.item_card` for the \
             grid-row form, or `columns key, title` plus `cells title \
             @client.title_cell` for per-column bindings."
        )
    }
}

/// Run `lzx-cells-mixed-form` across all `.lzx` surfaces.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_cli::doctor::lzx::cells_mixed_form::check;
/// use lazuli_cli::doctor::lzx::ir_stub::Module;
///
/// // let findings = check(&module);
/// ```
pub fn check(module: &Module) -> Vec<Finding> {
    let mut out = Vec::new();
    for feature in &module.features {
        for surface in &feature.surfaces {
            for audience in &surface.audiences {
                for view in &audience.views {
                    if let View::List(list) = view {
                        if matches!(list.render, ListRender::Cells { .. }) && !list.cells.is_empty()
                        {
                            out.push(Finding {
                                feature: feature.name.clone(),
                                view: list.name.clone(),
                                line: list.line,
                                message: Finding::message(&list.name),
                            });
                        }
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
    use super::super::ir_stub::*;
    use super::*;

    fn cell(field: &str, slot: &str, line: usize) -> CellBinding {
        CellBinding {
            field: field.to_owned(),
            slot: slot.to_owned(),
            line,
        }
    }

    fn list_view(name: &str, render: ListRender, cells: Vec<CellBinding>) -> View {
        View::List(ViewList {
            name: name.to_owned(),
            at: Some("/items".into()),
            source: QueryRef {
                feature: None,
                name: "search".into(),
            },
            columns: match &render {
                ListRender::Table { columns } => columns.clone(),
                ListRender::Cells { .. } => vec![],
            },
            render,
            search: vec![],
            filter: vec![],
            search_decl: None,
            filter_decls: vec![],
            selection: None,
            sort: None,
            cells,
            actions: vec![],
            drawer: None,
            line: 10,
        })
    }

    fn mk_module(views: Vec<View>) -> Module {
        Module {
            features: vec![Feature {
                name: "item".into(),
                resources: vec![],
                queries: vec![],
                commands: vec![],
                surfaces: vec![Surface {
                    feature: "item".into(),
                    target: "web".into(),
                    audiences: vec![Audience {
                        name: "admin".into(),
                        requires: vec![],
                        views,
                        line: 3,
                    }],
                }],
            }],
        }
    }

    #[test]
    fn violation_grid_cells_and_per_column_cells_fires() {
        let module = mk_module(vec![list_view(
            "item_terminal",
            ListRender::Cells {
                slot: "item_card".into(),
            },
            vec![cell("tags", "@client.tags_cell", 12)],
        )]);

        let findings = check(&module);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].feature, "item");
        assert_eq!(findings[0].view, "item_terminal");
        assert_eq!(findings[0].line, 10);
        assert!(findings[0].message.contains("view list 'item_terminal'"));
        assert!(findings[0].message.contains("cells @client.<slot>"));
        assert!(findings[0].message.contains("cells <field> @client.<slot>"));
        assert!(findings[0].message.contains("cells @client.item_card"));
        assert!(findings[0].message.contains("columns key, title"));
    }

    #[test]
    fn non_violation_only_one_cells_form_is_allowed() {
        let grid_only = list_view(
            "item_terminal",
            ListRender::Cells {
                slot: "item_card".into(),
            },
            vec![],
        );
        let per_column_only = list_view(
            "slug_list",
            ListRender::Table {
                columns: vec!["key".into(), "tags".into()],
            },
            vec![cell("tags", "@client.tags_cell", 22)],
        );

        let module = mk_module(vec![grid_only, per_column_only]);
        assert!(check(&module).is_empty());
    }
}
