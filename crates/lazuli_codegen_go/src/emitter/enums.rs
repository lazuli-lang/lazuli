//! Cell E2.5 — `EnumDecl` kind emission. Walks every `EnumDecl`
//! declared on a feature and emits a typed Go alias plus a
//! column-aligned `const (...)` block into `<feature>/enum.gen.go`.
//!
//! Proposal references:
//! - §3 — kind mapping (enums surface as Go typed aliases so pgx
//!   `RowToStructByName[T]` round-trips them transparently when used
//!   as a field type on a resource).
//! - §11 — boundary discipline: enum.gen.go imports nothing today
//!   (typed-alias form is self-contained); the `ImportSet` integration
//!   stays in place so when a future variant pulls in a runtime helper
//!   the wiring is already aligned with the rest of the emitter.
//!
//! ## Storage strategy
//!
//! - Any variant carrying `StorageValue::Integer(_)` flips the whole
//!   enum to `int64` typed alias. Variants without explicit values fall
//!   back to their declaration ordinal (warning comment emitted —
//!   mixed-storage enums signal a language-level smell that doctor
//!   should flag, not codegen to fix).
//! - All other enums (including those with mixed `StorageValue::String`
//!   and no-value variants) render as `string` typed aliases; variants
//!   without an explicit value reuse their canonical name as the
//!   literal.
//! - We deliberately skip emitting `String()` / `MarshalJSON` /
//!   `Scan`/`Value` helpers in v0. The typed-alias form works
//!   transparently with pgx + `encoding/json` and over-engineering
//!   would lock in shape before a pilot demands the helper.
//!
//! Determinism: enums are sorted by name before emission; variants
//! preserve the IR `Vec` order (which mirrors the source `.lzi` order
//! and is already deterministic).
//!
//! Module name `enums` (plural) because `enum` is a reserved keyword
//! in Rust 2024. Generated Go filename remains `enum.gen.go` —
//! singular reads more naturally for the file body's package
//! contents.

use std::fmt::Write;

use lazuli_ir::{EnumDecl, Feature, StorageValue};

use super::imports::ImportSet;
use super::printer::GoPrinter;

/// Emit `<feature>/enum.gen.go` for a feature, or `None` when the
/// feature declares no enums (so `module.rs` skips the file entirely
/// — an empty package body carries no signal).
pub fn emit_enum_file(source_label: &str, feature: &Feature) -> Option<String> {
    if feature.enums.is_empty() {
        return None;
    }

    let mut p = GoPrinter::new();
    let imports = ImportSet::new();

    // Sort enums by name so iteration order is independent of how the
    // IR `Vec` happened to be populated. Variants keep their source
    // ordering — that order maps to numeric ordinals when an enum
    // mixes explicit storage values with bare variants.
    let mut enums: Vec<&EnumDecl> = feature.enums.iter().collect();
    enums.sort_by(|a, b| a.name.cmp(&b.name));

    p.banner(source_label, &feature.name);
    // Even though today no enum requires an import, we route through
    // `ImportSet::emit` so the file shape stays consistent with the
    // resource emitter and a future `String()` helper can register its
    // dependency without restructuring the prelude.
    if !imports.is_empty() {
        imports.emit(&mut p);
        p.blank();
    }

    let mut first_block = true;
    for decl in &enums {
        if !first_block {
            p.blank();
        }
        first_block = false;
        emit_enum(&mut p, decl);
    }

    Some(p.finish())
}

