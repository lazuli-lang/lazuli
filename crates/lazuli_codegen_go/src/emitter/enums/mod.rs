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
///
/// ## Examples
///
/// ```ignore
/// let go_src = emit_enum_file("billing.lzi", &feature);
/// ```
pub fn emit_enum_file(source_label: &str, feature: &Feature) -> Option<String> {
    if feature.enums.is_empty() {
        return None;
    }

    let mut p = GoPrinter::new();
    let mut imports = ImportSet::new();

    // String-backed enums emit a `Valid()` membership predicate plus an
    // `UnmarshalJSON` decode hook (see `emit_enum`). Those pull in
    // `encoding/json` (to peel the JSON string) and `fmt` (to format the
    // rejection error). Register them up-front so the import block is
    // emitted before any enum body. Int-storage enums need nothing.
    if feature
        .enums
        .iter()
        .any(|e| classify_storage(e) == StorageKind::String)
    {
        imports.add("encoding/json");
        imports.add("fmt");
    }

    // Sort enums by name so iteration order is independent of how the
    // IR `Vec` happened to be populated. Variants keep their source
    // ordering — that order maps to numeric ordinals when an enum
    // mixes explicit storage values with bare variants.
    let mut enums: Vec<&EnumDecl> = feature.enums.iter().collect();
    enums.sort_by(|a, b| a.name.cmp(&b.name));

    p.banner(
        source_label,
        &super::casing::gen_package_name(&feature.name),
    );
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

    if enum_has_option_metadata(decl) {
        p.blank();
        emit_enum_options(p, decl, &pascal);
    }

    // Membership guard. String-backed enums round-trip through
    // `json.Unmarshal` at the command-input decode boundary as a bare
    // `type X string` alias — without a hook, a value outside the
    // declared variant set is silently accepted (the gap the pauta
    // `validate_member_role.go` hand-guards). Emit a `Valid()` predicate
    // plus an `UnmarshalJSON` that rejects an unknown variant, mirroring
    // the `@semantic.*` carrier pattern (`runtime/go/lazuli/
    // semantic_scalars.go`): the error lifts to a 400 validation_failed
    // envelope through the decode pipeline. Int-storage enums keep the
    // ordinal-only `int64` form (int membership is out of scope here).
    if storage == StorageKind::String {
        p.blank();
        emit_string_membership_guard(p, decl, &pascal);
    }
}

/// Emit a `Valid()` membership predicate and an `UnmarshalJSON` decode
/// hook for a string-backed enum. `Valid()` switches over the declared
/// variant constants; `UnmarshalJSON` peels the JSON string, then
/// rejects any value not in the set so an unknown variant fails at the
/// input boundary instead of silently passing through.
fn emit_string_membership_guard(p: &mut GoPrinter, decl: &EnumDecl, pascal: &str) {
    // `Valid()` — true iff the receiver is one of the declared variants.
    p.line(&format!(
        "// Valid reports whether v is one of the declared {pascal} variants."
    ));
    p.line(&format!("func (v {pascal}) Valid() bool {{"));
    p.indent();
    p.line("switch v {");
    if decl.variants.is_empty() {
        // No variants → nothing is valid. (Unreachable for a well-formed
        // enum, but keeps the emitted Go compilable.)
        p.line("default:");
        p.indent();
        p.line("return false");
        p.dedent();
    } else {
        let cases = decl
            .variants
            .iter()
            .map(|variant| format!("{}{}", pascal, pascal_case(&variant.name)))
            .collect::<Vec<_>>()
            .join(", ");
        p.line(&format!("case {cases}:"));
        p.indent();
        p.line("return true");
        p.dedent();
        p.line("default:");
        p.indent();
        p.line("return false");
        p.dedent();
    }
    p.line("}");
    p.dedent();
    p.line("}");
    p.blank();

    // `UnmarshalJSON` — peel the string, enforce membership at decode.
    p.line(&format!(
        "// UnmarshalJSON decodes a JSON string and rejects any value outside the"
    ));
    p.line(&format!(
        "// declared {pascal} variant set. An unknown variant returns an error that"
    ));
    p.line("// lifts to a 400 validation_failed envelope through the decode pipeline.");
    p.line(&format!(
        "func (v *{pascal}) UnmarshalJSON(data []byte) error {{"
    ));
    p.indent();
    p.line("var s string");
    p.line("if err := json.Unmarshal(data, &s); err != nil {");
    p.indent();
    p.line("return err");
    p.dedent();
    p.line("}");
    p.line(&format!("parsed := {pascal}(s)"));
    p.line("if !parsed.Valid() {");
    p.indent();
    p.line(&format!(
        "return fmt.Errorf(\"lazuli: invalid {pascal} value %q\", s)"
    ));
    p.dedent();
    p.line("}");
    p.line("*v = parsed");
    p.line("return nil");
    p.dedent();
    p.line("}");
}

fn enum_has_option_metadata(decl: &EnumDecl) -> bool {
    decl.variants.iter().any(|variant| {
        variant.label_key.is_some() || variant.hint_key.is_some() || variant.icon_key.is_some()
    })
}

fn emit_enum_options(p: &mut GoPrinter, decl: &EnumDecl, pascal: &str) {
    let option_type = format!("{pascal}Option");
    let options_var = format!("{pascal}Options");

    p.line(&format!("type {option_type} struct {{"));
    p.indent();
    p.line(&format!("Value {pascal}"));
    p.line("LabelKey string");
    p.line("HintKey string");
    p.line("IconKey string");
    p.dedent();
    p.line("}");
    p.blank();

    p.line(&format!("var {options_var} = []{option_type}{{"));
    p.indent();
    for variant in &decl.variants {
        let const_name = format!("{}{}", pascal, pascal_case(&variant.name));
        let mut fields = vec![format!("Value: {const_name}")];
        if let Some(label_key) = &variant.label_key {
            fields.push(format!("LabelKey: {}", go_string_literal(label_key)));
        }
        if let Some(hint_key) = &variant.hint_key {
            fields.push(format!("HintKey: {}", go_string_literal(hint_key)));
        }
        if let Some(icon_key) = &variant.icon_key {
            fields.push(format!("IconKey: {}", go_string_literal(icon_key)));
        }
        p.line(&format!("{{{}}},", fields.join(", ")));
    }
    p.dedent();
    p.line("}");
}

/// Classify the storage strategy for an enum. Any variant carrying an
/// explicit integer storage value flips the whole enum to `int64`;
/// otherwise we render as a `string` typed alias.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StorageKind {
    Int,
    String,
}

pub(crate) fn classify_storage(decl: &EnumDecl) -> StorageKind {
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

fn go_string_literal(raw: &str) -> String {
    format!("\"{}\"", escape_string(raw))
}

fn escape_string(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            _ => out.push(ch),
        }
    }
    out
}

#[cfg(test)]
mod tests;
