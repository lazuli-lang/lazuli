//! examples_snapshot -- assert each skills/audit/EXAMPLES/*.lzi file triggers its expected rule.
//!
//! Per docs/proposals/audit-skill-mvp.md §8 D.4. Each `.lzi` snippet's
//! header comment names the rule it triggers + the expected message
//! substring. This test enforces fidelity between the skill bundle's
//! examples and the doctor's actual rule output.

use std::path::{Path, PathBuf};

use lazuli_doctor::vocab::{
    vocab_audit_001, vocab_audit_002, vocab_cap_missing_001, vocab_derived_read_001,
    vocab_event_orphan_001, vocab_event_payload_001, vocab_event_producer_001,
    vocab_grammar_form_001, vocab_handler_heavy_001, vocab_json_typed_001,
    vocab_money_multi_currency_001, vocab_resource_wide_cluster_001, vocab_shadow_record_001,
    vocab_tests_missing_001, vocab_union_001, vocab_union_002,
};
use lazuli_ir::{BuiltinType, Event, EventField, EventKind, TypeRef};

fn examples_dir() -> PathBuf {
    let crate_dir = env!("CARGO_MANIFEST_DIR");
    Path::new(crate_dir)
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("skills/audit/EXAMPLES")
}

fn read_example(file_name: &str) -> (PathBuf, String, String) {
    let path = examples_dir().join(file_name);
    let source = std::fs::read_to_string(&path).expect("read snippet");
    let expected = expected_message(&source);
    (path, source, expected)
}

fn expected_message(source: &str) -> String {
    let line = source
        .lines()
        .find(|line| line.starts_with("# Expected message contains: "))
        .expect("snippet declares expected message substring");
    line.trim_start_matches("# Expected message contains: ")
        .trim()
        .trim_matches('"')
        .to_owned()
}

fn lower_feature(source: &str) -> lazuli_ir::Feature {
    let skeletons = lazuli_syntax::parse_feature_skeletons(source).expect("snippet parses");
    assert_eq!(skeletons.len(), 1, "snippet contains exactly one feature");
    let mut feature =
        lazuli_analyzer::lower_feature_skeleton(&skeletons[0]).expect("snippet lowers");

    // The current skeleton lowering leaves same-feature enum references as
    // UserDefined and does not yet lower feature-level event declarations.
    // Normalize those two vocab-rule inputs here so the public examples stay
    // anchored in source snippets while still exercising the rule check APIs.
    let enum_names: std::collections::HashSet<String> =
        feature.enums.iter().map(|decl| decl.name.clone()).collect();
    for resource in &mut feature.resources {
        for field in &mut resource.fields {
            if let TypeRef::UserDefined(name) = &field.type_ref {
                if name.feature.is_none() && enum_names.contains(&name.name) {
                    field.type_ref = TypeRef::EnumRef(name.clone());
                }
            }
        }
    }
    feature.events = parse_event_declarations(source);
    feature
}

fn parse_event_declarations(source: &str) -> Vec<Event> {
    let lines: Vec<&str> = source.lines().collect();
    let mut events = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let raw = lines[i];
        let trimmed = raw.trim_start();
        let indent = raw.len() - trimmed.len();
        let Some(rest) = trimmed.strip_prefix("event ") else {
            i += 1;
            continue;
        };

        let name = rest.split_whitespace().next().unwrap_or("").to_owned();
        if name.is_empty() {
            i += 1;
            continue;
        }

        let mut payload = Vec::new();
        let mut payload_none = false;
        i += 1;
        while i < lines.len() {
            let child_raw = lines[i];
            let child_trimmed = child_raw.trim_start();
            let child_indent = child_raw.len() - child_trimmed.len();
            if child_trimmed.is_empty() || child_trimmed.starts_with('#') {
                i += 1;
                continue;
            }
            if child_indent <= indent {
                break;
            }
            if child_trimmed == "payload none" {
                payload_none = true;
            } else if let Some((field_name, type_text)) = child_trimmed.split_once(':') {
                payload.push(EventField {
                    name: field_name.trim().to_owned(),
                    type_ref: builtin_type(type_text.trim()),
                    optional: type_text.contains(" optional"),
                });
            }
            i += 1;
        }

        events.push(Event {
            name,
            kind: EventKind::Domain,
            payload,
            payload_none,
            level: None,
            previous_names: vec![],
            span_ref: None,
        });
    }
    events
}

fn builtin_type(type_text: &str) -> TypeRef {
    match type_text.split_whitespace().next().unwrap_or(type_text) {
        "ID" | "Id" => TypeRef::Builtin(BuiltinType::Id),
        "Integer" => TypeRef::Builtin(BuiltinType::Integer),
        "Boolean" => TypeRef::Builtin(BuiltinType::Boolean),
        _ => TypeRef::Builtin(BuiltinType::Text),
    }
}

fn assert_fires(file_name: &str, messages: impl FnOnce(&str, &Path) -> Vec<String>) {
    let (path, source, expected) = read_example(file_name);
    let messages = messages(&source, &path);
    assert!(
        !messages.is_empty(),
        "expected at least one finding for {}",
        path.display()
    );
    assert!(
        messages[0].contains(&expected),
        "first finding for {} did not contain expected substring\nexpected: {expected}\nactual: {}",
        path.display(),
        messages[0]
    );
}