/// Walk a single `EnumDecl` — typed alias + aligned const block.
fn emit_enum(p: &mut GoPrinter, decl: &EnumDecl) {
    let pascal = pascal_case(&decl.name);
    let storage = classify_storage(decl);

    write_section_banner(p, &[format!("Enum: {pascal}"), format!("  enum {pascal}")]);

    // Typed alias. `int64` chosen for the integer form because the IR
    // carries `StorageValue::Integer(i64)`; downcasting to a narrower
    // Go integer would silently truncate authored values.
    let alias_kind = match storage {
        StorageKind::Int => "int64",
        StorageKind::String => "string",
    };
    p.line(&format!(
        "// {pascal} is a typed alias for the {pascal} enum (proposal §3)."
    ));
    p.line(&format!("type {pascal} {alias_kind}"));
    p.blank();

    // Pre-render each variant row's columns so we can align name and
    // value across the const block. The third column (`= <literal>`)
    // sits flush against `<EnumName>` — the type column is fixed width
    // (the enum name) so we only need to pad the variant name.
    enum Row {
        Variant { name: String, literal: String },
        Comment(String),
    }

    let mut rows: Vec<Row> = Vec::with_capacity(decl.variants.len() + 1);
    let mut next_ordinal: i64 = 0;
    let mixed_storage = storage_is_mixed(decl);
    if mixed_storage {
        rows.push(Row::Comment(format!(
            "// WARNING: mixed storage values on `enum {pascal}`; bare variants fall back to ordinals.",
        )));
    }
    for variant in &decl.variants {
        let const_name = format!("{}{}", pascal, pascal_case(&variant.name));
        let literal = match (&storage, &variant.storage_value) {
            (StorageKind::Int, Some(StorageValue::Integer(n))) => {
                next_ordinal = n.saturating_add(1);
                n.to_string()
            }
            (StorageKind::Int, Some(StorageValue::String(_))) => {
                // Treated as design error upstream; emit ordinal so the
                // file at least compiles. doctor should flag the
                // smell separately.
                let current = next_ordinal;
                next_ordinal = next_ordinal.saturating_add(1);
                current.to_string()
            }
            (StorageKind::Int, None) => {
                let current = next_ordinal;
                next_ordinal = next_ordinal.saturating_add(1);
                current.to_string()
            }
            (StorageKind::String, Some(StorageValue::String(s))) => {
                format!("\"{}\"", s)
            }
            (StorageKind::String, Some(StorageValue::Integer(_))) => {
                // Unreachable in well-formed enums: `classify_storage`
                // promotes to Int the moment any variant carries an
                // integer storage value. Defensive fallback emits the
                // canonical variant name as a string literal.
                format!("\"{}\"", variant.name)
            }
            (StorageKind::String, None) => format!("\"{}\"", variant.name),
        };
        rows.push(Row::Variant {
            name: const_name,
            literal,
        });
    }

    let name_width = rows
        .iter()
        .filter_map(|r| match r {
            Row::Variant { name, .. } => Some(name.len()),
            Row::Comment(_) => None,
        })
        .max()
        .unwrap_or(0);

    p.line("const (");
    p.indent();
    for row in &rows {
        match row {
            Row::Comment(text) => p.line(text),
            Row::Variant { name, literal } => {
                let mut scratch =
                    String::with_capacity(name_width + pascal.len() + literal.len() + 8);
                let _ = write!(
                    scratch,
                    "{name:<name_width$} {pascal} = {literal}",
                    name = name,
                    name_width = name_width,
                    pascal = pascal,
                    literal = literal,
                );
                p.line(&scratch);
            }
        }
    }
    p.dedent();
    p.line(")");
}

/// Classify the storage strategy for an enum. Any variant carrying an
/// explicit integer storage value flips the whole enum to `int64`;
/// otherwise we render as a `string` typed alias.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StorageKind {
    Int,
    String,
}

fn classify_storage(decl: &EnumDecl) -> StorageKind {
    if decl
        .variants
        .iter()
        .any(|v| matches!(v.storage_value, Some(StorageValue::Integer(_))))
    {
        StorageKind::Int
    } else {
        StorageKind::String
    }
}

/// `true` when an enum mixes explicit storage values with bare
/// variants (or string + integer storage values). The emitter still
/// produces a compilable file but renders a `// WARNING` comment so a
/// reviewer can spot the smell; doctor surfaces the same condition as
/// a lint upstream.
fn storage_is_mixed(decl: &EnumDecl) -> bool {
    let mut has_value = false;
    let mut has_bare = false;
    let mut has_int = false;
    let mut has_string = false;
    for v in &decl.variants {
        match &v.storage_value {
            Some(StorageValue::Integer(_)) => {
                has_value = true;
                has_int = true;
            }
            Some(StorageValue::String(_)) => {
                has_value = true;
                has_string = true;
            }
            None => has_bare = true,
        }
    }
    (has_value && has_bare) || (has_int && has_string)
}

