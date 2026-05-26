//! `lzx-drawer-source-shape` — drawer source query input/output shape.
//!
//! Fires on `view list` drawer sub-views when `route <slot> from selection`
//! cannot bind to a real drawer source query input, when that input cannot
//! accept the host resource ID type, or when the drawer source query returns
//! a different resource than the host list source.
//!
//! Severity: `error` per `docs/proposals/lzx-terminal-grammar.md` §6 cell D.2.

use super::ir_stub::{
    DrawerBindingSource, Feature, ListQuery, Module, QueryInput, QueryRef, Resource, View,
};
use super::sort_findings;

/// One occurrence of the rule's diagnostic, carrying the
/// feature/view/line provenance needed to render the doctor surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub feature: String,
    pub view: String,
    pub line: usize,
    pub message: String,
}

impl Finding {
    /// Stable doctor rule code surfaced to the user.
    pub const CODE: &'static str = "lzx-drawer-source-shape";

    fn missing_input(drawer: &str, source_ref: &str, target: &str) -> String {
        format!(
            "drawer '{drawer}' source query '{source_ref}' has no input named '{target}' - \
             declared 'route {target} from selection' must bind to a real query input"
        )
    }

    fn type_mismatch(
        drawer: &str,
        source_ref: &str,
        target: &str,
        input: &str,
        host: &str,
    ) -> String {
        format!(
            "drawer '{drawer}' source query '{source_ref}' input '{target}' has type {input} \
             but host selection has type {host} - declared 'route {target} from selection' \
             must accept the host resource ID type"
        )
    }

    fn resource_mismatch(drawer: &str, drawer_type: &str, host_type: &str) -> String {
        format!(
            "drawer '{drawer}' source returns {drawer_type} but host source returns {host_type} - \
             drawer detail must show the same resource as the grid"
        )
    }
}

