//! `lzx-source-resource-mismatch` — `columns`/`search`/`filter` field not on
//! the resource backing `source`.
//!
//! Fires on `view list` and `view detail` views when any name in their
//! `columns`, `search`, or `filter` clauses is absent from the resource the
//! view's `source` query backs. Suppresses when the source query is unknown
//! OR doesn't have a backing resource (those are upstream errors raised by
//! the analyzer pass; this rule only catches the column-vs-resource
//! mismatch).
//!
//! Severity: `error` per `docs/proposals/lzx-integration-codegen.md` §11.

use super::ir_stub::{Feature, Module, View};
use super::{find_resource_for_query, sort_findings};

/// One `lzx-source-resource-mismatch` finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub feature: String,
    pub view: String,
    pub line: usize,
    pub message: String,
}

impl Finding {
    /// Stable doctor rule code surfaced to the user.
    pub const CODE: &'static str = "lzx-source-resource-mismatch";

    /// Render the canonical diagnostic message.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// // let msg = Finding::message(/* ... */);
    /// ```
    pub fn message(feature: &str, view: &str, clause: &str, field: &str, resource: &str) -> String {
        format!(
            "view `{view}` in feature `{feature}`: {clause} field `{field}` is not \
             declared on resource `{resource}` (the resource backing `source`). \
             Add the field to the resource or fix the typo in `{clause}`. See \
             docs/proposals/lzx-integration-codegen.md §11."
        )
    }
}

/// Run `lzx-source-resource-mismatch` across all `.lzx` surfaces in the module.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_cli::doctor::lzx::source_resource_mismatch::check;
/// use lazuli_cli::doctor::lzx::ir_stub::Module;
///
/// // let findings = check(&module);
/// ```
pub fn check(module: &Module) -> Vec<Finding> {
    let mut out = Vec::new();
    for feature in &module.features {
        out.extend(check_feature(feature));
    }
    sort_findings(&mut out, |f| {
        (f.feature.clone(), f.view.clone(), Finding::CODE, f.line)
    });
    out
}

fn check_feature(feature: &Feature) -> Vec<Finding> {
    let mut out = Vec::new();
    for surface in &feature.surfaces {
        for audience in &surface.audiences {
            for view in &audience.views {
                check_view(feature, view, &mut out);
            }
        }
    }
    out
}