#[test]
fn vocab_audit_001_example_fires() {
    assert_fires("vocab-audit-001.lzi", |source, path| {
        let feature = lower_feature(source);
        vocab_audit_001::check(&feature, path)
            .into_iter()
            .map(|finding| finding.message())
            .collect()
    });
}

#[test]
fn vocab_audit_002_example_fires() {
    assert_fires("vocab-audit-002.lzi", |source, path| {
        let feature = lower_feature(source);
        vocab_audit_002::check(&feature, path)
            .into_iter()
            .map(|finding| finding.message())
            .collect()
    });
}

#[test]
fn vocab_cap_missing_001_example_fires() {
    assert_fires("vocab-cap-missing-001.lzi", |source, path| {
        vocab_cap_missing_001::check_source(source, path)
            .into_iter()
            .map(|finding| finding.message())
            .collect()
    });
}

#[test]
fn vocab_derived_read_001_example_fires() {
    assert_fires("vocab-derived-read-001.lzi", |source, path| {
        let feature = lower_feature(source);
        vocab_derived_read_001::check(&feature, path)
            .into_iter()
            .map(|finding| finding.message())
            .collect()
    });
}

#[test]
fn vocab_event_orphan_001_example_fires() {
    assert_fires("vocab-event-orphan-001.lzi", |source, path| {
        let feature = lower_feature(source);
        vocab_event_orphan_001::check(&feature, path)
            .into_iter()
            .map(|finding| finding.message())
            .collect()
    });
}

#[test]
fn vocab_event_payload_001_example_fires() {
    assert_fires("vocab-event-payload-001.lzi", |source, path| {
        let feature = lower_feature(source);
        vocab_event_payload_001::check(&feature, path)
            .into_iter()
            .map(|finding| finding.message())
            .collect()
    });
}

#[test]
fn vocab_event_producer_001_example_fires() {
    assert_fires("vocab-event-producer-001.lzi", |source, path| {
        let feature = lower_feature(source);
        vocab_event_producer_001::check(&feature, path)
            .into_iter()
            .map(|finding| finding.message())
            .collect()
    });
}

#[test]
fn vocab_grammar_form_001_example_fires() {
    assert_fires("vocab-grammar-form-001.lzi", |source, path| {
        vocab_grammar_form_001::check(source, path)
            .into_iter()
            .map(|finding| finding.message())
            .collect()
    });
}

#[test]
fn vocab_handler_heavy_001_example_fires() {
    assert_fires("vocab-handler-heavy-001.lzi", |source, path| {
        let feature = lower_feature(source);
        vocab_handler_heavy_001::check(&feature, path)
            .into_iter()
            .map(|finding| finding.message())
            .collect()
    });
}

#[test]
fn vocab_json_typed_001_example_fires() {
    assert_fires("vocab-json-typed-001.lzi", |source, path| {
        let feature = lower_feature(source);
        vocab_json_typed_001::check(&feature, path)
            .into_iter()
            .map(|finding| finding.message())
            .collect()
    });
}

#[test]
fn vocab_tests_missing_001_example_fires() {
    assert_fires("vocab-tests-missing-001.lzi", |source, path| {
        let feature = lower_feature(source);
        vocab_tests_missing_001::check(&feature, path)
            .into_iter()
            .map(|finding| finding.message())
            .collect()
    });
}

#[test]
fn vocab_money_multi_currency_001_example_fires() {
    assert_fires("vocab-money-multi-currency-001.lzi", |source, path| {
        let feature = lower_feature(source);
        vocab_money_multi_currency_001::check(&feature, path)
            .into_iter()
            .map(|finding| finding.message())
            .collect()
    });
}

#[test]
fn vocab_shadow_record_001_example_fires() {
    assert_fires("vocab-shadow-record-001.lzi", |source, path| {
        let feature = lower_feature(source);
        let module = lazuli_ir::Module {
            workspace: None,
            contracts: vec![],
            app: None,
            registry: None,
            profiles: vec![],
            design: None,
            rbac: None,
            features: vec![feature.clone()],
        };
        vocab_shadow_record_001::check(&feature, &module, path)
            .into_iter()
            .map(|finding| finding.message())
            .collect()
    });
}

#[test]
fn vocab_resource_wide_cluster_001_example_fires() {
    assert_fires("vocab-resource-wide-cluster-001.lzi", |source, path| {
        let feature = lower_feature(source);
        let module = lazuli_ir::Module {
            workspace: None,
            contracts: vec![],
            app: None,
            registry: None,
            profiles: vec![],
            design: None,
            rbac: None,
            features: vec![feature.clone()],
        };
        vocab_resource_wide_cluster_001::check(&feature, &module, path)
            .into_iter()
            .map(|finding| finding.message())
            .collect()
    });
}

#[test]
fn vocab_union_001_example_fires() {
    assert_fires("vocab-union-001.lzi", |source, path| {
        let feature = lower_feature(source);
        vocab_union_001::check(&feature, path)
            .into_iter()
            .map(|finding| finding.message())
            .collect()
    });
}

#[test]
fn vocab_union_002_example_fires() {
    assert_fires("vocab-union-002.lzi", |source, path| {
        let feature = lower_feature(source);
        vocab_union_002::check(&feature, path)
            .into_iter()
            .map(|finding| finding.message())
            .collect()
    });
}
