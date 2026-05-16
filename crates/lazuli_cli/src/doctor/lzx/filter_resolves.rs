//! `lzx-filter-type-resolves` — typed list filters must resolve.
//!
//! For each `FilterDecl` on a `view list`, the authored `type_ref` must be
//! either:
//!
//! - an enum type carried by at least one field on the resource backing the
//!   view's `source`, or
//! - a known scalar label from Lazuli's scalar catalog.
//!
//! Unknown source queries / backing resources are suppressed; query resolution
//! diagnostics belong to the analyzer/source-resource rules.
//!
//! Severity: `error` per `docs/proposals/lzx-terminal-grammar.md` §3.3.

use lazuli_ir::{BuiltinType, Feature, Field, Module, Query, QueryRef, Resource, TypeRef, View};

use super::sort_findings;

/// One `lzx-filter-type-resolves` finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub feature: String,
    pub view: String,
    pub line: usize,
    pub message: String,
}

impl Finding {
    pub const CODE: &'static str = "lzx-filter-type-resolves";

    pub fn message(filter: &str, type_ref: &str, resource: &str) -> String {
        format!(
            "filter '{filter}' type '{type_ref}' does not resolve to an enum on resource \
             {resource} or a known scalar (Text, Integer, Decimal, Bool, Date, Time, Json)"
        )
    }
}

