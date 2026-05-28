//! Test fixtures shared between `lzx` and the per-view emitter
//! test modules. Encodes the L0 #3 §13.1 `slug.web.lzx` example
//! verbatim.
//!
//! Centralising the fixtures keeps the LZX view emitters and their
//! snapshot tests agreeing on the canonical example. Adding a new
//! emitter test should reuse one of these builders rather than
//! re-encoding the IR shape by hand — drift between the spec sample
//! and the test fixture has bitten this crate before.

use super::ir::*;

/// `CellBinding` that maps the `tags` field onto the `type_badge`
/// slot — used by every view in the slug surface example.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_codegen_ts::lzx::test_fixtures::type_badge_cell;
/// let cell = type_badge_cell();
/// assert_eq!(cell.field, "tags");
/// ```
pub fn type_badge_cell() -> CellBinding {
    CellBinding {
        field: "tags".to_owned(),
        slot: "type_badge".to_owned(),
    }
}

/// The L0 §13.1 `slug_list` view — table layout with search, a
/// `tags` filter, and the full CRUD action set.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_codegen_ts::lzx::test_fixtures::slug_list_view;
/// let view = slug_list_view();
/// assert_eq!(view.name, "slug_list");
/// ```
pub fn slug_list_view() -> ViewList {
    ViewList {
        name: "slug_list".to_owned(),
        route: Some("/slugs".to_owned()),
        source: QueryRef {
            feature: "slug".to_owned(),
            kind: QueryKind::List,
            name: "mine".to_owned(),
        },
        render: ListRender::Table {
            columns: vec![
                "key".to_owned(),
                "title".to_owned(),
                "tags".to_owned(),
                "created_at".to_owned(),
            ],
        },
        search: Some(SearchDecl {
            mode: SearchMode::Columns {
                columns: vec!["key".to_owned(), "title".to_owned()],
            },
            fields: vec![],
            free_text_target: None,
            span_ref: None,
        }),
        filter: vec![FilterDecl {
            name: "tags".to_owned(),
            type_ref: String::new(),
            cardinality: FilterCardinality::Single,
            url_sync: false,
            span_ref: None,
        }],
        cells: vec![type_badge_cell()],
        actions: vec![
            CommandRef {
                feature: "slug".to_owned(),
                name: "create".to_owned(),
            },
            CommandRef {
                feature: "slug".to_owned(),
                name: "update".to_owned(),
            },
            CommandRef {
                feature: "slug".to_owned(),
                name: "delete".to_owned(),
            },
        ],
        drawer: None,
        sort: None,
        selection: None,
        settings: vec![],
        redacted_fields: Vec::new(),
        ux: Default::default(),
        span_ref: None,
    }
}

/// The L0 §13.1 `slug_detail` view — keyed by `:key`, three
/// sections, update + delete actions.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_codegen_ts::lzx::test_fixtures::slug_detail_view;
/// let view = slug_detail_view();
/// assert_eq!(view.name, "slug_detail");
/// ```
pub fn slug_detail_view() -> ViewDetail {
    ViewDetail {
        name: "slug_detail".to_owned(),
        route: Some("/slugs/:key".to_owned()),
        source: QueryRef {
            feature: "slug".to_owned(),
            kind: QueryKind::Lookup,
            name: "by_key".to_owned(),
        },
        route_params: vec![RouteParam {
            name: "key".to_owned(),
            type_ref: "Text".to_owned(),
        }],
        sections: vec![
            "header".to_owned(),
            "metadata".to_owned(),
            "related_items".to_owned(),
        ],
        cells: vec![type_badge_cell()],
        actions: vec![
            CommandRef {
                feature: "slug".to_owned(),
                name: "update".to_owned(),
            },
            CommandRef {
                feature: "slug".to_owned(),
                name: "delete".to_owned(),
            },
        ],
        redacted_fields: Vec::new(),
        ux: Default::default(),
        span_ref: None,
    }
}

