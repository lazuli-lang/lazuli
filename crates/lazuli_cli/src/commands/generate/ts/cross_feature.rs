//! Cross-feature reference walkers + alias emitters.
//!
//! Carved out of `ts/mod.rs` as part of Wave R6-5 (Rails-style refactor).
//! This module owns the type-reference graph helpers that make
//! `lazuli generate ts` emit correct cross-feature imports, re-exports,
//! plugin semantic aliases, and enum aliases under
//! `dist/ts-{web,mobile}/<feature>/<feature>.gen.ts`.
//!
//! Two failure classes drove the original code (kept verbatim here):
//!
//! - **WAR-CODEGEN-TS-01 / WAR-CODEGEN-XFEAT-01/02** — cross-feature
//!   enum/record refs silently dropped their import, forcing pilots
//!   to duplicate type declarations between consuming features.
//!   [`write_cross_feature_imports`] now emits both
//!   `import type { … } from "../other/other.gen"` AND a re-export
//!   shim so existing consumer code keeps compiling after a duplicate
//!   alias is removed.
//! - **Plugin semantic types** (`docs/proposals/semantic-types-plugin-locales.md`)
//!   — `@semantic.<Name>` projects to a brand alias
//!   (`export type BrazilianCPF = string;`); collected here so the SDK
//!   emitter writes the line at file head and every consuming
//!   interface picks up the alias.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use crate::casing::pascal_case;

use super::format::{
    enum_has_option_metadata, enum_option_constant_name, enum_value_constant_name,
    enum_variant_ts_literal, write_enum_options_alias,
};
use super::types::{command_sdk_slots, query_args};

/// Emit `import { X } from '../other-feature/other-feature.gen';` lines
/// for every enum/record referenced by this feature but declared in
/// another feature. Closes WAR-CODEGEN-TS-01 + WAR-CODEGEN-XFEAT-01/02:
/// previously such cross-feature references silently dropped the import
/// and produced `tsc` errors at the consumer site, forcing users to
/// duplicate enums/records across every consuming feature.
pub(crate) fn write_cross_feature_imports(
    s: &mut String,
    feature: &lazuli_ir::Feature,
    module: &lazuli_ir::Module,
) {
    // Map of owner-feature name → set of type names imported from it.
    let mut imports: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    collect_cross_feature_refs(feature, module, &mut imports);

    if imports.is_empty() {
        return;
    }

    let mut emitted = false;
    for (owner_feature, names) in &imports {
        let mut sorted: Vec<&String> = names.iter().collect();
        sorted.sort();
        let joined = sorted
            .iter()
            .map(|n| pascal_case(n))
            .collect::<Vec<_>>()
            .join(", ");
        // Emit both import (for local use in resource/command shapes)
        // AND re-export (so existing consumer code that imports the
        // type from this feature's .gen.ts continues to work after a
        // duplicate alias is removed). `export type { ... }` is
        // required because enum/record cross-feature refs are
        // type-only and isolatedModules rejects bare `export { ... }`
        // when the symbol carries no value.
        writeln!(
            s,
            "import type {{ {joined} }} from \"../{owner_feature}/{owner_feature}.gen\";"
        )
        .ok();
        writeln!(
            s,
            "export type {{ {joined} }} from \"../{owner_feature}/{owner_feature}.gen\";"
        )
        .ok();
        emitted = true;
    }
    if emitted {
        writeln!(s).ok();
    }
}

/// Walk every field/slot of every record/resource/command/query in
/// `feature` and accumulate the set of enum/record names that are
/// referenced but DECLARED in another feature.
fn collect_cross_feature_refs(
    feature: &lazuli_ir::Feature,
    module: &lazuli_ir::Module,
    out: &mut BTreeMap<String, BTreeSet<String>>,
) {
    let walk_type = |type_ref: &lazuli_ir::TypeRef,
                     out: &mut BTreeMap<String, BTreeSet<String>>| {
        let mut stack: Vec<&lazuli_ir::TypeRef> = vec![type_ref];
        while let Some(t) = stack.pop() {
            match t {
                lazuli_ir::TypeRef::Many(inner) => stack.push(inner),
                lazuli_ir::TypeRef::EnumRef(qn) | lazuli_ir::TypeRef::UserDefined(qn) => {
                    if let Some(owner) = owner_feature_for_type(qn, module, feature) {
                        out.entry(owner)
                            .or_insert_with(BTreeSet::new)
                            .insert(qn.name.clone());
                    }
                }
                _ => {}
            }
        }
    };
    for record in &feature.records {
        for field in &record.fields {
            walk_type(&field.type_ref, out);
        }
    }
    for resource in &feature.resources {
        for field in &resource.fields {
            walk_type(&field.type_ref, out);
        }
    }
    for command in &feature.commands {
        for slot in command_sdk_slots(feature, command, module) {
            walk_type(&slot.type_ref, out);
        }
        if let lazuli_ir::CommandEffect::Returns(effect) = &command.effect {
            walk_type(&effect.return_type, out);
        }
    }
    for query in &feature.queries {
        for slot in query_args(feature, query, module) {
            walk_type(&slot.type_ref, out);
        }
    }
}

