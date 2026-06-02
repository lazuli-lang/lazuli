//! C3 (overnight-2026-06-02/03-codegen.md) regression — every emitted Go
//! function carries a `//lazuli:pattern <id> <version>` header.
//!
//! The framework's provenance invariant ("this function is codegen-owned,
//! do not edit") is enforced by the emitter-side lint
//! (`emitter::lint::check_pattern_annotations`). Before this fix a clean
//! pauta regen printed 27 CODEGEN-PATTERN-001 complaints (69 actual
//! offending funcs) across these shapes: enum `Valid()` / `UnmarshalJSON`,
//! record `Validate()`, referential `guard…Refs`, and the unique-violation
//! `init()`. This test drives the full parse→analyze→emit pipeline over a
//! source that exercises all four shapes and asserts the lint passes over
//! EVERY emitted `.gen.go` file — so the invariant can never silently
//! regress for these shapes again.

use lazuli_codegen_go::emitter::lint::check_pattern_annotations;
use lazuli_codegen_go::{GeneratedFile, GoEmitOptions, generate_v1};
use lazuli_ir::Module;

fn parsed_module(source: &str) -> Module {
    let features = lazuli_syntax::parse_feature_skeletons(source)
        .expect("feature source should parse")
        .into_iter()
        .map(|feature| {
            lazuli_analyzer::lower_feature_skeleton(&feature).expect("feature source should lower")
        })
        .collect();
    Module {
        workspace: None,
        contracts: Vec::new(),
        app: None,
        registry: None,
        profiles: Vec::new(),
        design: None,
        rbac: None,
        features,
    }
}

fn emit(source: &str) -> Vec<GeneratedFile> {
    generate_v1(&parsed_module(source), &GoEmitOptions::default())
}

// Covers the four header-omitting shapes the audit flagged:
//  - enum (string-backed) -> `Valid()` + `UnmarshalJSON`
//  - record with a nested record field -> `Validate()`
//  - `restrict on_delete` -> `guard…Refs`
//  - `unique … error <CODE>` -> registration `init()`
const SOURCE: &str = r#"feature catalog
  domain
    enum ItemStatus
      draft
      active
      retired

    record Address
      street: Text required
      city: Text required

    resource Supplier
      name: Text required
      address: Address required
      status: ItemStatus required
      restrict on_delete references item via supplier_id

    resource Item
      supplier_id: ID required
      sku: Text required
      unique (supplier_id, sku) error SKU_TAKEN
"#;

#[test]
fn every_emitted_go_func_carries_a_pattern_header() {
    let files = emit(SOURCE);

    // Sanity: the four target shapes were actually emitted (otherwise the
    // lint passing would be vacuous).
    let all: String = files
        .iter()
        .filter(|f| f.path.ends_with(".gen.go"))
        .map(|f| f.contents.clone())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(all.contains(") Valid() bool"), "enum Valid() not emitted");
    assert!(
        all.contains("UnmarshalJSON(data []byte) error"),
        "enum UnmarshalJSON not emitted"
    );
    assert!(all.contains(") Validate() error"), "record Validate() not emitted");
    assert!(all.contains("func guard"), "referential guard not emitted");
    assert!(
        all.contains("RegisterUniqueViolationCode"),
        "unique-code init() not emitted"
    );

    // The invariant: every emitted .gen.go func has a //lazuli:pattern
    // header. Handler stubs (app/features/…) are exempt by design — they
    // live in the user package, matching module/mod.rs's lint skip.
    for f in &files {
        if !f.path.ends_with(".go") || f.path.starts_with("app/features/") {
            continue;
        }
        check_pattern_annotations(&f.contents, &f.path).unwrap_or_else(|e| {
            panic!("CODEGEN-PATTERN-001 on {}: {e}", f.path);
        });
    }
}
