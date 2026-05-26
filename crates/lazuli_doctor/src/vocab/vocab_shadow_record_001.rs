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
    pub path: PathBuf,
    pub feature: String,
    pub left: DeclarationRef,
    pub right: DeclarationRef,
    pub shared_fields: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclarationRef {
    pub kind: DeclarationKind,
    pub name: String,
    pub post_filter_field_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeclarationKind {
    Resource,
    Record,
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
    pub const CODE: &'static str = "VOCAB-SHADOW-RECORD-001";

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
pub fn check(feature: &Feature, module: &Module, path: &Path) -> Vec<Finding> {
    check_with_config(
        feature,
        module,
        path,
        DEFAULT_MIN_CLUSTER_FIELDS,
        DEFAULT_MIN_CLUSTER_RATIO,
    )
}

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
