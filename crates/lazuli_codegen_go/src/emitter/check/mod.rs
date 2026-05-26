//! `lazuli generate go --check` closed error catalog.
//!
//! The pass is intentionally codegen-local: it catches refs that the
//! Go emitter cannot resolve without reaching into doctor or the
//! filesystem. Discovery-backed checks (`@adapter.*` register sites
//! and `@fn.*` stubs) are recognized here but remain no-op stubs until
//! extension discovery lands.

use std::collections::BTreeSet;

use lazuli_ir::Module;

mod refs;
mod registries;

use refs::collect_feature_refs;
use registries::{
    declared_plugin_names, known_cap_ref, known_runtime_ref, known_semantic_ref, plugin_declared,
};

pub const CODE_PLUGIN: &str = "CODEGEN-GO-PLUGIN-001";
pub const CODE_UNRESOLVED: &str = "CODEGEN-GO-UNRESOLVED-002";
pub const CODE_ADAPTER: &str = "CODEGEN-GO-ADAPTER-003";
pub const CODE_SEMANTIC: &str = "CODEGEN-GO-SEMANTIC-004";
pub const CODE_CAP: &str = "CODEGEN-GO-CAP-005";
pub const CODE_FN: &str = "CODEGEN-GO-FN-006";
/// `TypeRef::Unresolved` with a non-`@` raw name reached the codegen.
/// The analyzer could not resolve a record/resource/enum reference, so
/// the emitter would either inline a Go-invalid name or silently fall
/// back to a placeholder. Fail loudly instead so users see broken
/// references at `lazuli generate go` time, not at runtime.
pub const CODE_TYPE_UNRESOLVED: &str = "CODEGEN-GO-TYPE-007";

/// Synthetic ref literal used to flag a `TypeRef::Unresolved(raw)` so
/// `run_checks` can emit `CODE_TYPE_UNRESOLVED` without changing the
/// signature of the recursive collectors. The prefix is illegal in any
/// authored DSL (no `@` host, double underscore) so it cannot collide
/// with a real reference.
pub(super) const UNRESOLVED_TYPE_PREFIX: &str = "__lazuli_type_unresolved__/";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckIssue {
    pub code: &'static str,
    pub severity: Severity,
    pub message: String,
    pub feature: Option<String>,
    pub site: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RefUse {
    pub(super) literal: String,
    pub(super) feature: Option<String>,
    pub(super) site: Option<String>,
}

pub fn run_checks(module: &Module) -> Vec<CheckIssue> {
    let declared_plugins = declared_plugin_names(module);
    let mut refs = Vec::new();

    for feature in &module.features {
        collect_feature_refs(feature, &mut refs);
    }

    let mut issues = Vec::new();
    let mut seen = BTreeSet::new();
    for reference in refs {
        if let Some(name) = reference.literal.strip_prefix("@lazuli/plugin-") {
            if !plugin_declared(&declared_plugins, name) {
                push_issue(
                    &mut issues,
                    &mut seen,
                    CODE_PLUGIN,
                    Severity::Error,
                    format!(
                        "plugin reference {} not declared in app.lzi registry",
                        reference.literal
                    ),
                    &reference,
                );
            }
        } else if let Some(name) = reference.literal.strip_prefix("@runtime/") {
            if !known_runtime_ref(name) {
                push_issue(
                    &mut issues,
                    &mut seen,
                    CODE_UNRESOLVED,
                    Severity::Error,
                    format!(
                        "runtime reference {} is not in the closed Go runtime catalog",
                        reference.literal
                    ),
                    &reference,
                );
            }
        } else if let Some(name) = reference.literal.strip_prefix("@semantic.") {
            if !known_semantic_ref(name) {
                push_issue(
                    &mut issues,
                    &mut seen,
                    CODE_SEMANTIC,
                    Severity::Error,
                    format!(
                        "semantic reference {} is outside the closed Go semantic table",
                        reference.literal
                    ),
                    &reference,
                );
            }
        } else if let Some(name) = reference.literal.strip_prefix("@cap.") {
            if !known_cap_ref(name) {
                push_issue(
                    &mut issues,
                    &mut seen,
                    CODE_CAP,
                    Severity::Error,
                    format!(
                        "capability reference {} is outside Hashed/Encrypted/Token/File",
                        reference.literal
                    ),
                    &reference,
                );
            }
        } else if let Some(name) = reference.literal.strip_prefix(UNRESOLVED_TYPE_PREFIX) {
            // Bare unresolved type identifier — analyzer could not map
            // it to a resource/record/enum. Without this hard fail the
            // emitter inlines a sanitised placeholder and the Go build
            // breaks with a confusing "undeclared name" downstream.
            push_issue(
                &mut issues,
                &mut seen,
                CODE_TYPE_UNRESOLVED,
                Severity::Error,
                format!(
                    "type reference `{}` does not resolve to a known resource, record, or enum",
                    name
                ),
                &reference,
            );
        } else if reference.literal.starts_with("@adapter.") {
            // Stub for CODEGEN-GO-ADAPTER-003. RegisterAdapter discovery
            // needs filesystem/runtime integration context that this pure
            // Module pass does not receive yet.
        } else if reference.literal.starts_with("@fn.") {
            // Stub for CODEGEN-GO-FN-006. Extension stub discovery lands
            // with the follow-up §10.5 resolver.
        }
    }

    issues
}

fn push_issue(
    issues: &mut Vec<CheckIssue>,
    seen: &mut BTreeSet<(&'static str, String, Option<String>, Option<String>)>,
    code: &'static str,
    severity: Severity,
    message: String,
    reference: &RefUse,
) {
    let key = (
        code,
        reference.literal.clone(),
        reference.feature.clone(),
        reference.site.clone(),
    );
    if seen.insert(key) {
        issues.push(CheckIssue {
            code,
            severity,
            message,
            feature: reference.feature.clone(),
            site: reference.site.clone(),
        });
    }
}


#[cfg(test)]
mod tests;