fn write_section_banner(p: &mut GoPrinter, lines: &[String]) {
    let rule = "-".repeat(76);
    p.line(&format!("// {rule}"));
    for line in lines {
        p.line(&format!("// {line}"));
    }
    p.line(&format!("// {rule}"));
    p.blank();
}

fn pascal_case(s: &str) -> String {
    super::casing::pascal_case(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{Defaults, EnumVariant, Feature, Policies};

    fn base_feature(name: &str) -> Feature {
        Feature {
            name: name.to_owned(),
            purpose: None,
            non_goals: Vec::new(),
            context_path: None,
            defaults: Defaults {
                tenancy: None,
                timestamps: false,
                policy: None,
            },
            uses: Vec::new(),
            requirements: Vec::new(),
            enums: Vec::new(),
            resources: Vec::new(),
            events: Vec::new(),
            rules: Vec::new(),
            policies: Policies {
                categories: Vec::new(),
                fields: Vec::new(),
                span_ref: None,
            },
            commands: Vec::new(),
            apis: Vec::new(),
            records: Vec::new(),
            queries: Vec::new(),
            workflows: Vec::new(),
            jobs: Vec::new(),
            webhooks: Vec::new(),
            notifications: Vec::new(),
            event_groups: Vec::new(),
            tenant_migrations: Vec::new(),
            translation: None,
            auth: None,
            surfaces: Vec::new(),
            extensions: Vec::new(),
            escape_routes: Vec::new(),
            agents: Vec::new(),
            previous_names: Vec::new(),
            span_ref: None,
        }
    }

    fn variant(name: &str, value: Option<StorageValue>) -> EnumVariant {
        EnumVariant {
            name: name.to_owned(),
            storage_value: value,
            previous_names: Vec::new(),
        }
    }

    fn make_enum(name: &str, variants: Vec<EnumVariant>) -> EnumDecl {
        EnumDecl {
            name: name.to_owned(),
            variants,
            previous_names: Vec::new(),
            span_ref: None,
        }
    }

    #[test]
    fn empty_feature_returns_none() {
        let feature = base_feature("customer");
        assert!(emit_enum_file("examples/x.lzi", &feature).is_none());
    }

    #[test]
    fn int_typed_enum_emits_int64_alias_and_aligned_consts() {
        let mut feature = base_feature("customer");
        feature.enums.push(make_enum(
            "CustomerStatus",
            vec![
                variant("lead", Some(StorageValue::Integer(10))),
                variant("active", Some(StorageValue::Integer(20))),
                variant("paused", Some(StorageValue::Integer(30))),
                variant("archived", Some(StorageValue::Integer(90))),
            ],
        ));
        let out = emit_enum_file("examples/x.lzi", &feature).expect("must emit");

        assert!(out.contains("// Code generated by lazuli; DO NOT EDIT."));
        assert!(out.contains("package customer"));
        assert!(out.contains("type CustomerStatus int64"));
        assert!(out.contains("const ("));
        // Alignment check — name column widest is `CustomerStatusArchived` (22 chars);
        // shorter rows pad to that width.
        assert!(
            out.contains("CustomerStatusLead     CustomerStatus = 10"),
            "expected aligned lead row, got:\n{out}"
        );
        assert!(out.contains("CustomerStatusActive   CustomerStatus = 20"));
        assert!(out.contains("CustomerStatusPaused   CustomerStatus = 30"));
        assert!(out.contains("CustomerStatusArchived CustomerStatus = 90"));
    }

    #[test]
    fn string_typed_enum_emits_string_alias_and_quoted_literals() {
        let mut feature = base_feature("customer");
        feature.enums.push(make_enum(
            "CustomerTier",
            vec![
                variant("free", None),
                variant("pro", None),
                variant("enterprise", None),
            ],
        ));
        let out = emit_enum_file("examples/x.lzi", &feature).expect("must emit");

        assert!(out.contains("type CustomerTier string"));
        assert!(out.contains("CustomerTierFree       CustomerTier = \"free\""));
        assert!(out.contains("CustomerTierPro        CustomerTier = \"pro\""));
        assert!(out.contains("CustomerTierEnterprise CustomerTier = \"enterprise\""));
    }

    #[test]
    fn snake_case_variant_renders_pascal_concat() {
        let mut feature = base_feature("billing");
        feature.enums.push(make_enum(
            "AccountKind",
            vec![
                variant("paid_account", None),
                variant("trial_account", None),
            ],
        ));
        let out = emit_enum_file("examples/x.lzi", &feature).expect("must emit");
        assert!(out.contains("AccountKindPaidAccount  AccountKind = \"paid_account\""));
        assert!(out.contains("AccountKindTrialAccount AccountKind = \"trial_account\""));
    }

    #[test]
    fn two_enums_separated_by_blank_line() {
        let mut feature = base_feature("customer");
        feature.enums.push(make_enum(
            "CustomerTier",
            vec![variant("free", None), variant("pro", None)],
        ));
        feature.enums.push(make_enum(
            "CustomerSource",
            vec![variant("manual", None), variant("api", None)],
        ));
        let out = emit_enum_file("examples/x.lzi", &feature).expect("must emit");
        assert!(out.contains("type CustomerSource string"));
        assert!(out.contains("type CustomerTier string"));
        // Sorted alphabetically — CustomerSource before CustomerTier.
        let source_pos = out.find("Enum: CustomerSource").expect("source banner");
        let tier_pos = out.find("Enum: CustomerTier").expect("tier banner");
        assert!(
            source_pos < tier_pos,
            "enums should be sorted by name: source ({source_pos}) before tier ({tier_pos})"
        );
        // Blank line between blocks: the second banner's leading rule
        // is preceded by a blank line. Easier to check the structural
        // invariant: there's a `)\n\n//` between the two const blocks
        // (close of first const, blank, then the second section's
        // banner).
        assert!(out.contains(")\n\n//"));
    }

    #[test]
    fn mixed_storage_emits_warning_comment_and_falls_back_to_ordinals() {
        let mut feature = base_feature("ops");
        feature.enums.push(make_enum(
            "Mixed",
            vec![
                variant("lead", Some(StorageValue::Integer(10))),
                variant("paused", None),
                variant("done", Some(StorageValue::Integer(30))),
            ],
        ));
        let out = emit_enum_file("examples/x.lzi", &feature).expect("must emit");
        assert!(out.contains("type Mixed int64"));
        assert!(
            out.contains("// WARNING: mixed storage values on `enum Mixed`"),
            "expected mixed-storage warning, got:\n{out}"
        );
        // `paused` falls between 10 and 30; ordinal starts at 11
        // (i.e. previous value + 1).
        assert!(out.contains("MixedLead   Mixed = 10"));
        assert!(out.contains("MixedPaused Mixed = 11"));
        assert!(out.contains("MixedDone   Mixed = 30"));
    }

    #[test]
    fn deterministic_across_runs() {
        let mut feature = base_feature("customer");
        feature.enums.push(make_enum(
            "Zebra",
            vec![variant("a", None), variant("b", None)],
        ));
        feature.enums.push(make_enum(
            "Alpha",
            vec![variant("x", None), variant("y", None)],
        ));
        let a = emit_enum_file("examples/x.lzi", &feature).expect("must emit");
        let b = emit_enum_file("examples/x.lzi", &feature).expect("must emit");
        assert_eq!(a, b);
        let alpha_pos = a.find("Enum: Alpha").expect("alpha banner");
        let zebra_pos = a.find("Enum: Zebra").expect("zebra banner");
        assert!(alpha_pos < zebra_pos);
    }
}