/// Resolve a type reference to its owner feature name, but only when
/// the type lives in a DIFFERENT feature than `consumer`. Returns None
/// when the type is local (no import needed), defined in both consumer
/// and another feature (treat the duplicate as local — happens when
/// authors copy enums between features per WAR-CODEGEN-XFEAT-01), or
/// builtin/unresolvable.
fn owner_feature_for_type(
    qn: &lazuli_ir::QualifiedName,
    module: &lazuli_ir::Module,
    consumer: &lazuli_ir::Feature,
) -> Option<String> {
    let local_hit = consumer
        .enums
        .iter()
        .any(|e| e.name.eq_ignore_ascii_case(&qn.name))
        || consumer
            .records
            .iter()
            .any(|r| r.name.eq_ignore_ascii_case(&qn.name));
    if local_hit {
        return None;
    }
    // Honor the QualifiedName.feature hint if present (preferred owner).
    if let Some(hint) = qn.feature.as_deref() {
        if module.features.iter().any(|f| f.name == hint) {
            return Some(hint.to_owned());
        }
    }
    // Otherwise, find the first feature that declares this enum/record.
    for feature in &module.features {
        if feature.name == consumer.name {
            continue;
        }
        let owns = feature
            .enums
            .iter()
            .any(|e| e.name.eq_ignore_ascii_case(&qn.name))
            || feature
                .records
                .iter()
                .any(|r| r.name.eq_ignore_ascii_case(&qn.name));
        if owns {
            return Some(feature.name.clone());
        }
    }
    None
}

/// B3 — emit `export type <Name> = string;` brand aliases for every
/// plugin-contributed `@semantic.<Name>` referenced by this feature.
/// Per `docs/proposals/semantic-types-plugin-locales.md` §Codegen the
/// TS layer is type-only — no runtime validation — so an opaque alias
/// is the right surface. The Go side keeps the validate dispatch.
///
/// Sorted output keeps generated TS byte-stable across runs.
pub(crate) fn write_plugin_semantic_aliases(s: &mut String, feature: &lazuli_ir::Feature) {
    let mut aliases: BTreeSet<String> = BTreeSet::new();
    collect_plugin_semantic_aliases_in_feature(feature, &mut aliases);
    if aliases.is_empty() {
        return;
    }
    writeln!(
        s,
        "// Plugin-contributed semantic types (docs/proposals/semantic-types-plugin-locales.md)."
    )
    .ok();
    for name in aliases {
        // Carrier is `Text` in v1 → `string`. The proposal closed
        // carrier catalog locks to `String`; widening needs a separate
        // proposal that also threads a non-string TS shape.
        writeln!(s, "export type {} = string;", pascal_case(&name)).ok();
    }
    writeln!(s).ok();
}

fn collect_plugin_semantic_aliases_in_feature(
    feature: &lazuli_ir::Feature,
    out: &mut BTreeSet<String>,
) {
    for resource in &feature.resources {
        for field in &resource.fields {
            collect_plugin_semantic_aliases_in_type(&field.type_ref, out);
        }
    }
    for record in &feature.records {
        for field in &record.fields {
            collect_plugin_semantic_aliases_in_type(&field.type_ref, out);
        }
    }
    for event in &feature.events {
        for field in &event.payload {
            collect_plugin_semantic_aliases_in_type(&field.type_ref, out);
        }
    }
    for command in &feature.commands {
        if let lazuli_ir::CommandInput::Typed(slots) = &command.input {
            for slot in slots {
                collect_plugin_semantic_aliases_in_type(&slot.type_ref, out);
            }
        }
    }
}

fn collect_plugin_semantic_aliases_in_type(
    type_ref: &lazuli_ir::TypeRef,
    out: &mut BTreeSet<String>,
) {
    match type_ref {
        lazuli_ir::TypeRef::Builtin(lazuli_ir::BuiltinType::SemanticPluginType {
            name, ..
        }) => {
            out.insert(name.clone());
        }
        lazuli_ir::TypeRef::Many(inner) => {
            collect_plugin_semantic_aliases_in_type(inner, out);
        }
        _ => {}
    }
}

