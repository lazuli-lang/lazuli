//! VOCAB-SHADOW-RECORD-001 — multi-declaration shadow.
//!
//! Per `docs/proposals/vocab-shadow-record-vo-extraction.md` v0.2 §4.1.
//! Fires when two or more declaration sites within the SAME feature share
//! N or more `(name, type_ref)` pairs after universal-column filtering AND
//! the intersection is ≥ 50% of each side's post-filter field count.
//!
//! Declaration sites considered: `Resource.fields`, `Record.fields`, and
//! `Command.input` (only when `CommandInput::Typed` — `Short` variants
//! reuse the target resource's fields and contribute no new structural
//! information).
//!
//! Severity: `warning` (strict and production profiles).
//! Reference: docs/proposals/vocab-shadow-record-vo-extraction.md
//!            docs/next-checklist.md §VOCAB-SHADOW-RECORD-001

use std::path::{Path, PathBuf};

use lazuli_ir::{CommandInput, Feature, Module, TypeRef};

use super::universal_columns::{is_universal_column, is_view_projection_record};

/// Minimum cluster size for a finding. Authors can override via
/// `Lazurite.toml [doctor.vocab.shadow_record].min_cluster_fields`.
pub const DEFAULT_MIN_CLUSTER_FIELDS: usize = 4;

/// Minimum intersection ratio relative to each declaration's post-filter
/// field count. Authors can override via
/// `Lazurite.toml [doctor.vocab.shadow_record].min_cluster_ratio`.
pub const DEFAULT_MIN_CLUSTER_RATIO: f64 = 0.5;

/// One VOCAB-SHADOW-RECORD-001 finding: a pair of declarations with a
/// structurally similar field cluster. Per-pair finding; if three
/// declarations all share the cluster, the rule emits 3 pairwise findings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file the offending declarations live in.
    pub path: PathBuf,
    /// Feature owning both declarations.
    pub feature: String,
    /// Left-hand declaration in the pair.
    pub left: DeclarationRef,
    /// Right-hand declaration in the pair.
    pub right: DeclarationRef,
    /// Field names shared by both sides (post universal-column filter).
    pub shared_fields: Vec<String>,
}

/// Lightweight handle to a declaration site that participated in a
/// shadow-record finding — enough metadata to render the diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclarationRef {
    /// Whether the declaration is a resource, record, or command input.
    pub kind: DeclarationKind,
    /// Declaration identifier (resource/record/command name).
    pub name: String,
    /// Field count after universal-column filtering — used to compute
    /// the intersection ratio against the cluster cutoff.
    pub post_filter_field_count: usize,
}

/// Closed catalog of declaration kinds the shadow-record rule inspects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeclarationKind {
    /// `resource <Name>` declaration.
    Resource,
    /// `record <Name>` declaration.
    Record,
    /// `command.* <name>` whose `input:` is a `Typed` record body.
    CommandInput,
}

impl DeclarationKind {
    fn label(&self) -> &'static str {
        match self {
            DeclarationKind::Resource => "resource",
            DeclarationKind::Record => "record",
            DeclarationKind::CommandInput => "command input",
        }
    }
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "VOCAB-SHADOW-RECORD-001";

    /// Render the "share N fields with matching types" message, prompting
    /// for `record` extraction or a documented `# doctor:allow`.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::vocab::vocab_shadow_record_001::{
    ///     Finding, DeclarationRef, DeclarationKind,
    /// };
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("f.lzi"),
    ///     feature: "catalog".into(),
    ///     left: DeclarationRef {
    ///         kind: DeclarationKind::Resource,
    ///         name: "Property".into(),
    ///         post_filter_field_count: 8,
    ///     },
    ///     right: DeclarationRef {
    ///         kind: DeclarationKind::Record,
    ///         name: "PropertySummary".into(),
    ///         post_filter_field_count: 6,
    ///     },
    ///     shared_fields: vec!["title".into(), "city".into(), "price".into(), "host".into()],
    /// };
    /// assert!(f.message().contains("Consider extracting"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "{} `{}` and {} `{}` share {} fields with matching types ({}). \
             Consider extracting a `record` and referencing it from both \
             declarations. If they will diverge, add \
             `# doctor:allow VOCAB-SHADOW-RECORD-001 — reason \"...\"` on \
             each declaration.",
            self.left.kind.label(),
            self.left.name,
            self.right.kind.label(),
            self.right.name,
            self.shared_fields.len(),
            self.shared_fields.join(", "),
        )
    }
}

