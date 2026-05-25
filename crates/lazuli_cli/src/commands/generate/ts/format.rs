//! Leaf formatters for TypeScript codegen.
//!
//! Carved out of `ts/mod.rs` as part of Wave R6-5 (Rails-style refactor).
//! Pure leaf helpers — no IR walking, no module/feature traversal —
//! that turn one IR node into one TS literal fragment:
//!
//! - **String escapes** ([`escape_js_string`], [`format_ts_string`]):
//!   covers `"`, `\`, control chars, and CRLF; conservative enough
//!   for embedding in TS double-quoted literals across the entire SDK
//!   surface.
//! - **Policy / audit literals** ([`format_policy_ts`],
//!   [`parse_policy_atom_ts`], [`format_audit_ts`]): lower
//!   `PolicyRef` / `AuditSpec` into the `PolicySpec` / `AuditSpec`
//!   shapes exported by `@lazuli/runtime/spec`.
//! - **Enum const/option emitters** ([`enum_value_constant_name`],
//!   [`enum_option_constant_name`], [`enum_variant_ts_literal`],
//!   [`enum_has_option_metadata`], [`write_enum_options_alias`],
//!   [`enum_variant_option_ts_literal`]): canonical
//!   `FOO_VALUES`/`FOO_OPTIONS` arrays + per-variant literal shapes.
//! - **Module-level lookup** ([`find_enum_decl`]): canonical resolver
//!   for `EnumRef` and `UserDefined` enum references.
//! - **Misc** ([`write_deprecated_const_alias`], [`format_string_array`]).

use std::fmt::Write as _;

/// Lower a `PolicyRef` to a TypeScript object literal matching the
/// `PolicySpec` shape exported by `@lazuli/runtime/spec`. Returns `None`
/// when the policy is omitted or explicitly `None` so the caller can
/// elide the `policy: ...` line entirely (review bug #7).
pub(crate) fn format_policy_ts(
    policy: &lazuli_ir::PolicyRef,
    feature: &lazuli_ir::Feature,
) -> Option<String> {
    // Re-prepend `@` when the parser dropped it. PolicyRef::Local
    // carries either the bare category name (`"update"`) or the
    // partial-qualified form (`"policy.update"`); PolicyRef::Atom can
    // arrive with or without the `@` host prefix. Normalize to the
    // DSL-faithful surface (`@policy.update`, `@role.admin`, …) so
    // clients see what they wrote.
    fn ensure_at_prefix(s: &str) -> String {
        if s.starts_with('@') {
            s.to_owned()
        } else {
            format!("@{}", s)
        }
    }
    let (name, atoms): (String, Vec<&str>) = match policy {
        lazuli_ir::PolicyRef::None => return None,
        lazuli_ir::PolicyRef::Local(local) => {
            let qualified = if local.contains('.') {
                ensure_at_prefix(local)
            } else {
                format!("@policy.{}", local)
            };
            let resolved_atoms: Vec<&str> = feature
                .policies
                .categories
                .iter()
                .find(|cat| cat.name == *local)
                .map(|cat| cat.atoms.iter().map(String::as_str).collect())
                .unwrap_or_default();
            (qualified, resolved_atoms)
        }
        lazuli_ir::PolicyRef::Atom(atom) => {
            let qualified = ensure_at_prefix(atom);
            // When the parser stored a `@policy.<name>` reference as
            // an Atom (vs Local), the literal `atom` itself is the
            // POLICY NAME, not an actual `@role.X`/`@scope.X`/`@actor.X`
            // atom. Resolve via the feature's policies dictionary to
            // recover the real atoms; fall back to treating it as a
            // standalone atom only when no category matches.
            let body = atom.trim_start_matches('@');
            let local_name = body.strip_prefix("policy.").unwrap_or("");
            let resolved_atoms: Vec<&str> = if !local_name.is_empty() {
                feature
                    .policies
                    .categories
                    .iter()
                    .find(|cat| cat.name == local_name)
                    .map(|cat| cat.atoms.iter().map(String::as_str).collect())
                    .unwrap_or_default()
            } else {
                vec![atom.as_str()]
            };
            (qualified, resolved_atoms)
        }
        lazuli_ir::PolicyRef::External { feature, name } => (
            format!("{}.{}", feature, ensure_at_prefix(name)),
            Vec::new(),
        ),
        lazuli_ir::PolicyRef::Unresolved(raw) => (raw.clone(), Vec::new()),
    };
    let atoms_lit = if atoms.is_empty() {
        "[]".to_owned()
    } else {
        let entries: Vec<String> = atoms
            .iter()
            .filter_map(|atom| parse_policy_atom_ts(atom))
            .collect();
        format!("[{}]", entries.join(", "))
    };
    Some(format!(
        "{{ name: \"{}\", atoms: {} }}",
        escape_js_string(&name),
        atoms_lit
    ))
}

/// Parse a raw policy atom string like `@role.admin` (or `role.admin`
/// when the parser dropped the host prefix) into the TS
/// `{ namespace: "role", name: "admin" }` literal. Returns `None` when
/// the atom does not parse — caller drops it from the literal rather
/// than emitting an invalid spec.
fn parse_policy_atom_ts(raw: &str) -> Option<String> {
    let body = raw.trim_start_matches('@');
    let (namespace, name) = body.split_once('.')?;
    if namespace.is_empty() || name.is_empty() {
        return None;
    }
    Some(format!(
        "{{ namespace: \"{}\", name: \"{}\" }}",
        escape_js_string(namespace),
        escape_js_string(name)
    ))
}