pub(crate) fn write_referenced_enum_aliases(
    s: &mut String,
    feature: &lazuli_ir::Feature,
    module: &lazuli_ir::Module,
) {
    let mut referenced = BTreeSet::new();
    collect_referenced_feature_enums(feature, module, &mut referenced);
    // Closes WAR-CODEGEN-TS-01: also emit enums referenced by OTHER
    // features (via cross-feature import). Without this, the owner
    // feature's .gen.ts wouldn't export the type the consumer imports.
    for other in &module.features {
        if other.name == feature.name {
            continue;
        }
        let mut other_refs = BTreeSet::new();
        collect_referenced_feature_enums(other, module, &mut other_refs);
        for r in other_refs {
            if feature.enums.iter().any(|e| e.name == r) {
                referenced.insert(r);
            }
        }
        // Also walk cross-feature import collection — captures enums
        // used in command inputs/outputs that the simple "referenced"
        // walk may have missed for the consumer side.
        let mut cross = BTreeMap::new();
        collect_cross_feature_refs(other, module, &mut cross);
        if let Some(names) = cross.get(&feature.name) {
            for n in names {
                if feature.enums.iter().any(|e| e.name == *n) {
                    referenced.insert(n.clone());
                }
            }
        }
    }

    let mut emitted = false;
    let mut enums: Vec<&lazuli_ir::EnumDecl> = feature.enums.iter().collect();
    enums.sort_by(|a, b| a.name.cmp(&b.name));
    for enum_decl in enums {
        if !referenced.contains(&enum_decl.name) {
            continue;
        }
        let type_name = pascal_case(&enum_decl.name);
        let const_name = enum_value_constant_name(&enum_decl.name);
        let options_name = enum_option_constant_name(&enum_decl.name);
        let values = enum_decl
            .variants
            .iter()
            .map(enum_variant_ts_literal)
            .collect::<Vec<_>>()
            .join(", ");
        writeln!(s, "export const {const_name} = [{values}] as const;").ok();
        writeln!(s, "export type {type_name} = typeof {const_name}[number];").ok();
        if enum_has_option_metadata(enum_decl) {
            write_enum_options_alias(s, enum_decl, &type_name, &options_name);
        }
        emitted = true;
    }
    if emitted {
        writeln!(s).ok();
    }
}

fn collect_referenced_feature_enums(
    feature: &lazuli_ir::Feature,
    module: &lazuli_ir::Module,
    out: &mut BTreeSet<String>,
) {
    for record in &feature.records {
        for field in &record.fields {
            collect_enum_ref(&field.type_ref, feature, out);
        }
    }
    for resource in &feature.resources {
        for field in &resource.fields {
            collect_enum_ref(&field.type_ref, feature, out);
        }
    }
    for command in &feature.commands {
        for slot in command_sdk_slots(feature, command, module) {
            collect_enum_ref(&slot.type_ref, feature, out);
        }
        if let lazuli_ir::CommandEffect::Returns(effect) = &command.effect {
            collect_enum_ref(&effect.return_type, feature, out);
        }
    }
    for query in &feature.queries {
        for slot in query_args(feature, query, module) {
            collect_enum_ref(&slot.type_ref, feature, out);
        }
    }
}

fn collect_enum_ref(
    type_ref: &lazuli_ir::TypeRef,
    feature: &lazuli_ir::Feature,
    out: &mut BTreeSet<String>,
) {
    match type_ref {
        lazuli_ir::TypeRef::EnumRef(name) if enum_ref_matches_feature(feature, name) => {
            out.insert(name.name.clone());
        }
        // UserDefined-tagged enum fields. Parallel to the
        // `UserDefined → enum_decl` fallback in `ts_type_for_type_ref`:
        // when the analyzer leaves an enum reference as
        // `UserDefined("CustomerTier")` (default-bearing fields seem
        // to take this path), the emitter resolves it to a real enum
        // — but the alias only lands at the top of the file if we ALSO
        // record the reference here. Without this branch the generated
        // TS references an undeclared `CustomerTier` symbol.
        lazuli_ir::TypeRef::UserDefined(name) if enum_ref_matches_feature(feature, name) => {
            out.insert(name.name.clone());
        }
        lazuli_ir::TypeRef::Many(inner) => collect_enum_ref(inner, feature, out),
        // Bare-name `Unresolved` fallback for the same reason — kept
        // narrow so we never invent a reference: the bare name MUST
        // already exist in the same feature's enum catalog.
        lazuli_ir::TypeRef::Unresolved(raw) if !raw.starts_with('@') => {
            if feature
                .enums
                .iter()
                .any(|enum_decl| enum_decl.name.eq_ignore_ascii_case(raw))
            {
                out.insert(raw.clone());
            }
        }
        _ => {}
    }
}

fn enum_ref_matches_feature(feature: &lazuli_ir::Feature, name: &lazuli_ir::QualifiedName) -> bool {
    name.feature
        .as_ref()
        .is_none_or(|owner| owner == &feature.name)
        && feature
            .enums
            .iter()
            .any(|enum_decl| enum_decl.name.eq_ignore_ascii_case(&name.name))
}
