//! VOCAB-UNION-002 — polymorphic FK (enum discriminator + untyped id).
//!
//! Fires on the `target: <Enum> + target_id: ID` shape where the FK is
//! untyped and gated by an enum tag. The discriminated-union vocabulary
//! expresses this with typed FKs per variant.
//!
//! Sibling of VOCAB-UNION-001 (which catches enum + correlated-optional-
//! fields). This rule catches the polymorphic-id-pair variant.
//!
//! Severity: `warning` (strict), `error` (production).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use lazuli_ir::{BuiltinType, EnumDecl, Feature, Field, Resource, TypeRef};

const DISCRIMINATOR_NAMES: &[&str] = &["target", "subject", "attachment_target", "parent_target"];

// ── output ───────────────────────────────────────────────────────────────────

/// One VOCAB-UNION-002 finding: an enum-discriminated untyped FK pair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file.
    pub path: PathBuf,
    /// Resource name.
    pub resource: String,
    /// Enum discriminator field, e.g. `target`.
    pub discriminator_field: String,
    /// Untyped FK sibling field, e.g. `target_id`.
    pub fk_field: String,
    /// Same-feature enum name used by the discriminator.
    pub enum_name: String,
    /// Same-feature enum variants.
    pub variants: Vec<String>,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "VOCAB-UNION-002";

    /// Render the long-form "enum + untyped FK" message, including the
    /// suggested `union` block and per-variant typed-FK resource form.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::vocab::vocab_union_002::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("f.lzi"),
    ///     resource: "Notification".into(),
    ///     discriminator_field: "target_kind".into(),
    ///     fk_field: "target_id".into(),
    ///     enum_name: "TargetKind".into(),
    ///     variants: vec!["Post".into(), "Comment".into()],
    /// };
    /// assert!(f.message().contains("discriminated union"));
    /// ```
    pub fn message(&self) -> String {
        let union_variants = self
            .variants
            .iter()
            .map(|variant| {
                format!(
                    "    On{variant}\n      {}: {variant} required\n      ...common fields...",
                    variant.to_lowercase()
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        let typed_resources = self
            .variants
            .iter()
            .map(|variant| {
                format!(
                    "  resource {variant}{}\n    {}: {variant} required\n    ...",
                    self.resource,
                    variant.to_lowercase()
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        format!(
            "resource `{}` declares `{}` as enum `{}` ({}) plus untyped FK `{}`; \
             suggestion: split into a discriminated union OR sibling typed-FK resources.\n\
             \n\
             union {}\n\
{}\n\
             \n\
             Or, for the typed-FK-per-resource form (vocabulary already supports this):\n\
{}",
            self.resource,
            self.discriminator_field,
            self.enum_name,
            self.variants.join(", "),
            self.fk_field,
            self.resource,
            union_variants,
            typed_resources,
        )
    }
}

// ── detection ────────────────────────────────────────────────────────────────

/// Run VOCAB-UNION-002 over one feature's resources.
///
/// `path` is the source `.lzi` file; this rule performs no I/O.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::vocab::vocab_union_002::check;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower a feature with enum+id pairs");
/// let _ = check(&feature, Path::new("billing.lzi"));
/// ```
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    let variants = build_variant_map(&feature.enums);
    feature
        .resources
        .iter()
        .flat_map(|r| check_resource(r, &variants, path))
        .collect()
}

// ── internals ────────────────────────────────────────────────────────────────

/// `enum_name -> [variant_name, ...]` for same-feature enums.
fn build_variant_map(enums: &[EnumDecl]) -> HashMap<String, Vec<String>> {
    enums
        .iter()
        .map(|e| {
            let variants = e.variants.iter().map(|v| v.name.clone()).collect();
            (e.name.clone(), variants)
        })
        .collect()
}

fn check_resource(
    resource: &Resource,
    variants: &HashMap<String, Vec<String>>,
    path: &Path,
) -> Vec<Finding> {
    let mut findings = Vec::new();

    for discriminator in resource
        .fields
        .iter()
        .filter(|f| DISCRIMINATOR_NAMES.contains(&f.name.as_str()))
    {
        let Some(enum_name) = enum_name_of(discriminator) else {
            continue;
        };

        let Some(enum_variants) = variants.get(enum_name) else {
            continue;
        };

        if enum_variants.len() < 2 {
            continue;
        }

        let fk_name = format!("{}_id", discriminator.name);
        let Some(fk_field) = resource.fields.iter().find(|f| f.name == fk_name) else {
            continue;
        };

        if !is_id_or_text(fk_field) {
            continue;
        }

        findings.push(Finding {
            path: path.to_path_buf(),
            resource: resource.name.clone(),
            discriminator_field: discriminator.name.clone(),
            fk_field: fk_field.name.clone(),
            enum_name: enum_name.to_string(),
            variants: enum_variants.clone(),
        });
    }

    findings
}

fn enum_name_of(field: &Field) -> Option<&str> {
    match &field.type_ref {
        TypeRef::EnumRef(qn) if qn.feature.is_none() => Some(qn.name.as_str()),
        _ => None,
    }
}

fn is_id_or_text(field: &Field) -> bool {
    matches!(
        &field.type_ref,
        TypeRef::Builtin(BuiltinType::Id | BuiltinType::Text)
    )
}

// ── tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    include!("vocab_union_002_tests.rs");
}