fn check_view(feature: &Feature, view: &View, out: &mut Vec<Finding>) {
    // Only list and detail views have source/columns/search/filter; create
    // views go through `command_input_mismatch` instead.
    let (name, line, source, columns, search, filter): (
        &str,
        usize,
        _,
        &[String],
        &[String],
        &[String],
    ) = match view {
        View::List(v) => (
            v.name.as_str(),
            v.line,
            &v.source,
            v.columns.as_slice(),
            v.search.as_slice(),
            v.filter.as_slice(),
        ),
        View::Detail(v) => (v.name.as_str(), v.line, &v.source, &[], &[], &[]),
        View::Create(_) => return,
    };

    let resource = match find_resource_for_query(feature, source) {
        Some(r) => r,
        // Unknown query / no backing resource — out of this rule's scope.
        None => return,
    };

    let resource_fields = &resource.fields;
    for (clause, fields) in [("columns", columns), ("search", search), ("filter", filter)] {
        for field in fields {
            if !resource_fields.iter().any(|f| f == field) {
                out.push(Finding {
                    feature: feature.name.clone(),
                    view: name.to_owned(),
                    line,
                    message: Finding::message(&feature.name, name, clause, field, &resource.name),
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::ir_stub::*;
    use super::*;

    fn mk_resource(name: &str, fields: &[&str]) -> Resource {
        Resource {
            name: name.to_owned(),
            id_type: "ID".to_owned(),
            fields: fields.iter().map(|s| (*s).to_owned()).collect(),
        }
    }

    fn mk_query(name: &str, backs: Option<&str>) -> ListQuery {
        ListQuery {
            name: name.to_owned(),
            input_slots: vec![],
            backs_resource: backs.map(str::to_owned),
            input_slot_meta: vec![],
            params: vec![],
        }
    }

    fn mk_view_list(
        name: &str,
        line: usize,
        source: &str,
        columns: &[&str],
        search: &[&str],
        filter: &[&str],
    ) -> View {
        View::List(ViewList {
            name: name.to_owned(),
            at: Some(format!("/{}", name)),
            source: QueryRef {
                feature: None,
                name: source.to_owned(),
            },
            render: ListRender::Table {
                columns: columns.iter().map(|s| (*s).to_owned()).collect(),
            },
            columns: columns.iter().map(|s| (*s).to_owned()).collect(),
            search: search.iter().map(|s| (*s).to_owned()).collect(),
            filter: filter.iter().map(|s| (*s).to_owned()).collect(),
            search_decl: None,
            filter_decls: vec![],
            selection: None,
            sort: None,
            cells: vec![],
            actions: vec![],
            drawer: None,
            line,
        })
    }

    fn mk_feature(views: Vec<View>) -> Feature {
        Feature {
            name: "slug".into(),
            resources: vec![mk_resource("Slug", &["key", "title", "tags", "created_at"])],
            queries: vec![mk_query("mine", Some("Slug")), mk_query("nobacking", None)],
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
        }
    }

    fn mk_module(views: Vec<View>) -> Module {
        Module {
            features: vec![mk_feature(views)],
        }
    }

    /// Negative — every column/search/filter field exists on the resource.
    #[test]
    fn negative_all_fields_valid_produces_no_finding() {
        let v = mk_view_list(
            "slug_list",
            10,
            "mine",
            &["key", "title"],
            &["key"],
            &["tags"],
        );
        let module = mk_module(vec![v]);
        assert!(check(&module).is_empty(), "no violation expected");
    }

    /// Positive — a bogus column fires exactly one finding.
    #[test]
    fn positive_single_bad_column_fires_once() {
        let v = mk_view_list("slug_list", 11, "mine", &["key", "ghost"], &[], &[]);
        let module = mk_module(vec![v]);
        let findings = check(&module);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].feature, "slug");
        assert_eq!(findings[0].view, "slug_list");
        assert_eq!(findings[0].line, 11);
        assert!(findings[0].message.contains("ghost"));
        assert!(findings[0].message.contains("columns"));
    }

    /// Edge — bad field in each of columns/search/filter on the same view
    /// produces three findings; suppressed when source query has no backing
    /// resource.
    #[test]
    fn edge_multiple_clauses_violate_and_no_backing_suppresses() {
        let bad_view = mk_view_list(
            "slug_list",
            12,
            "mine",
            &["ghost1"],
            &["ghost2"],
            &["ghost3"],
        );
        let suppressed_view = mk_view_list(
            "untracked",
            13,
            "nobacking", // query has no backing resource
            &["whatever"],
            &[],
            &[],
        );
        let module = mk_module(vec![bad_view, suppressed_view]);
        let findings = check(&module);
        assert_eq!(findings.len(), 3, "exactly three findings for the bad_view");

        let clauses: Vec<&str> = findings
            .iter()
            .map(|f| {
                if f.message.contains("columns field") {
                    "columns"
                } else if f.message.contains("search field") {
                    "search"
                } else if f.message.contains("filter field") {
                    "filter"
                } else {
                    "unknown"
                }
            })
            .collect();
        assert!(clauses.contains(&"columns"));
        assert!(clauses.contains(&"search"));
        assert!(clauses.contains(&"filter"));

        // All findings anchor to bad_view (line 12), not the suppressed view.
        for f in &findings {
            assert_eq!(f.view, "slug_list");
            assert_eq!(f.line, 12);
        }
    }

    /// Detail view with bogus column on its source resource is suppressed
    /// because detail views don't carry `columns` per §5.1.
    #[test]
    fn detail_view_columns_suppressed() {
        let v = View::Detail(ViewDetail {
            name: "slug_detail".into(),
            at: Some("/slugs/:key".into()),
            source: QueryRef {
                feature: None,
                name: "mine".into(),
            },
            route_params: vec![],
            sections: vec![],
            cells: vec![],
            actions: vec![],
            line: 20,
        });
        let module = mk_module(vec![v]);
        assert!(check(&module).is_empty());
    }
}
