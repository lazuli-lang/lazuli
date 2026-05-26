//! `lzx-search-binds-target-exists` and
//! `lzx-search-field-multi-cardinality` — segmented search field bindings
//! resolve to host-view state with known cardinality.
//!
//! The rule is deliberately structural: filter bindings resolve against the
//! host view's typed filters, source bindings resolve against the host
//! source query's input slots, and `selection` is only scalar in single-select
//! mode. Once a filter binding resolves, cardinality is known by construction;
//! source input bindings additionally require preserved cardinality metadata so
//! TypeScript codegen can compute `search-query-parser`'s `alwaysArray`.

use super::ir_stub::{BindingRef, Feature, Module, QueryInputSlot, SearchDecl, View, ViewList};
use super::sort_findings;

/// One `lzx-search-*` finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub feature: String,
    pub view: String,
    pub line: usize,
    pub code: &'static str,
    pub message: String,
}

impl Finding {
    /// Doctor rule code for unbound search targets.
    pub const BINDS_TARGET_EXISTS: &'static str = "lzx-search-binds-target-exists";
    /// Doctor rule code for missing-cardinality search bindings.
    pub const FIELD_MULTI_CARDINALITY: &'static str = "lzx-search-field-multi-cardinality";

    /// Render the diagnostic for an unbound search target.
    pub fn missing_target_message(key: &str, binding: &BindingRef) -> String {
        format!(
            "search field '{key}' binds to '{}' which is not declared on the view",
            display_binding_ref(binding)
        )
    }

    /// Render the diagnostic when a search target binds without
    /// cardinality metadata.
    pub fn missing_cardinality_message(key: &str, binding: &BindingRef) -> String {
        format!(
            "search field '{key}' binds to '{}' but the target has no cardinality metadata",
            display_binding_ref(binding)
        )
    }
}

/// Run the segmented-search binding checks across all `.lzx` surfaces.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_cli::doctor::lzx::search_binds::check;
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
                        check_list(feature, list, &mut out);
                    }
                }
            }
        }
    }
    sort_findings(&mut out, |f| {
        (f.feature.clone(), f.view.clone(), f.code, f.line)
    });
    out
}

fn check_list(feature: &Feature, view: &ViewList, out: &mut Vec<Finding>) {
    let Some(search) = view.search_decl.as_ref() else {
        return;
    };
    for field in &search.fields {
        check_binding(feature, view, search, &field.key, &field.binds_to, out);
    }
    if let Some(binding) = &search.free_text_target {
        check_binding(feature, view, search, "free_text", binding, out);
    }
}

fn check_binding(
    feature: &Feature,
    view: &ViewList,
    _search: &SearchDecl,
    key: &str,
    binding: &BindingRef,
    out: &mut Vec<Finding>,
) {
    match binding {
        BindingRef::Filter { name } => {
            if !view.filter_decls.iter().any(|f| f.name == *name) {
                push_missing_target(feature, view, key, binding, out);
            }
        }
        BindingRef::SourceInput { name } => match source_input(feature, view, name) {
            Some(slot) if slot.cardinality.is_some() => {}
            Some(_) => out.push(Finding {
                feature: feature.name.clone(),
                view: view.name.clone(),
                line: view.line,
                code: Finding::FIELD_MULTI_CARDINALITY,
                message: Finding::missing_cardinality_message(key, binding),
            }),
            None => push_missing_target(feature, view, key, binding, out),
        },
        BindingRef::SelectionScalar => {
            let single = view
                .selection
                .as_ref()
                .is_some_and(|selection| selection.mode == super::ir_stub::SelectionMode::Single);
            if !single {
                push_missing_target(feature, view, key, binding, out);
            }
        }
    }
}

fn source_input<'a>(
    feature: &'a Feature,
    view: &ViewList,
    name: &str,
) -> Option<&'a QueryInputSlot> {
    feature
        .queries
        .iter()
        .find(|q| q.name == view.source.name)
        .and_then(|query| query.input_slot_meta.iter().find(|slot| slot.name == name))
}

fn push_missing_target(
    feature: &Feature,
    view: &ViewList,
    key: &str,
    binding: &BindingRef,
    out: &mut Vec<Finding>,
) {
    out.push(Finding {
        feature: feature.name.clone(),
        view: view.name.clone(),
        line: view.line,
        code: Finding::BINDS_TARGET_EXISTS,
        message: Finding::missing_target_message(key, binding),
    });
}

