//! Tests for the `SanitizeColumns` wiring emitted for fields carrying
//! `validate sanitize_html(<profile>)`. The map threads each column's
//! profile to the runtime so `applyCreates` / `applyUpdates` can rewrite
//! the bound value through the matching bluemonday policy before write —
//! closing the previously no-op `sanitize_html` constraint (stored XSS).

#![cfg(test)]

use super::test_support::{base_feature, emit, simple_field, simple_resource};
use lazuli_ir::{BuiltinType, SanitizeHtmlProfile};

fn sanitized_field(name: &str, profile: SanitizeHtmlProfile) -> lazuli_ir::Field {
    let mut f = simple_field(name, BuiltinType::Text, true);
    f.constraints.sanitize_html = Some(profile);
    f
}

#[test]
fn sanitize_html_field_emits_sanitize_columns_map() {
    let mut feature = base_feature("blog");
    let resource = simple_resource(
        "post",
        vec![
            simple_field("title", BuiltinType::Text, true),
            sanitized_field("body", SanitizeHtmlProfile::Strict),
        ],
    );
    feature.resources.push(resource);
    let out = emit(&feature).expect("must emit");

    // The runtime reads `SanitizeColumns` at the write boundary; codegen
    // must emit the column→profile entry so the constraint is no longer
    // a no-op.
    assert!(
        out.contains("SanitizeColumns: map[string]string{"),
        "expected SanitizeColumns map literal:\n{out}"
    );
    assert!(
        out.contains("\"body\": \"strict\","),
        "expected body→strict sanitize entry:\n{out}"
    );
}

#[test]
fn sanitize_profiles_lower_to_runtime_serde_strings() {
    let mut feature = base_feature("blog");
    let resource = simple_resource(
        "post",
        vec![
            sanitized_field("summary", SanitizeHtmlProfile::Basic),
            sanitized_field("content", SanitizeHtmlProfile::MarkdownSafe),
        ],
    );
    feature.resources.push(resource);
    let out = emit(&feature).expect("must emit");

    assert!(
        out.contains("\"summary\": \"basic\","),
        "expected summary→basic:\n{out}"
    );
    assert!(
        out.contains("\"content\": \"markdown_safe\","),
        "expected content→markdown_safe:\n{out}"
    );
}

#[test]
fn resource_without_sanitize_html_omits_the_map() {
    let mut feature = base_feature("blog");
    let resource = simple_resource("post", vec![simple_field("title", BuiltinType::Text, true)]);
    feature.resources.push(resource);
    let out = emit(&feature).expect("must emit");

    assert!(
        !out.contains("SanitizeColumns:"),
        "resources without sanitize_html must stay free of the map:\n{out}"
    );
}
