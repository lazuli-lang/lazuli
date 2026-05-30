//! VOCAB-LIFECYCLE-001 — transition commands over a status enum.
//!
//! Fires when a resource declares a status-like enum field and at least three
//! commands update that resource by advancing the enum through named states.
//! Suggests refactoring the repeated commands + enum into a resource-local
//! `lifecycle` block.
//!
//! Severity: `warning` (strict-profile), `error` (production-profile).
//! Reference: docs/proposals/doctor-vocabulary-lints.md §VOCAB-LIFECYCLE-001
//! Destination: docs/proposals/lifecycle-vocab.md §3

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use lazuli_ir::{
    Command, CommandEffect, EnumDecl, Expr, Feature, Field, QualifiedName, Resource, TypeRef,
};

// ── output ────────────────────────────────────────────────────────────────────

/// One VOCAB-LIFECYCLE-001 finding: a resource still spelling a lifecycle as
/// N transition commands plus a status enum.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file.
    pub path: PathBuf,
    /// Resource that owns the status field.
    pub resource: String,
    /// Status-like enum field (`status`, `state`, `phase`, `stage`, `step`).
    pub status_field: String,
    /// Enum type backing the status field.
    pub enum_name: String,
    /// Commands that look like state transitions.
    pub transition_commands: Vec<String>,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "VOCAB-LIFECYCLE-001";

    /// Render the "refactor to lifecycle block" message listing the
    /// transition commands and pointing at the closed-catalog invariants.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::vocab::vocab_lifecycle_001::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("f.lzi"),
    ///     resource: "Order".into(),
    ///     status_field: "status".into(),
    ///     enum_name: "OrderStatus".into(),
    ///     transition_commands: vec!["place_order".into(), "ship_order".into()],
    /// };
    /// assert!(f.message().contains("lifecycle"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "resource `{}` has {} transition command(s) ({}) advancing field `{}: {}` \
             — refactor to a `lifecycle {}` block on the resource. The new vocabulary \
             collapses commands + enum + invariants into one declaration with closed-\
             catalog invariants (terminal_immutable, single <state> per <scope>, \
             no_jump_more_than_one). See docs/proposals/lifecycle-vocab.md §3.",
            self.resource,
            self.transition_commands.len(),
            self.transition_commands.join(", "),
            self.status_field,
            self.enum_name,
            self.status_field
        )
    }
}

// ── detection ─────────────────────────────────────────────────────────────────

/// Run VOCAB-LIFECYCLE-001 over one feature's resources.
///
/// `path` is the source `.lzi` file — used to anchor findings; no I/O is
/// performed here.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::vocab::vocab_lifecycle_001::check;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower a feature with status-driven commands");
/// let _ = check(&feature, Path::new("orders.lzi"));
/// ```
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    let variants = build_variant_map(&feature.enums);
    feature
        .resources
        .iter()
        .filter_map(|resource| check_resource(resource, feature, &variants, path))
        .collect()
}

// ── internals ─────────────────────────────────────────────────────────────────

/// `enum_name -> variant_names_lowercase` for same-feature enums.
fn build_variant_map(enums: &[EnumDecl]) -> HashMap<String, HashSet<String>> {
    enums
        .iter()
        .map(|e| {
            let variants = e.variants.iter().map(|v| v.name.to_lowercase()).collect();
            (e.name.clone(), variants)
        })
        .collect()
}

fn check_resource(
    resource: &Resource,
    feature: &Feature,
    variants_by_enum: &HashMap<String, HashSet<String>>,
    path: &Path,
) -> Option<Finding> {
    if resource.lifecycle.is_some() {
        return None;
    }

    let (status_field, enum_name) = resource
        .fields
        .iter()
        .find_map(|field| status_enum_field(field, variants_by_enum))?;
    let variants = variants_by_enum.get(enum_name.as_str())?;

    let transition_commands: Vec<String> = feature
        .commands
        .iter()
        .filter(|cmd| updates_resource(cmd, &resource.name))
        .filter(|cmd| command_advances_status(cmd, status_field.as_str(), variants))
        .map(|cmd| cmd.name.clone())
        .collect();

    if transition_commands.len() < 3 {
        return None;
    }

    Some(Finding {
        path: path.to_path_buf(),
        resource: resource.name.clone(),
        status_field,
        enum_name,
        transition_commands,
    })
}

fn status_enum_field(
    field: &Field,
    variants_by_enum: &HashMap<String, HashSet<String>>,
) -> Option<(String, String)> {
    if !is_status_like_field(&field.name) {
        return None;
    }

    let TypeRef::EnumRef(qn) = &field.type_ref else {
        return None;
    };

    if qn.feature.is_none() && variants_by_enum.contains_key(&qn.name) {
        Some((field.name.clone(), qn.name.clone()))
    } else {
        None
    }
}

fn is_status_like_field(name: &str) -> bool {
    matches!(name, "status" | "state" | "phase" | "stage" | "step")
}

fn updates_resource(cmd: &Command, resource_name: &str) -> bool {
    match &cmd.effect {
        CommandEffect::Updates(effect) => qn_matches_local(&effect.resource, resource_name),
        _ => false,
    }
}

fn qn_matches_local(qn: &QualifiedName, name: &str) -> bool {
    qn.feature.is_none() && qn.name == name
}

fn command_advances_status(cmd: &Command, status_field: &str, variants: &HashSet<String>) -> bool {
    assignment_sets_status_to_variant(cmd, status_field, variants)
        || command_name_matches_variant(&cmd.name, variants)
}

fn assignment_sets_status_to_variant(
    cmd: &Command,
    status_field: &str,
    variants: &HashSet<String>,
) -> bool {
    let CommandEffect::Updates(effect) = &cmd.effect else {
        return false;
    };

    effect.assignments.iter().any(|assignment| {
        assignment.field == status_field && expr_names_variant(&assignment.value, variants)
    })
}

fn expr_names_variant(expr: &Expr, variants: &HashSet<String>) -> bool {
    match expr {
        Expr::Enum(lit) => variants.contains(&lit.variant.to_lowercase()),
        Expr::Path(path) => path
            .segments
            .last()
            .is_some_and(|segment| variants.contains(&segment.to_lowercase())),
        _ => false,
    }
}

fn command_name_matches_variant(name: &str, variants: &HashSet<String>) -> bool {
    let lower = name.to_lowercase();
    variants.iter().any(|variant| {
        lower == *variant
            || lower == format!("mark_{variant}")
            || lower == format!("set_{variant}")
            || lower == format!("transition_to_{variant}")
            || lower.ends_with(&format!("_{variant}"))
    })
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    include!("vocab_lifecycle_001_tests.rs");
}