/// The L0 §13.1 `slug_create` view — bound to the `create`
/// command, fields drawn from the resource.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_codegen_ts::lzx::test_fixtures::slug_create_view;
/// let view = slug_create_view();
/// assert_eq!(view.name, "slug_create");
/// ```
pub fn slug_create_view() -> ViewCreate {
    ViewCreate {
        name: "slug_create".to_owned(),
        route: Some("/slugs/new".to_owned()),
        submit: CommandRef {
            feature: "slug".to_owned(),
            name: "create".to_owned(),
        },
        on_success: None,
        fields: vec![
            "key".to_owned(),
            "title".to_owned(),
            "description".to_owned(),
            "tags".to_owned(),
        ],
        cells: vec![type_badge_cell()],
        redacted_fields: Vec::new(),
        span_ref: None,
    }
}

/// The read-only `public_slug_list` view shown to non-admin audiences
/// — same data source, narrower columns, no actions.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_codegen_ts::lzx::test_fixtures::public_slug_list_view;
/// let view = public_slug_list_view();
/// assert!(view.actions.is_empty());
/// ```
pub fn public_slug_list_view() -> ViewList {
    ViewList {
        name: "public_slug_list".to_owned(),
        route: Some("/browse".to_owned()),
        source: QueryRef {
            feature: "slug".to_owned(),
            kind: QueryKind::List,
            name: "mine".to_owned(),
        },
        render: ListRender::Table {
            columns: vec!["key".to_owned(), "title".to_owned()],
        },
        search: Some(SearchDecl {
            mode: SearchMode::Columns {
                columns: vec!["key".to_owned(), "title".to_owned()],
            },
            fields: vec![],
            free_text_target: None,
            span_ref: None,
        }),
        filter: vec![],
        cells: vec![],
        actions: vec![],
        drawer: None,
        sort: None,
        selection: None,
        settings: vec![],
        redacted_fields: Vec::new(),
        ux: Default::default(),
        span_ref: None,
    }
}

/// The `admin` audience scoped to `workspace_admin`, holding the
/// full list/detail/create surface.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_codegen_ts::lzx::test_fixtures::admin_audience;
/// let audience = admin_audience();
/// assert_eq!(audience.name, "admin");
/// ```
pub fn admin_audience() -> Audience {
    Audience {
        name: "admin".to_owned(),
        requires: vec![PolicyAtom {
            namespace: "scope".to_owned(),
            name: "workspace_admin".to_owned(),
            args: None,
        }],
        views: vec![
            View::List(slug_list_view()),
            View::Detail(slug_detail_view()),
            View::Create(slug_create_view()),
        ],
        ux: Default::default(),
        span_ref: None,
    }
}

/// The `public` audience scoped to any `workspace_member`, holding
/// only the read-only list view.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_codegen_ts::lzx::test_fixtures::public_audience;
/// let audience = public_audience();
/// assert_eq!(audience.name, "public");
/// ```
pub fn public_audience() -> Audience {
    Audience {
        name: "public".to_owned(),
        requires: vec![PolicyAtom {
            namespace: "scope".to_owned(),
            name: "workspace_member".to_owned(),
            args: None,
        }],
        views: vec![View::List(public_slug_list_view())],
        ux: Default::default(),
        span_ref: None,
    }
}

/// The full `slug.web.lzx` [`Surface`] — web target, two audiences,
/// drives every snapshot in the LZX emitter suite.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_codegen_ts::lzx::test_fixtures::slug_web_surface;
/// let surface = slug_web_surface();
/// assert_eq!(surface.audiences.len(), 2);
/// ```
pub fn slug_web_surface() -> Surface {
    Surface {
        feature: "slug".to_owned(),
        target: SurfaceTarget::Web,
        audiences: vec![admin_audience(), public_audience()],
        span_ref: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_web_surface_carries_both_audiences() {
        let surface = slug_web_surface();
        assert_eq!(surface.feature, "slug");
        assert_eq!(surface.audiences.len(), 2);
        assert_eq!(surface.audiences[0].name, "admin");
        assert_eq!(surface.audiences[1].name, "public");
    }

    #[test]
    fn slug_list_view_has_table_render_and_search() {
        let view = slug_list_view();
        assert_eq!(view.name, "slug_list");
        assert!(view.search.is_some());
        assert!(matches!(view.render, ListRender::Table { .. }));
    }
}
