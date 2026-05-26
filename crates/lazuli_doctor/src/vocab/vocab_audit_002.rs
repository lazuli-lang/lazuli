//! VOCAB-AUDIT-002 — handler-only command on capability-tagged fields lacks audit.
//!
//! Companion to VOCAB-AUDIT-001. This rule covers the conservative IR-visible
//! case where a handler-only command (`returns` / no effect) invalidates a
//! resource carrying sensitive `@cap.*` fields but declares no `audit` child.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use lazuli_ir::{CapabilityRef, Command, CommandEffect, Feature, Field, Resource, TypeRef};

const SENSITIVE_TIERS: &[&str] = &["Encrypted", "Token", "Hashed", "PII"];

// ── output ────────────────────────────────────────────────────────────────────

/// One VOCAB-AUDIT-002 finding: a handler-only command can mutate sensitive
/// capability-tagged fields without an explicit audit contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file.
    pub path: PathBuf,
    /// Name of the offending command.
    pub command: String,
    /// Invalidated resource carrying sensitive capability-tagged fields.
    pub resource: String,
    /// Sensitive field names on the invalidated resource.
    pub sensitive_fields: Vec<String>,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "VOCAB-AUDIT-002";

    /// Render the message naming the command, invalidated resource, and
    /// the list of sensitive capability-tagged field names.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::vocab::vocab_audit_002::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("f.lzi"),
    ///     command: "rotate_key".into(),
    ///     resource: "Credential".into(),
    ///     sensitive_fields: vec!["secret".into()],
    /// };
    /// assert!(f.message().contains("@cap.*"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "handler-only command `{}` invalidates `{}` which has \
             {} field(s) with sensitive @cap.* tier ({}) but declares no \
             `audit` child — handler-side mutation of capability-tagged \
             fields requires an explicit audit contract. Add \
             `audit default` or `audit <fields>` with a documented reason.",
            self.command,
            self.resource,
            self.sensitive_fields.len(),
            self.sensitive_fields.join(", ")
        )
    }
}

// ── detection ─────────────────────────────────────────────────────────────────

/// Run VOCAB-AUDIT-002 for all commands in one feature.
///
/// `Command.invalidates` currently models `invalidates query.<name>(...)`.
/// This v1 rule intentionally stays conservative: it only fires when that
/// invalidation target name also resolves to a resource in the same feature.
/// Handler/policy body analysis remains outside doctor vocabulary lints.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::vocab::vocab_audit_002::check;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower a feature with handler-only commands");
/// let _ = check(&feature, Path::new("auth.lzi"));
/// ```
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    let resources_by_name: HashMap<&str, &Resource> = feature
        .resources
        .iter()
        .map(|resource| (resource.name.as_str(), resource))
        .collect();

    feature
        .commands
        .iter()
        .filter(|cmd| cmd.audit.is_none())
        .filter(|cmd| is_handler_like(cmd))
        .flat_map(|cmd| {
            sensitive_invalidates(cmd, &resources_by_name)
                .into_iter()
                .map(move |(resource, sensitive_fields)| Finding {
                    path: path.to_path_buf(),
                    command: cmd.name.clone(),
                    resource,
                    sensitive_fields,
                })
        })
        .collect()
}

// ── internals ─────────────────────────────────────────────────────────────────

fn is_handler_like(cmd: &Command) -> bool {
    matches!(cmd.effect, CommandEffect::Returns(_) | CommandEffect::None)
}

fn sensitive_invalidates(
    cmd: &Command,
    by_name: &HashMap<&str, &Resource>,
) -> Vec<(String, Vec<String>)> {
    cmd.invalidates
        .iter()
        .filter_map(|invalidate| {
            let resource_name = invalidate.query.name.as_str();
            by_name
                .get(resource_name)
                .map(|resource| (invalidate.query.name.clone(), sensitive_fields(resource)))
        })
        .filter(|(_, fields)| !fields.is_empty())
        .collect()
}

fn sensitive_fields(resource: &Resource) -> Vec<String> {
    resource
        .fields
        .iter()
        .filter(|field| has_sensitive_capability(field))
        .map(|field| field.name.clone())
        .collect()
}

fn has_sensitive_capability(field: &Field) -> bool {
    match &field.type_ref {
        TypeRef::Capability(capability) => is_sensitive_tier(capability),
        _ => false,
    }
}

fn is_sensitive_tier(capability: &CapabilityRef) -> bool {
    let tier = match capability {
        CapabilityRef::Encrypted(_) => "Encrypted",
        // `@cap.E2ee` is server-blind ciphertext — strictly more
        // sensitive than `@cap.Encrypted` (the server cannot decrypt
        // at all). Treated the same audit-tier-wise.
        CapabilityRef::E2ee(_) => "E2ee",
        CapabilityRef::Token(_) => "Token",
        CapabilityRef::Hashed(_) => "Hashed",
        CapabilityRef::PII(_) => "PII",
        CapabilityRef::File(_) => "File",
    };

    SENSITIVE_TIERS.contains(&tier)
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    include!("vocab_audit_002_tests.rs");
}
