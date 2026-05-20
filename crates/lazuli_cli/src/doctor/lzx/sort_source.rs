//! `lzx-sort-source-accepts` — sorted list views require source query inputs.
//!
//! Fires on `view list` declarations with a `sort` block when the source query
//! does not accept both `sort` and `dir` text parameters. Unknown source
//! queries are suppressed here because source resolution is owned by upstream
//! analyzer diagnostics.
//!
//! Severity: `error` per `docs/proposals/lzx-terminal-grammar.md` §3.5.

use super::ir_stub::{Feature, ListQuery, Module, QueryRef, SortDecl, View};
use super::sort_findings;

/// One `lzx-sort-source-accepts` finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub feature: String,
    pub view: String,
    pub line: usize,
    pub message: String,
}

impl Finding {
    pub const CODE: &'static str = "lzx-sort-source-accepts";

    pub fn message(view: &str, source_ref: &str, sort: &SortDecl) -> String {
        let fields = sort.allowed.join(", ");
        format!(
            "view '{view}' declares 'sort by {fields}' but source query \
             '{source_ref}' does not accept sort + dir inputs - add \
             'inputs sort: Text default \"{}\", dir: Text default \"{}\"' \
             to the query",
            sort.default_field,
            sort.default_dir.as_str(),
        )
    }
}

/// Run `lzx-sort-source-accepts` across all `.lzx` surfaces in the module.
pub fn check(module: &Module) -> Vec<Finding> {
    let mut out = Vec::new();
    for feature in &module.features {
        for surface in &feature.surfaces {
            for audience in &surface.audiences {
                for view in &audience.views {
                    check_view(module, feature, view, &mut out);
                }
            }
        }
    }
    sort_findings(&mut out, |f| {
        (f.feature.clone(), f.view.clone(), Finding::CODE, f.line)
    });
    out
}

fn check_view(module: &Module, current_feature: &Feature, view: &View, out: &mut Vec<Finding>) {
    let View::List(view) = view else {
        return;
    };
    let Some(sort) = &view.sort else {
        return;
    };
    let Some((source_feature, query)) = find_query(module, current_feature, &view.source) else {
        return;
    };

    if accepts_text_param(query, "sort") && accepts_text_param(query, "dir") {
        return;
    }

    out.push(Finding {
        feature: current_feature.name.clone(),
        view: view.name.clone(),
        line: view.line,
        message: Finding::message(&view.name, &source_ref(source_feature, &view.source), sort),
    });
}

fn find_query<'a>(
    module: &'a Module,
    current_feature: &'a Feature,
    query_ref: &QueryRef,
) -> Option<(&'a str, &'a ListQuery)> {
    let feature = match query_ref.feature.as_deref() {
        Some(name) => module
            .features
            .iter()
            .find(|feature| feature.name == name)?,
        None => current_feature,
    };
    let query = feature
        .queries
        .iter()
        .find(|query| query.name == query_ref.name)?;
    Some((feature.name.as_str(), query))
}

fn accepts_text_param(query: &ListQuery, name: &str) -> bool {
    query
        .params
        .iter()
        .any(|param| param.name == name && is_text_type(&param.type_label))
}

fn is_text_type(type_label: &str) -> bool {
    matches!(type_label.trim(), "Text" | "String")
}

fn source_ref(feature: &str, query_ref: &QueryRef) -> String {
    match query_ref.feature.as_deref() {
        Some(_) => format!("{feature}.query.{}", query_ref.name),
        None => query_ref.name.clone(),
    }
}

impl super::ir_stub::SortDir {
    fn as_str(self) -> &'static str {
        match self {
            Self::Asc => "asc",
            Self::Desc => "desc",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::ir_stub::*;
    use super::*;

    fn slot(name: &str, type_label: &str) -> TypedSlot {
        TypedSlot {
            name: name.to_owned(),
            type_label: type_label.to_owned(),
            type_ref: TypeRef::Other(type_label.to_owned()),
            required: false,
        }
    }

    fn query(params: Vec<TypedSlot>) -> ListQuery {
        ListQuery {
            name: "search".to_owned(),
            input_slots: vec![],
            params,
            backs_resource: Some("Item".to_owned()),
            input_slot_meta: vec![],
        }
    }

    fn sorted_view() -> View {
        View::List(ViewList {
            name: "item_terminal".to_owned(),
            at: Some("/items".to_owned()),
            source: QueryRef {
                feature: None,
                name: "search".to_owned(),
            },
            render: ListRender::Table { columns: vec![] },
            columns: vec![],
            search: vec![],
            filter: vec![],
            search_decl: None,
            filter_decls: vec![],
            selection: None,
            sort: Some(SortDecl {
                allowed: vec!["title".to_owned(), "updated".to_owned()],
                default_field: "updated".to_owned(),
                default_dir: SortDir::Desc,
            }),
            cells: vec![],
            actions: vec![],
            drawer: None,
            line: 12,
            redacted_fields: Vec::new(),
        })
    }

    fn module_with(query: ListQuery) -> Module {
        Module {
            features: vec![Feature {
                name: "item".to_owned(),
                resources: vec![],
                queries: vec![query],
                commands: vec![],
                surfaces: vec![Surface {
                    feature: "item".to_owned(),
                    target: "web".to_owned(),
                    audiences: vec![Audience {
                        name: "admin".to_owned(),
                        requires: vec![],
                        views: vec![sorted_view()],
                        line: 1,
                    }],
                }],
            }],
        }
    }

    #[test]
    fn happy_path_sort_and_dir_text_params() {
        let module = module_with(query(vec![slot("sort", "Text"), slot("dir", "String")]));
        assert!(check(&module).is_empty());
    }

    #[test]
    fn missing_sort_param_fires() {
        let module = module_with(query(vec![slot("dir", "Text")]));
        let findings = check(&module);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].feature, "item");
        assert_eq!(findings[0].view, "item_terminal");
        assert_eq!(findings[0].line, 12);
        assert!(findings[0].message.contains("sort by title, updated"));
        assert!(findings[0].message.contains("source query 'search'"));
    }

    #[test]
    fn missing_dir_param_fires() {
        let module = module_with(query(vec![slot("sort", "Text")]));
        let findings = check(&module);
        assert_eq!(findings.len(), 1);
        assert!(
            findings[0]
                .message
                .contains("does not accept sort + dir inputs")
        );
        assert!(findings[0].message.contains("sort: Text"));
        assert!(findings[0].message.contains("dir: Text"));
    }
}
