//! Test fixtures shared between `lzx` and the per-view emitter
//! test modules. Encodes the L0 #3 §13.1 `slug.web.lzx` example
//! verbatim.

use super::ir::*;

pub fn type_badge_cell() -> CellBinding {
    CellBinding {
        field: "tags".to_owned(),
        slot: "type_badge".to_owned(),
    }
}

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
        span_ref: None,
    }
}

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
        span_ref: None,
    }
}

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
        span_ref: None,
    }
}

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
        span_ref: None,
    }
}

pub fn public_audience() -> Audience {
    Audience {
        name: "public".to_owned(),
        requires: vec![PolicyAtom {
            namespace: "scope".to_owned(),
            name: "workspace_member".to_owned(),
            args: None,
        }],
        views: vec![View::List(public_slug_list_view())],
        span_ref: None,
    }
}

pub fn slug_web_surface() -> Surface {
    Surface {
        feature: "slug".to_owned(),
        target: SurfaceTarget::Web,
        audiences: vec![admin_audience(), public_audience()],
        span_ref: None,
    }
}
