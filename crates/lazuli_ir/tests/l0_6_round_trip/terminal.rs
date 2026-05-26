//! L0 #6 — terminal render / search / sort / selection / setting / drawer /
//! filter / on-success serde round-trips.

use serde_json::json;

use lazuli_ir::{
    BindingRef, CellBinding, DrawerBindingSource, DrawerRouteBinding, DrawerSubView, DrawerTrigger,
    FilterCardinality, FilterDecl, FlashSpec, InvalidatesSpec, ListRender, OnSuccessSpec,
    QualifiedName, SearchDecl, SearchField, SearchMode, SelectionDecl, SelectionMode, SettingDecl,
    SettingPersistence, SettingValueSpace, SortDecl, SortDir, SpanRef, TranslationKeyRef,
};

use super::{command_ref, query_ref, round_trip};

#[test]
fn terminal_render_drawer_and_filter_ir_round_trip() {
    round_trip(&ListRender::Table {
        columns: vec!["title".to_string(), "updated".to_string()],
    });
    round_trip(&ListRender::Cells {
        slot: "item_card".to_string(),
    });

    let route_binding = DrawerRouteBinding {
        target: "key".to_string(),
        source: DrawerBindingSource::Selection,
    };
    round_trip(&route_binding);
    round_trip(&DrawerTrigger::ManualOpen);
    round_trip(&DrawerBindingSource::Selection);
    round_trip(&DrawerSubView {
        name: "item_detail".to_string(),
        trigger: DrawerTrigger::Select,
        source: query_ref("by_id"),
        route_binding: Some(route_binding),
        sections: vec!["header".to_string(), "metadata".to_string()],
        cells: vec![CellBinding {
            field: "related".to_string(),
            slot: "related_items".to_string(),
        }],
        actions: vec![command_ref("update"), command_ref("delete")],
        span_ref: Some(SpanRef { start: 10, end: 42 }),
    });

    round_trip(&FilterCardinality::Multi);
    round_trip(&FilterDecl {
        name: "tags".to_string(),
        type_ref: "Text".to_string(),
        cardinality: FilterCardinality::Multi,
        url_sync: true,
        span_ref: Some(SpanRef { start: 44, end: 60 }),
    });
}

#[test]
fn terminal_search_sort_selection_and_setting_ir_round_trip() {
    let filter_binding = BindingRef::Filter {
        name: "slug".to_string(),
    };
    let source_binding = BindingRef::SourceInput {
        name: "q".to_string(),
    };
    round_trip(&filter_binding);
    round_trip(&source_binding);
    round_trip(&BindingRef::SelectionScalar);

    round_trip(&SearchMode::Columns {
        columns: vec!["title".to_string()],
    });
    round_trip(&SearchMode::Segmented);
    round_trip(&SearchField {
        key: "slug".to_string(),
        binds_to: filter_binding.clone(),
        span_ref: Some(SpanRef { start: 70, end: 88 }),
    });
    round_trip(&SearchDecl {
        mode: SearchMode::Segmented,
        fields: vec![SearchField {
            key: "slug".to_string(),
            binds_to: filter_binding,
            span_ref: None,
        }],
        free_text_target: Some(source_binding),
        span_ref: Some(SpanRef { start: 62, end: 96 }),
    });

    round_trip(&SortDir::Desc);
    round_trip(&SortDecl {
        allowed: vec!["title".to_string(), "updated".to_string()],
        default_field: "updated".to_string(),
        default_dir: SortDir::Desc,
        span_ref: Some(SpanRef {
            start: 100,
            end: 120,
        }),
    });

    round_trip(&SelectionMode::Multi);
    round_trip(&SelectionDecl {
        mode: SelectionMode::Multi,
        bulk_actions: vec![command_ref("delete")],
        span_ref: Some(SpanRef {
            start: 130,
            end: 150,
        }),
    });

    round_trip(&SettingValueSpace::Enum {
        values: vec!["sm".to_string(), "md".to_string(), "lg".to_string()],
    });
    round_trip(&SettingValueSpace::Bool);
    round_trip(&SettingValueSpace::Int { min: 1, max: 12 });
    round_trip(&SettingPersistence::Workspace);
    round_trip(&SettingDecl {
        name: "grid_size".to_string(),
        value_space: SettingValueSpace::Enum {
            values: vec!["sm".to_string(), "md".to_string(), "lg".to_string()],
        },
        default: "sm".to_string(),
        persistence: SettingPersistence::Local,
        span_ref: Some(SpanRef {
            start: 160,
            end: 190,
        }),
    });
}