fn display_binding_ref(binding: &BindingRef) -> String {
    match binding {
        BindingRef::Filter { name } => format!("filters.{name}"),
        BindingRef::SourceInput { name } => format!("source.{name}"),
        BindingRef::SelectionScalar => "selection".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::super::ir_stub::*;
    use super::*;

    fn slot(name: &str, cardinality: Option<SearchTargetCardinality>) -> QueryInputSlot {
        QueryInputSlot {
            name: name.to_owned(),
            cardinality,
        }
    }

    fn filter(name: &str, cardinality: SearchTargetCardinality) -> FilterDecl {
        FilterDecl {
            name: name.to_owned(),
            cardinality,
        }
    }

    fn field(key: &str, binds_to: BindingRef) -> SearchField {
        SearchField {
            key: key.to_owned(),
            binds_to,
        }
    }

    fn list(
        fields: Vec<SearchField>,
        filters: Vec<FilterDecl>,
        source_inputs: Vec<QueryInputSlot>,
        selection: Option<SelectionMode>,
    ) -> Module {
        Module {
            features: vec![Feature {
                name: "item".into(),
                resources: vec![],
                queries: vec![ListQuery {
                    name: "search".into(),
                    backs_resource: None,
                    input_slots: vec![],
                    input_slot_meta: source_inputs,
                    params: vec![],
                }],
                commands: vec![],
                surfaces: vec![Surface {
                    feature: "item".into(),
                    target: "web".into(),
                    audiences: vec![Audience {
                        name: "admin".into(),
                        requires: vec![],
                        views: vec![View::List(ViewList {
                            name: "terminal".into(),
                            at: Some("/items".into()),
                            source: QueryRef {
                                feature: None,
                                name: "search".into(),
                            },
                            render: ListRender::Table { columns: vec![] },
                            columns: vec![],
                            search: vec![],
                            filter: vec![],
                            search_decl: Some(SearchDecl {
                                fields,
                                free_text_target: None,
                            }),
                            filter_decls: filters,
                            selection: selection.map(|mode| SelectionDecl {
                                mode,
                                bulk_actions: vec![],
                            }),
                            sort: None,
                            cells: vec![],
                            actions: vec![],
                            drawer: None,
                            line: 12,
                        })],
                        line: 3,
                    }],
                }],
            }],
        }
    }

    #[test]
    fn filter_binding_ok() {
        let module = list(
            vec![field(
                "tag",
                BindingRef::Filter {
                    name: "tags".into(),
                },
            )],
            vec![filter("tags", SearchTargetCardinality::Multi)],
            vec![],
            None,
        );
        assert!(check(&module).is_empty());
    }

    #[test]
    fn filter_binding_missing() {
        let module = list(
            vec![field(
                "tag",
                BindingRef::Filter {
                    name: "tags".into(),
                },
            )],
            vec![],
            vec![],
            None,
        );
        let findings = check(&module);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, Finding::BINDS_TARGET_EXISTS);
        assert_eq!(
            findings[0].message,
            "search field 'tag' binds to 'filters.tags' which is not declared on the view"
        );
    }

    #[test]
    fn source_binding_ok() {
        let module = list(
            vec![field("q", BindingRef::SourceInput { name: "q".into() })],
            vec![],
            vec![slot("q", Some(SearchTargetCardinality::Single))],
            None,
        );
        assert!(check(&module).is_empty());
    }

    #[test]
    fn source_binding_missing() {
        let module = list(
            vec![field("q", BindingRef::SourceInput { name: "q".into() })],
            vec![],
            vec![],
            None,
        );
        let findings = check(&module);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, Finding::BINDS_TARGET_EXISTS);
        assert!(findings[0].message.contains("'source.q'"));
    }

    #[test]
    fn selection_scalar_with_single_mode_ok() {
        let module = list(
            vec![field("selected", BindingRef::SelectionScalar)],
            vec![],
            vec![],
            Some(SelectionMode::Single),
        );
        assert!(check(&module).is_empty());
    }

    #[test]
    fn selection_scalar_with_multi_mode_errors() {
        let module = list(
            vec![field("selected", BindingRef::SelectionScalar)],
            vec![],
            vec![],
            Some(SelectionMode::Multi),
        );
        let findings = check(&module);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, Finding::BINDS_TARGET_EXISTS);
        assert!(findings[0].message.contains("'selection'"));
    }

    #[test]
    fn source_binding_without_cardinality_metadata_errors() {
        let module = list(
            vec![field(
                "tag",
                BindingRef::SourceInput {
                    name: "tags".into(),
                },
            )],
            vec![],
            vec![slot("tags", None)],
            None,
        );
        let findings = check(&module);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, Finding::FIELD_MULTI_CARDINALITY);
    }
}