/// Internal projection of a declaration into a comparable field-set form.
struct DeclarationView {
    kind: DeclarationKind,
    name: String,
    fields: Vec<(String, TypeRef)>,
}

/// Run VOCAB-SHADOW-RECORD-001 over one feature's declarations.
///
/// Delegates to [`check_with_config`] with the default field-count /
/// ratio thresholds. Tests that vary the cutoff call the config
/// variant directly.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::vocab::vocab_shadow_record_001::check;
/// use lazuli_ir::{Feature, Module};
///
/// let module: Module = unimplemented!();
/// let feature = &module.features[0];
/// let _ = check(feature, &module, Path::new("catalog.lzi"));
/// ```
pub fn check(feature: &Feature, module: &Module, path: &Path) -> Vec<Finding> {
    check_with_config(
        feature,
        module,
        path,
        DEFAULT_MIN_CLUSTER_FIELDS,
        DEFAULT_MIN_CLUSTER_RATIO,
    )
}

/// Run VOCAB-SHADOW-RECORD-001 with caller-tuned cluster thresholds.
///
/// Exposed so unit tests and downstream tooling can vary the cluster
/// field-count / ratio cutoffs without touching the defaults.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::vocab::vocab_shadow_record_001::check_with_config;
/// use lazuli_ir::{Feature, Module};
///
/// let module: Module = unimplemented!();
/// let feature = &module.features[0];
/// let _ = check_with_config(feature, &module, Path::new("catalog.lzi"), 3, 0.4);
/// ```
pub fn check_with_config(
    feature: &Feature,
    module: &Module,
    path: &Path,
    min_cluster_fields: usize,
    min_cluster_ratio: f64,
) -> Vec<Finding> {
    let views = collect_declaration_views(feature, module);
    let mut findings = Vec::new();
    for i in 0..views.len() {
        for j in (i + 1)..views.len() {
            let left = &views[i];
            let right = &views[j];
            let mut shared: Vec<String> = left
                .fields
                .iter()
                .filter(|(name, ty)| {
                    right
                        .fields
                        .iter()
                        .any(|(n2, t2)| n2 == name && t2 == ty)
                })
                .map(|(name, _)| name.clone())
                .collect();
            if shared.len() < min_cluster_fields {
                continue;
            }
            let left_total = left.fields.len();
            let right_total = right.fields.len();
            if left_total == 0 || right_total == 0 {
                continue;
            }
            let left_ratio = shared.len() as f64 / left_total as f64;
            let right_ratio = shared.len() as f64 / right_total as f64;
            if left_ratio < min_cluster_ratio || right_ratio < min_cluster_ratio {
                continue;
            }
            shared.sort();
            findings.push(Finding {
                path: path.to_path_buf(),
                feature: feature.name.clone(),
                left: DeclarationRef {
                    kind: left.kind.clone(),
                    name: left.name.clone(),
                    post_filter_field_count: left_total,
                },
                right: DeclarationRef {
                    kind: right.kind.clone(),
                    name: right.name.clone(),
                    post_filter_field_count: right_total,
                },
                shared_fields: shared,
            });
        }
    }
    findings
}

fn collect_declaration_views(feature: &Feature, module: &Module) -> Vec<DeclarationView> {
    let mut out = Vec::new();
    for resource in &feature.resources {
        let fields = resource
            .fields
            .iter()
            .filter(|f| !is_universal_column(f, &resource.name, feature, module))
            .map(|f| (f.name.clone(), f.type_ref.clone()))
            .collect::<Vec<_>>();
        if !fields.is_empty() {
            out.push(DeclarationView {
                kind: DeclarationKind::Resource,
                name: resource.name.clone(),
                fields,
            });
        }
    }
    for record in &feature.records {
        if is_view_projection_record(record) {
            continue;
        }
        let fields = record
            .fields
            .iter()
            .filter(|f| !is_universal_column(f, &record.name, feature, module))
            .map(|f| (f.name.clone(), f.type_ref.clone()))
            .collect::<Vec<_>>();
        if !fields.is_empty() {
            out.push(DeclarationView {
                kind: DeclarationKind::Record,
                name: record.name.clone(),
                fields,
            });
        }
    }
    for command in &feature.commands {
        if let CommandInput::Typed(slots) = &command.input {
            let fields = slots
                .iter()
                .map(|s| (s.name.clone(), s.type_ref.clone()))
                .collect::<Vec<_>>();
            if !fields.is_empty() {
                out.push(DeclarationView {
                    kind: DeclarationKind::CommandInput,
                    name: command.name.clone(),
                    fields,
                });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    include!("vocab_shadow_record_001_tests.rs");
}