/// Lower an `AuditSpec` to a TypeScript literal matching the
/// `AuditSpec` union exported by `@lazuli/runtime/spec`:
///   - `Some({subjects: [], ..})`        → `"default"` sentinel
///   - `Some({subjects: ["actor", ..]})` → string array literal
///   - `None`                             → caller elides the field
pub(crate) fn format_audit_ts(audit: Option<&lazuli_ir::AuditSpec>) -> Option<String> {
    let audit = audit?;
    if audit.subjects.is_empty() {
        return Some("\"default\"".to_owned());
    }
    let entries: Vec<String> = audit
        .subjects
        .iter()
        .map(|s| format!("\"{}\"", escape_js_string(s)))
        .collect();
    Some(format!("[{}]", entries.join(", ")))
}

/// Escape a string for embedding in a TS double-quoted literal. Conservative:
/// covers `"`, `\`, and control chars that would terminate or break the
/// literal. Newlines collapse to `\n`; nothing else is interpreted.
pub(crate) fn escape_js_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if (ch as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", ch as u32));
            }
            _ => out.push(ch),
        }
    }
    out
}

pub(crate) fn find_enum_decl<'a>(
    module: &'a lazuli_ir::Module,
    name: &lazuli_ir::QualifiedName,
) -> Option<&'a lazuli_ir::EnumDecl> {
    module
        .features
        .iter()
        .filter(|feature| {
            name.feature
                .as_ref()
                .is_none_or(|owner| owner == &feature.name)
        })
        .flat_map(|feature| feature.enums.iter())
        .find(|enum_decl| enum_decl.name.eq_ignore_ascii_case(&name.name))
}

pub(crate) fn enum_value_constant_name(type_ref: &str) -> String {
    let local = type_ref
        .rsplit(['.', ':'])
        .find(|part| !part.is_empty())
        .unwrap_or(type_ref);
    let mut out = String::with_capacity(local.len() + "_VALUES".len());
    let mut prev_lower_or_digit = false;

    for ch in local.chars() {
        if ch.is_ascii_alphanumeric() {
            if ch.is_ascii_uppercase() && prev_lower_or_digit && !out.ends_with('_') {
                out.push('_');
            }
            out.push(ch.to_ascii_uppercase());
            prev_lower_or_digit = ch.is_ascii_lowercase() || ch.is_ascii_digit();
        } else {
            if !out.is_empty() && !out.ends_with('_') {
                out.push('_');
            }
            prev_lower_or_digit = false;
        }
    }

    while out.ends_with('_') {
        out.pop();
    }

    out.push_str("_VALUES");
    out
}

pub(crate) fn enum_option_constant_name(type_ref: &str) -> String {
    let mut out = enum_value_constant_name(type_ref);
    if out.ends_with("_VALUES") {
        let prefix_len = out.len() - "_VALUES".len();
        out.truncate(prefix_len);
        out.push_str("_OPTIONS");
    }
    out
}

pub(crate) fn enum_variant_ts_literal(variant: &lazuli_ir::EnumVariant) -> String {
    match &variant.storage_value {
        Some(lazuli_ir::StorageValue::String(value)) => format_ts_string(value),
        Some(lazuli_ir::StorageValue::Integer(value)) => value.to_string(),
        None => format_ts_string(&variant.name.to_ascii_lowercase()),
    }
}

pub(crate) fn enum_has_option_metadata(enum_decl: &lazuli_ir::EnumDecl) -> bool {
    enum_decl.variants.iter().any(|variant| {
        variant.label_key.is_some() || variant.hint_key.is_some() || variant.icon_key.is_some()
    })
}

pub(crate) fn write_enum_options_alias(
    s: &mut String,
    enum_decl: &lazuli_ir::EnumDecl,
    type_name: &str,
    options_name: &str,
) {
    let label_required = enum_decl
        .variants
        .iter()
        .all(|variant| variant.label_key.is_some());
    let label_prop = if label_required {
        "labelKey: string;"
    } else {
        "labelKey?: string;"
    };
    writeln!(s, "export const {options_name}: ReadonlyArray<{{").ok();
    writeln!(s, "  value: {type_name};").ok();
    writeln!(s, "  {label_prop}").ok();
    writeln!(s, "  hintKey?: string;").ok();
    writeln!(s, "  iconKey?: string;").ok();
    writeln!(s, "}}> = [").ok();
    for variant in &enum_decl.variants {
        writeln!(s, "  {},", enum_variant_option_ts_literal(variant)).ok();
    }
    writeln!(s, "];").ok();
}

fn enum_variant_option_ts_literal(variant: &lazuli_ir::EnumVariant) -> String {
    let mut props = vec![format!("value: {}", enum_variant_ts_literal(variant))];
    if let Some(label_key) = &variant.label_key {
        props.push(format!("labelKey: {}", format_ts_string(label_key)));
    }
    if let Some(hint_key) = &variant.hint_key {
        props.push(format!("hintKey: {}", format_ts_string(hint_key)));
    }
    if let Some(icon_key) = &variant.icon_key {
        props.push(format!("iconKey: {}", format_ts_string(icon_key)));
    }
    format!("{{ {} }}", props.join(", "))
}

fn format_ts_string(value: &str) -> String {
    let escaped = value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r");
    format!("\"{escaped}\"")
}

pub(crate) fn write_deprecated_const_alias(s: &mut String, old_name: &str, new_name: &str) {
    writeln!(s, "/** @deprecated use `{new_name}` */").ok();
    writeln!(s, "export const {old_name} = {new_name};").ok();
    writeln!(s).ok();
}

pub(crate) fn format_string_array(items: &[String]) -> String {
    if items.is_empty() {
        return "[]".to_owned();
    }
    let parts: Vec<String> = items.iter().map(|s| format!("\"{s}\"")).collect();
    format!("[{}]", parts.join(", "))
}
