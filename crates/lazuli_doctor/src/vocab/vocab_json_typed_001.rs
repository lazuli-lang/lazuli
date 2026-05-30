//! VOCAB-JSON-TYPED-001 — untyped JSON bag + sibling closed-catalog enum.
//!
//! ## Rule statement
//!
//! Fires when a resource carries a `JSON` field while the same feature declares
//! a thematically related enum that no typed slot references. The pattern means
//! the author has documented a closed shape, such as quiz question kinds, but
//! the IR still sees an unconstrained JSON bag; downstream codegen, validation,
//! and clients cannot enforce the enum.
//!
//! ## Severity profile
//!
//! Severity: `warning` in both strict and production profiles. The rule is a
//! vocabulary-fitness lint, not a correctness error, because raw JSON can still
//! be intentional for opaque third-party blobs.
//!
//! ## Fixture example
//!
//! ```lzi
//! feature quiz
//!   enum QuizQuestionType
//!     MultipleChoice
//!     TrueFalse
//!   resource Quiz
//!     title: Text required
//!     questions: JSON required
//! ```
//!
//! Canonical fix:
//!
//! ```lzi
//! feature quiz
//!   enum QuizQuestionType
//!     MultipleChoice
//!     TrueFalse
//!   record QuizQuestion
//!     kind: QuizQuestionType required
//!     text: Text required
//!   resource Quiz
//!     title: Text required
//!     questions: Many<QuizQuestion> required
//! ```
//!
//! ## Proposal anchor
//!
//! Historical proposal: `docs/proposals/doctor-vocabulary-lints.md`
//! §VOCAB-JSON-TYPED-001 (extracted to `lazuli-ops` in commit `acbc3c14`).
//!
//! Diagnostic ID / code constant: `VOCAB-JSON-TYPED-001`;
//! `Finding::CODE` is `pub const CODE: &'static str =
//! "VOCAB-JSON-TYPED-001";`.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use lazuli_ir::{
    BuiltinType, CommandEffect, CommandInput, EnumDecl, Feature, Field, JobBody, Query, Resource,
    TypeRef, TypedSlot,
};

use crate::allow_comment::file_contains_doctor_allow;

// ── output ───────────────────────────────────────────────────────────────────

/// One VOCAB-JSON-TYPED-001 finding: a JSON bag with an orphan enum catalog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file.
    pub path: PathBuf,
    /// Resource name.
    pub resource: String,
    /// The untyped JSON field.
    pub json_field: String,
    /// Related same-feature enum that is not referenced anywhere.
    pub orphan_enum: String,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "VOCAB-JSON-TYPED-001";

    /// Render the "untyped JSON sibling enum" message and prompt for
    /// either a discriminated union or a `record` to lift the contract.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::vocab::vocab_json_typed_001::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("f.lzi"),
    ///     resource: "Webhook".into(),
    ///     json_field: "payload".into(),
    ///     orphan_enum: "WebhookKind".into(),
    /// };
    /// assert!(f.message().contains("discriminated union"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "resource `{}` has untyped `{}: JSON` field with sibling enum `{}` \
             that documents the shape but isn't referenced anywhere — \
             consider a discriminated union OR a `record` type so the IR \
             carries the constraint, not just the documentation.",
            self.resource, self.json_field, self.orphan_enum
        )
    }
}

// ── detection ────────────────────────────────────────────────────────────────

/// Run VOCAB-JSON-TYPED-001 over one feature's resources.
///
/// `path` is the source `.lzi` file — used to anchor findings; no I/O is
/// performed here.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::vocab::vocab_json_typed_001::check;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower a feature with JSON fields + enums");
/// let _ = check(&feature, Path::new("billing.lzi"));
/// ```
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    // Whole-file opt-out — silence ALL findings from this `.lzi` when
    // the canonical allow comment is present. Mirrors the pattern used
    // by `migration_alter_missing_001` and the lifecycle rules; closes
    // the gap where the rule's own message advertised the opt-out but
    // never honored it.
    if file_contains_doctor_allow(path, Finding::CODE) {
        return Vec::new();
    }
    let referenced_enums = collect_referenced_enums(feature);
    feature
        .resources
        .iter()
        .flat_map(|resource| check_resource(resource, &feature.enums, &referenced_enums, path))
        .collect()
}