/// Run `lzx-filter-type-resolves` across all `.lzx` surfaces in the module.
pub fn check(module: &Module) -> Vec<Finding> {
    let mut out = Vec::new();
    for feature in &module.features {
        for surface in &feature.surfaces {
            for audience in &surface.audiences {
                for view in &audience.views {
                    let View::List(list) = view else {
                        continue;
                    };
                    let Some(source_feature) = module
                        .features
                        .iter()
                        .find(|candidate| candidate.name == list.source.feature)
                    else {
                        continue;
                    };
                    let Some(resource) = find_resource_for_query(source_feature, &list.source)
                    else {
                        continue;
                    };

                    for filter in &list.filter {
                        if !resolves_filter_type(source_feature, resource, &filter.type_ref) {
                            out.push(Finding {
                                feature: feature.name.clone(),
                                view: list.name.clone(),
                                line: filter.span_ref.map(|span| span.start).unwrap_or(0),
                                message: Finding::message(
                                    &filter.name,
                                    &filter.type_ref,
                                    &resource.name,
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

fn resolves_filter_type(feature: &Feature, resource: &Resource, type_ref: &str) -> bool {
    is_known_scalar(type_ref)
        || resource
            .fields
            .iter()
            .any(|field| field_type_is_resource_enum(feature, field, type_ref))
}

fn field_type_is_resource_enum(feature: &Feature, field: &Field, type_ref: &str) -> bool {
    let type_name = match &field.type_ref {
        TypeRef::EnumRef(qname) | TypeRef::UserDefined(qname) => qname.name.as_str(),
        _ => return false,
    };
    type_name == type_ref && feature.enums.iter().any(|decl| decl.name == type_ref)
}

fn is_known_scalar(type_ref: &str) -> bool {
    matches!(
        scalar_from_label(type_ref),
        Some(
            BuiltinType::Text
                | BuiltinType::Integer
                | BuiltinType::Decimal
                | BuiltinType::Boolean
                | BuiltinType::Date
                | BuiltinType::DateTime
                | BuiltinType::Json
        )
    )
}

fn scalar_from_label(type_ref: &str) -> Option<BuiltinType> {
    match type_ref {
        "Text" => Some(BuiltinType::Text),
        "Integer" | "Int" => Some(BuiltinType::Integer),
        "Decimal" => Some(BuiltinType::Decimal),
        "Boolean" | "Bool" => Some(BuiltinType::Boolean),
        "Date" => Some(BuiltinType::Date),
        "DateTime" | "Time" => Some(BuiltinType::DateTime),
        "Json" | "JSON" => Some(BuiltinType::Json),
        _ => None,
    }
}

fn find_resource_for_query<'a>(feature: &'a Feature, query_ref: &QueryRef) -> Option<&'a Resource> {
    let query = feature
        .queries
        .iter()
        .find(|query| query_matches_ref(query, query_ref))?;

    if let Query::Sql(sql) = query {
        if let Some(resource_name) = resource_name_from_type_ref(&sql.returns) {
            if let Some(resource) = feature
                .resources
                .iter()
                .find(|resource| resource.name == resource_name)
            {
                return Some(resource);
            }
        }
    }

    if feature.resources.len() == 1 {
        return feature.resources.first();
    }

    None
}

fn query_matches_ref(query: &Query, query_ref: &QueryRef) -> bool {
    query.name() == query_ref.name
}

fn resource_name_from_type_ref(type_ref: &TypeRef) -> Option<&str> {
    match type_ref {
        TypeRef::UserDefined(qname) => Some(qname.name.as_str()),
        TypeRef::Many(inner) => resource_name_from_type_ref(inner),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use lazuli_ir::{
        Audience, Defaults, EnumDecl, FieldConstraints, FilterCardinality, FilterDecl, ListQuery,
        ListRender, Policies, QualifiedName, SpanRef, Surface, SurfaceTarget, ViewList,
    };

    use super::*;

    fn qname(name: &str) -> QualifiedName {
        QualifiedName {
            feature: None,
            name: name.to_owned(),
        }
    }

    fn enum_field(name: &str, type_name: &str) -> Field {
        field(name, TypeRef::EnumRef(qname(type_name)))
    }

    fn text_field(name: &str) -> Field {
        field(name, TypeRef::Builtin(BuiltinType::Text))
    }

    fn field(name: &str, type_ref: TypeRef) -> Field {
        Field {
            name: name.to_owned(),
            type_ref,
            required: true,
            unique: false,
            slug: false,
            default: None,
            derived_from: None,
            constraints: FieldConstraints::default(),
            full_text: false,
            previous_names: Vec::new(),
            span_ref: None,
        }
    }

    fn filter(name: &str, type_ref: &str) -> FilterDecl {
        FilterDecl {
            name: name.to_owned(),
            type_ref: type_ref.to_owned(),
            cardinality: FilterCardinality::Single,
            url_sync: false,
            span_ref: Some(SpanRef { start: 42, end: 48 }),
        }
    }

    fn module_with_filter(
        type_ref: &str,
        resource_fields: Vec<Field>,
        enums: Vec<EnumDecl>,
    ) -> Module {
        Module {
            workspace: None,
            contracts: Vec::new(),
            app: None,
            registry: None,
            profiles: Vec::new(),
            design: None,
            rbac: None,
            features: vec![Feature {
                name: "item".to_owned(),
                purpose: None,
                non_goals: Vec::new(),
                context_path: None,
                defaults: Defaults::default(),
                uses: Vec::new(),
                uses_spans: Vec::new(),
                requirements: Vec::new(),
                enums,
                resources: vec![Resource {
                    name: "Item".to_owned(),
                    public_contract: None,
                    tenancy: None,
                    soft_delete: false,
                    timestamps: None,
                    fields: resource_fields,
                    constraints: Vec::new(),
                    validate: None,
                    validates: Vec::new(),
                    retention: None,
                    previous_names: Vec::new(),
                    span_ref: None,
                    lifecycle: None,
                    invariants: vec![],

                    lock: None,

                    composite_key: None,
                }],
                events: Vec::new(),
                rules: Vec::new(),
                policies: Policies::default(),
                commands: Vec::new(),
                apis: Vec::new(),
                records: Vec::new(),
                queries: vec![Query::List(ListQuery {
                    name: "search".to_owned(),
                    public_contract: None,
                    params: Vec::new(),
                    scope: Vec::new(),
                    scope_override: false,
                    filters: Vec::new(),
                    order: Vec::new(),
                    paginate: None,
                    modifier: None,
                    cache: None,
                    previous_names: Vec::new(),
                    span_ref: None,
                })],
                workflows: Vec::new(),
                jobs: Vec::new(),
                webhooks: Vec::new(),
                notifications: Vec::new(),
                event_groups: Vec::new(),
                tenant_migrations: Vec::new(),
                translation: None,
                pollers: vec![],
                auth: None,
                surfaces: vec![Surface {
                    feature: "item".to_owned(),
                    target: SurfaceTarget::Web,
                    audiences: vec![Audience {
                        name: "admin".to_owned(),
                        requires: Vec::new(),
                        views: vec![View::List(ViewList {
                            name: "item_terminal".to_owned(),
                            route: Some("/".to_owned()),
                            source: QueryRef {
                                feature: "item".to_owned(),
                                kind: lazuli_ir::QueryKind::List,
                                name: "search".to_owned(),
                            },
                            render: ListRender::Table {
                                columns: vec!["title".to_owned()],
                            },
                            search: None,
                            filter: vec![filter("kind", type_ref)],
                            cells: Vec::new(),
                            actions: Vec::new(),
                            drawer: None,
                            sort: None,
                            selection: None,
                            settings: Vec::new(),
                            span_ref: None,
                        })],
                        span_ref: None,
                    }],
                    span_ref: None,
                }],
                extensions: Vec::new(),
                escape_routes: Vec::new(),
                agents: Vec::new(),
                reports: Vec::new(),
                channels: Vec::new(),
            caches: Vec::new(),
                aggregates: vec![],
                previous_names: Vec::new(),
                span_ref: None,
            }],
        }
    }

    fn enum_decl(name: &str) -> EnumDecl {
        EnumDecl {
            name: name.to_owned(),
            public_contract: None,
            variants: Vec::new(),
            previous_names: Vec::new(),
            span_ref: None,
        }
    }

    #[test]
    fn enum_on_resource_happy_path() {
        let module = module_with_filter(
            "ItemType",
            vec![text_field("title"), enum_field("kind", "ItemType")],
            vec![enum_decl("ItemType")],
        );

        assert!(check(&module).is_empty());
    }

    #[test]
    fn scalar_happy_path() {
        let module = module_with_filter("Text", vec![text_field("title")], Vec::new());

        assert!(check(&module).is_empty());
    }

    #[test]
    fn unknown_type_errors() {
        let module = module_with_filter("GhostType", vec![text_field("title")], Vec::new());

        let findings = check(&module);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].feature, "item");
        assert_eq!(findings[0].view, "item_terminal");
        assert!(
            findings[0]
                .message
                .contains("filter 'kind' type 'GhostType'")
        );
        assert!(findings[0].message.contains("resource Item"));
    }

    #[test]
    fn case_mismatch_errors() {
        let module = module_with_filter("text", vec![text_field("title")], Vec::new());

        let findings = check(&module);
        assert_eq!(findings.len(), 1);
        assert!(findings[0].message.contains("type 'text'"));
    }
}