#[test]
fn terminal_tagged_enums_use_kind_discriminators() {
    assert_eq!(
        serde_json::to_value(ListRender::Cells {
            slot: "item_card".to_string()
        })
        .expect("serialize list render"),
        json!({ "kind": "cells", "slot": "item_card" })
    );
    assert_eq!(
        serde_json::to_value(SearchMode::Columns {
            columns: vec!["title".to_string()]
        })
        .expect("serialize search mode"),
        json!({ "kind": "columns", "columns": ["title"] })
    );
    assert_eq!(
        serde_json::to_value(BindingRef::SelectionScalar).expect("serialize binding ref"),
        json!({ "kind": "selection_scalar" })
    );
    assert_eq!(
        serde_json::to_value(SettingValueSpace::Int { min: 1, max: 9 })
            .expect("serialize setting value space"),
        json!({ "kind": "int", "min": 1, "max": 9 })
    );
}

#[test]
fn terminal_optional_and_vec_fields_default_and_skip() {
    let search_json = r#"{"mode":{"kind":"segmented"}}"#;
    let search: SearchDecl = serde_json::from_str(search_json).expect("deserialize search");
    assert_eq!(search.fields, Vec::<SearchField>::new());
    assert_eq!(search.free_text_target, None);
    assert_eq!(search.span_ref, None);
    assert_eq!(
        serde_json::to_value(search).expect("serialize search"),
        json!({ "mode": { "kind": "segmented" } })
    );

    let drawer_json = r#"{
        "name":"item_detail",
        "trigger":"select",
        "source":{"feature":"item","kind":"lookup","name":"by_id"}
    }"#;
    let drawer: DrawerSubView = serde_json::from_str(drawer_json).expect("deserialize drawer");
    assert_eq!(drawer.route_binding, None);
    assert!(drawer.sections.is_empty());
    assert!(drawer.cells.is_empty());
    assert!(drawer.actions.is_empty());
    assert_eq!(drawer.span_ref, None);
    assert_eq!(
        serde_json::to_value(drawer).expect("serialize drawer"),
        json!({
            "name": "item_detail",
            "trigger": "select",
            "source": { "feature": "item", "kind": "lookup", "name": "by_id" }
        })
    );
}

#[test]
fn on_success_spec_round_trips_and_skips_empty_slots() {
    let spec = OnSuccessSpec {
        back: true,
        redirect: Some("/host/property/{result.id}".to_owned()),
        flash: Some(FlashSpec {
            kind: "success".to_owned(),
            message_key: TranslationKeyRef {
                key: "saved".to_owned(),
                span_ref: None,
            },
        }),
        invalidates: vec![InvalidatesSpec {
            query: QualifiedName {
                feature: Some("host".to_owned()),
                name: "lookup_my_host".to_owned(),
            },
            args: vec![],
        }],
        replace: false,
    };
    round_trip(&spec);
    let value = serde_json::to_value(&spec).expect("serialize on_success");
    assert_eq!(value["back"], json!(true));
    assert_eq!(value["redirect"], json!("/host/property/{result.id}"));
    assert_eq!(value["flash"]["message_key"]["key"], json!("saved"));
    assert_eq!(
        value["invalidates"][0]["query"]["name"],
        json!("lookup_my_host")
    );
    assert!(value.get("replace").is_none());
}
