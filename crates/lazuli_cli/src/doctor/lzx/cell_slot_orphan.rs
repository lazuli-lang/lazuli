//! `lzx-cell-slot-orphan` — `cells <field> @client.<slot>` references a field
//! NOT in `columns`/`fields`/`sections`.
//!
//! Fires on any view that carries `cells <field> @client.<slot>` bindings
//! whose `<field>` doesn't appear in the field set of the enclosing view kind:
//!
//! - `view list`: must be in `columns` (search/filter alone aren't enough —
//!   they don't materialise as rendered cells).
//! - `view detail`: must be in `sections` (cells in detail views target
//!   section slots; standalone cells on detail views without a matching
//!   section are dead code).
//! - `view create`: must be in `fields`.
//!
//! Severity: `warning` per `docs/proposals/lzx-integration-codegen.md` §11
//! (the slot is harmless at runtime but useless — flag it before it confuses
//! downstream developers).

use super::ir_stub::{CellBinding, Module, View};
use super::sort_findings;

/// One `lzx-cell-slot-orphan` finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub feature: String,
    pub view: String,
    pub line: usize,
    pub message: String,
}

impl Finding {
    pub const CODE: &'static str = "lzx-cell-slot-orphan";

    pub fn message(
        feature: &str,
        view: &str,
        field: &str,
        slot: &str,
        view_kind: &str,
        anchor_clause: &str,
    ) -> String {
        format!(
            "view `{view}` in feature `{feature}` ({view_kind}): cell binding \
             `cells {field} {slot}` references a field not in `{anchor_clause}`. \
             Either include `{field}` in `{anchor_clause}` or remove the binding. \
             See docs/proposals/lzx-integration-codegen.md §11."
        )
    }
}

/// Run `lzx-cell-slot-orphan` across all `.lzx` surfaces.
pub fn check(module: &Module) -> Vec<Finding> {
    let mut out = Vec::new();
    for feature in &module.features {
        for surface in &feature.surfaces {
            for audience in &surface.audiences {
                for view in &audience.views {
                    check_view(&feature.name, view, &mut out);
                }
            }
        }
    }
    sort_findings(&mut out, |f| {
        (f.feature.clone(), f.view.clone(), Finding::CODE, f.line)
    });
    out
}

fn check_view(feature_name: &str, view: &View, out: &mut Vec<Finding>) {
    let (view_name, view_kind, anchor_clause, anchors, cells): (
        &str,
        &str,
        &str,
        &[String],
        &[CellBinding],
    ) = match view {
        View::List(v) => (
            v.name.as_str(),
            "view list",
            "columns",
            v.columns.as_slice(),
            v.cells.as_slice(),
        ),
        View::Detail(v) => (
            v.name.as_str(),
            "view detail",
            "sections",
            v.sections.as_slice(),
            v.cells.as_slice(),
        ),
        View::Create(v) => (
            v.name.as_str(),
            "view create",
            "fields",
            v.fields.as_slice(),
            v.cells.as_slice(),
        ),
    };

    for cell in cells {
        if !anchors.iter().any(|a| a == &cell.field) {
            out.push(Finding {
                feature: feature_name.to_owned(),
                view: view_name.to_owned(),
                line: cell.line,
                message: Finding::message(
                    feature_name,
                    view_name,
                    &cell.field,
                    &cell.slot,
                    view_kind,
                    anchor_clause,
                ),
            });
        }
    }
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

    fn list_view(name: &str, line: usize, columns: &[&str], cells: Vec<CellBinding>) -> View {
        View::List(ViewList {
            name: name.to_owned(),
            at: None,
            source: QueryRef {
                feature: None,
                name: "mine".into(),
            },
            render: ListRender::Table {
                columns: columns.iter().map(|s| (*s).to_owned()).collect(),
            },
            columns: columns.iter().map(|s| (*s).to_owned()).collect(),
            search: vec![],
            filter: vec![],
            search_decl: None,
            filter_decls: vec![],
            selection: None,
            cells,
            actions: vec![],
            drawer: None,
            line,
        })
    }

    fn detail_view(name: &str, line: usize, sections: &[&str], cells: Vec<CellBinding>) -> View {
        View::Detail(ViewDetail {
            name: name.to_owned(),
            at: None,
            source: QueryRef {
                feature: None,
                name: "by_key".into(),
            },
            route_params: vec![],
            sections: sections.iter().map(|s| (*s).to_owned()).collect(),
            cells,
            actions: vec![],
            line,
        })
    }

    fn create_view(name: &str, line: usize, fields: &[&str], cells: Vec<CellBinding>) -> View {
        View::Create(ViewCreate {
            name: name.to_owned(),
            at: None,
            submit: CommandRef {
                feature: None,
                name: "create".into(),
                line,
            },
            fields: fields.iter().map(|s| (*s).to_owned()).collect(),
            cells,
            line,
        })
    }

    fn mk_module(views: Vec<View>) -> Module {
        Module {
            features: vec![Feature {
                name: "slug".into(),
                resources: vec![],
                queries: vec![],
                commands: vec![],
                surfaces: vec![Surface {
                    feature: "slug".into(),
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

    /// Negative — every cell binding's field is in the anchor clause.
    #[test]
    fn negative_all_cells_anchored_no_finding() {
        let v = list_view(
            "slug_list",
            10,
            &["key", "tags"],
            vec![cell("tags", "@client.type_badge", 12)],
        );
        let module = mk_module(vec![v]);
        assert!(check(&module).is_empty());
    }

    /// Positive — orphaned cell field on a list view fires.
    #[test]
    fn positive_orphan_in_list_view_fires() {
        let v = list_view(
            "slug_list",
            10,
            &["key"],
            vec![cell("ghost", "@client.badge", 13)],
        );
        let module = mk_module(vec![v]);
        let findings = check(&module);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].line, 13);
        assert!(findings[0].message.contains("ghost"));
        assert!(findings[0].message.contains("columns"));
        assert!(findings[0].message.contains("@client.badge"));
    }

    /// Edge — orphan rules differ per view kind (sections vs fields vs columns).
    #[test]
    fn edge_per_view_kind_anchors() {
        let list = list_view("L", 10, &["a"], vec![cell("zzz", "@client.x", 11)]);
        let detail = detail_view(
            "D",
            20,
            &["header"],
            vec![cell("missing_section", "@client.y", 21)],
        );
        let create = create_view(
            "C",
            30,
            &["name"],
            vec![cell("orphan_field", "@client.z", 31)],
        );
        let module = mk_module(vec![list, detail, create]);
        let findings = check(&module);
        assert_eq!(findings.len(), 3, "one finding per view");
        let messages: Vec<&str> = findings.iter().map(|f| f.message.as_str()).collect();
        assert!(messages.iter().any(|m| m.contains("columns")));
        assert!(messages.iter().any(|m| m.contains("sections")));
        assert!(messages.iter().any(|m| m.contains("fields")));
    }

    /// Edge — multiple orphans on one view all fire.
    #[test]
    fn edge_multiple_orphans_same_view() {
        let v = list_view(
            "slug_list",
            10,
            &["key"],
            vec![
                cell("a", "@client.x", 11),
                cell("b", "@client.y", 12),
                cell("key", "@client.z", 13), // valid — should NOT fire
            ],
        );
        let module = mk_module(vec![v]);
        let findings = check(&module);
        assert_eq!(findings.len(), 2);
    }
}