/// Run `lzx-drawer-source-shape` across every drawer-bearing list
/// view. Flags missing inputs, ID-type mismatches, and host/drawer
/// resource divergence.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_cli::doctor::lzx::drawer_source::check;
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
                    let View::List(list) = view else {
                        continue;
                    };
                    let Some(drawer) = &list.drawer else {
                        continue;
                    };

                    let host = resolve_query_resource(module, feature, &list.source);
                    let drawer_query = resolve_query_resource(module, feature, &drawer.source);

                    if let (
                        Some((host_feature, _host_query, host_resource)),
                        Some((drawer_feature, drawer_query, drawer_resource)),
                    ) = (host, drawer_query)
                    {
                        if let Some(binding) = &drawer.route_binding {
                            if binding.source == DrawerBindingSource::Selection {
                                let source_ref = format_query_ref(feature, &drawer.source);
                                match find_input(drawer_query, &binding.target) {
                                    Some(input)
                                        if !types_compatible(
                                            &input.type_label,
                                            &host_resource.id_type,
                                        ) =>
                                    {
                                        out.push(Finding {
                                            feature: feature.name.clone(),
                                            view: list.name.clone(),
                                            line: binding.line,
                                            message: Finding::type_mismatch(
                                                &drawer.name,
                                                &source_ref,
                                                &binding.target,
                                                &input.type_label,
                                                &host_resource.id_type,
                                            ),
                                        });
                                    }
                                    Some(_) => {}
                                    None => out.push(Finding {
                                        feature: feature.name.clone(),
                                        view: list.name.clone(),
                                        line: binding.line,
                                        message: Finding::missing_input(
                                            &drawer.name,
                                            &source_ref,
                                            &binding.target,
                                        ),
                                    }),
                                }
                            }
                        }

                        if host_feature.name != drawer_feature.name
                            || host_resource.name != drawer_resource.name
                        {
                            out.push(Finding {
                                feature: feature.name.clone(),
                                view: list.name.clone(),
                                line: drawer.line,
                                message: Finding::resource_mismatch(
                                    &drawer.name,
                                    &drawer_resource.name,
                                    &host_resource.name,
                                ),
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

fn resolve_query_resource<'a>(
    module: &'a Module,
    current_feature: &'a Feature,
    query_ref: &QueryRef,
) -> Option<(&'a Feature, &'a ListQuery, &'a Resource)> {
    let feature = match query_ref.feature.as_deref() {
        Some(name) if name != current_feature.name => {
            module.features.iter().find(|f| f.name == name)?
        }
        _ => current_feature,
    };
    let query = feature.queries.iter().find(|q| q.name == query_ref.name)?;
    let resource_name = query.backs_resource.as_deref()?;
    let resource = feature.resources.iter().find(|r| r.name == resource_name)?;
    Some((feature, query, resource))
}

fn find_input<'a>(query: &'a ListQuery, target: &str) -> Option<&'a QueryInput> {
    query.input_slots.iter().find(|slot| slot.name == target)
}

fn types_compatible(input_type: &str, host_id_type: &str) -> bool {
    normalize_type(input_type) == normalize_type(host_id_type)
}

fn normalize_type(value: &str) -> String {
    value
        .chars()
        .filter(|c| !c.is_ascii_whitespace())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

fn format_query_ref(current_feature: &Feature, query_ref: &QueryRef) -> String {
    match query_ref.feature.as_deref() {
        Some(feature) if feature != current_feature.name => {
            format!("{feature}.query.{}", query_ref.name)
        }
        _ => format!("query.{}", query_ref.name),
    }
}

#[cfg(test)]
mod tests {
    use super::super::ir_stub::*;
    use super::*;

    fn resource(name: &str, id_type: &str) -> Resource {
        Resource {
            name: name.to_owned(),
            id_type: id_type.to_owned(),
            fields: vec!["id".into(), "title".into()],
        }
    }

    fn input(name: &str, type_label: &str) -> QueryInput {
        QueryInput {
            name: name.to_owned(),
            type_label: type_label.to_owned(),
        }
    }

    fn query(name: &str, backs_resource: &str, input_slots: Vec<QueryInput>) -> ListQuery {
        ListQuery {
            name: name.to_owned(),
            input_slots,
            backs_resource: Some(backs_resource.to_owned()),
            input_slot_meta: vec![],
            params: vec![],
        }
    }

    fn qref(name: &str) -> QueryRef {
        QueryRef {
            feature: None,
            name: name.to_owned(),
        }
    }

    fn view(drawer_source: &str, target: &str) -> View {
        View::List(ViewList {
            name: "item_terminal".into(),
            at: Some("/items".into()),
            source: qref("search"),
            render: ListRender::Table {
                columns: vec!["id".into(), "title".into()],
            },
            columns: vec!["id".into(), "title".into()],
            search: vec![],
            filter: vec![],
            search_decl: None,
            filter_decls: vec![],
            selection: None,
            sort: None,
            cells: vec![],
            actions: vec![],
            drawer: Some(DrawerSubView {
                name: "item_detail".into(),
                source: qref(drawer_source),
                route_binding: Some(DrawerRouteBinding {
                    target: target.to_owned(),
                    source: DrawerBindingSource::Selection,
                    line: 16,
                }),
                line: 14,
            }),
            line: 10,
        })
    }

    fn module(resources: Vec<Resource>, queries: Vec<ListQuery>, view: View) -> Module {
        Module {
            features: vec![Feature {
                name: "item".into(),
                resources,
                queries,
                commands: vec![],
                surfaces: vec![Surface {
                    feature: "item".into(),
                    target: "web".into(),
                    audiences: vec![Audience {
                        name: "admin".into(),
                        requires: vec![],
                        views: vec![view],
                        line: 3,
                    }],
                }],
            }],
        }
    }

    #[test]
    fn missing_input_fires() {
        let module = module(
            vec![resource("Item", "ID")],
            vec![
                query("search", "Item", vec![]),
                query("by_id", "Item", vec![input("id", "ID")]),
            ],
            view("by_id", "key"),
        );

        let findings = check(&module);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].line, 16);
        assert!(findings[0].message.contains("has no input named 'key'"));
    }

    #[test]
    fn type_mismatch_fires() {
        let module = module(
            vec![resource("Item", "Item.ID")],
            vec![
                query("search", "Item", vec![]),
                query("by_id", "Item", vec![input("key", "Other.ID")]),
            ],
            view("by_id", "key"),
        );

        let findings = check(&module);
        assert_eq!(findings.len(), 1);
        assert!(findings[0].message.contains("has type Other.ID"));
        assert!(
            findings[0]
                .message
                .contains("host selection has type Item.ID")
        );
    }

    #[test]
    fn resource_mismatch_fires() {
        let module = module(
            vec![resource("Item", "ID"), resource("Other", "ID")],
            vec![
                query("search", "Item", vec![]),
                query("other_by_id", "Other", vec![input("key", "ID")]),
            ],
            view("other_by_id", "key"),
        );

        let findings = check(&module);
        assert_eq!(findings.len(), 1);
        assert!(
            findings[0]
                .message
                .contains("source returns Other but host source returns Item")
        );
    }

    #[test]
    fn happy_path_accepts_matching_input_and_resource() {
        let module = module(
            vec![resource("Item", "Item.ID")],
            vec![
                query("search", "Item", vec![]),
                query("by_id", "Item", vec![input("key", "Item.ID")]),
            ],
            view("by_id", "key"),
        );

        assert!(check(&module).is_empty());
    }
}