// ── internals ────────────────────────────────────────────────────────────────

fn collect_referenced_enums(feature: &Feature) -> HashSet<&str> {
    let mut refs = HashSet::new();

    for resource in &feature.resources {
        collect_field_refs(&resource.fields, &mut refs);
    }
    for record in &feature.records {
        collect_field_refs(&record.fields, &mut refs);
    }
    for event in &feature.events {
        for field in &event.payload {
            collect_type_ref(&field.type_ref, &mut refs);
        }
    }
    for command in &feature.commands {
        for route_slot in &command.route {
            collect_type_ref(&route_slot.type_ref, &mut refs);
        }
        if let CommandInput::Typed(slots) = &command.input {
            collect_typed_slot_refs(slots, &mut refs);
        }
        if let CommandEffect::Returns(effect) = &command.effect {
            collect_type_ref(&effect.return_type, &mut refs);
        }
    }
    for query in &feature.queries {
        collect_query_refs(query, &mut refs);
    }
    for job in &feature.jobs {
        if let JobBody::Handler(handler) = &job.body {
            if let Some(return_type) = &handler.returns {
                collect_type_ref(return_type, &mut refs);
            }
        }
    }
    for webhook in &feature.webhooks {
        if let Some(return_type) = &webhook.returns {
            collect_type_ref(return_type, &mut refs);
        }
    }

    refs
}

fn collect_field_refs<'a>(fields: &'a [Field], refs: &mut HashSet<&'a str>) {
    for field in fields {
        collect_type_ref(&field.type_ref, refs);
    }
}

fn collect_typed_slot_refs<'a>(slots: &'a [TypedSlot], refs: &mut HashSet<&'a str>) {
    for slot in slots {
        collect_type_ref(&slot.type_ref, refs);
    }
}

fn collect_query_refs<'a>(query: &'a Query, refs: &mut HashSet<&'a str>) {
    match query {
        Query::List(q) => collect_typed_slot_refs(&q.params, refs),
        Query::Lookup(q) => collect_typed_slot_refs(&q.params, refs),
        Query::Sql(q) => {
            collect_typed_slot_refs(&q.params, refs);
            collect_type_ref(&q.returns, refs);
        }
    }
}

fn collect_type_ref<'a>(type_ref: &'a TypeRef, refs: &mut HashSet<&'a str>) {
    match type_ref {
        TypeRef::EnumRef(qn) => {
            refs.insert(qn.name.as_str());
        }
        TypeRef::Many(inner) => collect_type_ref(inner, refs),
        _ => {}
    }
}

fn check_resource(
    resource: &Resource,
    enums: &[EnumDecl],
    referenced: &HashSet<&str>,
    path: &Path,
) -> Vec<Finding> {
    if resource.fields.len() < 2 {
        return vec![];
    }

    let json_fields: Vec<&Field> = resource
        .fields
        .iter()
        .filter(|field| matches!(&field.type_ref, TypeRef::Builtin(BuiltinType::Json)))
        .collect();

    if json_fields.is_empty() {
        return vec![];
    }

    let mut findings = Vec::new();
    for json_field in json_fields {
        // Future false-positive guard: when source-map facts expose comments,
        // suppress fields with an explicit `# typed-by <enum>` pragma.
        for enum_decl in enums {
            if referenced.contains(enum_decl.name.as_str()) {
                continue;
            }
            if !thematically_related(&enum_decl.name, &json_field.name, &resource.name) {
                continue;
            }

            findings.push(Finding {
                path: path.to_path_buf(),
                resource: resource.name.clone(),
                json_field: json_field.name.clone(),
                orphan_enum: enum_decl.name.clone(),
            });
        }
    }
    findings
}

fn thematically_related(enum_name: &str, field_name: &str, resource_name: &str) -> bool {
    let enum_lower = enum_name.to_lowercase();
    let field_lower = field_name.to_lowercase();
    let resource_lower = resource_name.to_lowercase();
    enum_lower.contains(&field_lower)
        || enum_lower.contains(&format!("{resource_lower}type"))
        || enum_lower.starts_with(&resource_lower)
}

// ── tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    include!("vocab_json_typed_001_tests.rs");
}
