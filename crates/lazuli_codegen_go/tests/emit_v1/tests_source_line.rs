//! `//line` directive tests — span-bearing IR nodes must emit a
//! source-map back-pointer so the generated Go file blames the
//! originating `.lzi` line, and command bodies must reset to the
//! generated file afterwards.

use std::collections::BTreeMap;

use lazuli_analyzer::source_map::SourceMapResolver;
use lazuli_codegen_go::{GoEmitOptions, generate_v1_with_manifest_and_source};
use lazuli_ir::{SourceMap, SpanRef};

use super::builders::{
    command_gen_contents, empty_command, minimal_module, source_map_context,
};

#[test]
fn emit_command_with_span_emits_line_directive() {
    let mut module = minimal_module("marketplace", "foo");
    let source = format!("{}command create\n", "\n".repeat(41));
    let span = SpanRef { start: 41, end: 55 };
    module.features[0]
        .commands
        .push(empty_command("create", Some(span)));

    let source_map = SourceMap {
        files: vec![SourceMap::build_source_file(7, "features/foo.lzi", &source)],
    };
    let feature_file_ids = BTreeMap::from([("foo".to_owned(), 7)]);
    let files = generate_v1_with_manifest_and_source(
        &module,
        &GoEmitOptions::default(),
        None,
        source_map_context(&source_map, &feature_file_ids),
    );
    let out = command_gen_contents(&files, "foo");

    let directive = out
        .find("//line features/foo.lzi:42:1")
        .expect("expected source //line directive");
    let declaration = out
        .find("var createResult = lazuli.Command[struct{}, struct{}]{")
        .expect("expected command declaration");
    assert!(
        directive < declaration,
        "directive must precede command declaration:\n{out}"
    );
    assert!(
        out.contains("ctx = lazuli.WithSource(ctx, lazuli.SourceTag{"),
        "command handler source hook must stamp SourceTag:\n{out}"
    );
    assert!(
        out.contains("Capsule: \"marketplace\",")
            && out.contains("Feature: \"foo\",")
            && out.contains("Kind:    \"command\",")
            && out.contains("Op:      \"create\",")
            && out.contains("Source:  \"features/foo.lzi:42:1\","),
        "command handler source hook must carry the originating op:\n{out}"
    );
}

#[test]
fn emit_command_without_span_emits_no_directive() {
    let mut module = minimal_module("marketplace", "foo");
    module.features[0]
        .commands
        .push(empty_command("create", None));

    let source_map = SourceMap {
        files: vec![SourceMap::build_source_file(
            7,
            "features/foo.lzi",
            "command create\n",
        )],
    };
    let feature_file_ids = BTreeMap::from([("foo".to_owned(), 7)]);
    let files = generate_v1_with_manifest_and_source(
        &module,
        &GoEmitOptions::default(),
        None,
        source_map_context(&source_map, &feature_file_ids),
    );
    let out = command_gen_contents(&files, "foo");

    assert!(
        !out.contains("//line "),
        "missing SpanRef should fall back to generated Go locations:\n{out}"
    );
}

#[test]
fn emit_command_resets_to_gen_file_after_body() {
    let mut module = minimal_module("marketplace", "foo");
    module.features[0]
        .commands
        .push(empty_command("create", Some(SpanRef { start: 0, end: 14 })));

    let source_map = SourceMap {
        files: vec![SourceMap::build_source_file(
            7,
            "features/foo.lzi",
            "command create\n",
        )],
    };
    let feature_file_ids = BTreeMap::from([("foo".to_owned(), 7)]);
    let files = generate_v1_with_manifest_and_source(
        &module,
        &GoEmitOptions::default(),
        None,
        source_map_context(&source_map, &feature_file_ids),
    );
    let out = command_gen_contents(&files, "foo");

    assert!(
        out.contains("}\n//line foo/command.gen.go:1:1\n"),
        "expected reset directive after command value body:\n{out}"
    );
}

#[test]
fn multiple_handlers_in_same_feature_each_get_directive_pair() {
    let mut module = minimal_module("marketplace", "foo");
    module.features[0]
        .commands
        .push(empty_command("create", Some(SpanRef { start: 0, end: 14 })));
    module.features[0].commands.push(empty_command(
        "update",
        Some(SpanRef { start: 15, end: 29 }),
    ));

    let source = "command create\ncommand update\n";
    let source_map = SourceMap {
        files: vec![SourceMap::build_source_file(7, "features/foo.lzi", source)],
    };
    let feature_file_ids = BTreeMap::from([("foo".to_owned(), 7)]);
    let files = generate_v1_with_manifest_and_source(
        &module,
        &GoEmitOptions::default(),
        None,
        source_map_context(&source_map, &feature_file_ids),
    );
    let out = command_gen_contents(&files, "foo");

    assert_eq!(out.matches("//line features/foo.lzi:").count(), 2, "{out}");
    assert_eq!(
        out.matches("//line foo/command.gen.go:1:1").count(),
        2,
        "{out}"
    );
}
