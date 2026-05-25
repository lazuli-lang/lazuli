//! Cross-feature type-resolution diagnostics.
//!
//! Two producers, one shared index:
//!
//! * `cross_feature_type_unresolved_diagnostics` — emits
//!   `cross_feature_type_unresolved` for every `TypeRef::User` site
//!   whose name is not declared anywhere in the workspace.
//! * `feature_uses_missing_diagnostics` — emits `feature_uses_missing`
//!   when a feature references a type declared in another feature it
//!   does not list under `uses`.
//!
//! Both rules walk the same site set (resources + records + commands +
//! queries) and lean on the workspace-wide
//! `DoctorCrossFeatureTypeIndex` (`<TypeName> -> <OwnerFeature>`)
//! built from per-feature records / enums / resources.
//!
//! Wave R7-3 extract.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use lazuli_ir::{self as ir};

use crate::doctor::{
    DoctorDiagnostic, DoctorFile, DoctorSeverity, ResourceFact, Tier3FeatureFacts,
    line_col_for_offset,
};

/// Per-site evidence row for `feature_uses_missing_diagnostics` —
/// the path + line of the `TypeRef::User` mention that triggered the
/// missing-`uses` diagnostic. Aggregated keyed by `(consumer_feature,
/// referenced_type)` so a feature with multiple sites referencing the
/// same external type only fires once.
#[derive(Debug, Clone)]
struct MissingUsesRef {
    path: PathBuf,
    line: usize,
}

pub(crate) fn cross_feature_type_unresolved_diagnostics(
    files: &[DoctorFile],
    tier3_facts: &[Tier3FeatureFacts],
    feature_resources: &BTreeMap<String, BTreeMap<String, ResourceFact>>,
) -> Vec<DoctorDiagnostic> {
    let declared_types = build_cross_feature_declared_type_index(tier3_facts, feature_resources);
    let mut diagnostics = Vec::new();
    let mut reported: BTreeSet<(PathBuf, String, String)> = BTreeSet::new();

    for (feature_name, resources) in feature_resources {
        for (resource_name, resource) in resources {
            for (field_name, field) in &resource.fields {
                push_unresolved_type_ref_diagnostic(
                    &mut diagnostics,
                    &mut reported,
                    &declared_types,
                    &field.type_ref,
                    &resource.path,
                    field.line.max(resource.line).max(1),
                    format!("{feature_name}.{resource_name}.{field_name}"),
                );
            }
        }
    }

    for fact in tier3_facts {
        for record in &fact.records {
            for field in &record.fields {
                push_unresolved_type_ref_diagnostic(
                    &mut diagnostics,
                    &mut reported,
                    &declared_types,
                    &field.type_ref,
                    &fact.path,
                    span_line(files, &fact.path, field.span_ref, fact.feature_line),
                    format!("{}.{}.{}", fact.feature, record.name, field.name),
                );
            }
        }

        for command in &fact.commands {
            let command_line = fact
                .command_lines
                .get(&command.name)
                .copied()
                .unwrap_or_else(|| {
                    span_line(files, &fact.path, command.span_ref, fact.feature_line)
                });

            for slot in &command.route {
                push_unresolved_type_ref_diagnostic(
                    &mut diagnostics,
                    &mut reported,
                    &declared_types,
                    &slot.type_ref,
                    &fact.path,
                    command_line.max(1),
                    format!("{}.{}.route.{}", fact.feature, command.name, slot.name),
                );
            }

            if let ir::CommandInput::Typed(slots) = &command.input {
                for slot in slots {
                    push_unresolved_type_ref_diagnostic(
                        &mut diagnostics,
                        &mut reported,
                        &declared_types,
                        &slot.type_ref,
                        &fact.path,
                        command_line.max(1),
                        format!("{}.{}.input.{}", fact.feature, command.name, slot.name),
                    );
                }
            }

            if let ir::CommandEffect::Returns(returns) = &command.effect {
                push_unresolved_type_ref_diagnostic(
                    &mut diagnostics,
                    &mut reported,
                    &declared_types,
                    &returns.return_type,
                    &fact.path,
                    command_line.max(1),
                    format!("{}.{}.returns", fact.feature, command.name),
                );
            }
        }
    }

    diagnostics
}

pub(crate) fn build_cross_feature_declared_type_index(
    tier3_facts: &[Tier3FeatureFacts],
    feature_resources: &BTreeMap<String, BTreeMap<String, ResourceFact>>,
) -> BTreeSet<String> {
    let mut declared = BTreeSet::new();

    for resources in feature_resources.values() {
        declared.extend(resources.keys().cloned());
    }
    for fact in tier3_facts {
        declared.extend(fact.records.iter().map(|record| record.name.clone()));
        declared.extend(fact.enums.iter().map(|enum_decl| enum_decl.name.clone()));
    }

    declared
}

pub(crate) fn push_unresolved_type_ref_diagnostic(
    diagnostics: &mut Vec<DoctorDiagnostic>,
    reported: &mut BTreeSet<(PathBuf, String, String)>,
    declared_types: &BTreeSet<String>,
    type_ref: &ir::TypeRef,
    path: &Path,
    line: usize,
    site: String,
) {
    let Some(name) = unresolved_bare_user_type_name(type_ref, declared_types) else {
        return;
    };
    if !reported.insert((path.to_path_buf(), site.clone(), name.to_owned())) {
        return;
    }

    diagnostics.push(DoctorDiagnostic {
        path: path.to_path_buf(),
        line,
        column: 1,
        severity: DoctorSeverity::Error,
        code: "cross_feature_type_unresolved".to_owned(),
        message: format!(
            "type `{name}` referenced by `{site}` is not declared in any feature. Add a `resource`/`record`/`enum {name}` block, or check for a typo."
        ),
        category: None,
        feature_name: None,
        construct: None,
        fix: None,
        group: None,
    });
}

pub(crate) fn unresolved_bare_user_type_name<'a>(
    type_ref: &'a ir::TypeRef,
    declared_types: &BTreeSet<String>,
) -> Option<&'a str> {
    match type_ref {
        ir::TypeRef::UserDefined(qname) | ir::TypeRef::EnumRef(qname)
            if qname.feature.is_none()
                && !declared_types.contains(&qname.name)
                && !is_trivial_type_ref_name(&qname.name) =>
        {
            Some(qname.name.as_str())
        }
        _ => None,
    }
}

pub(crate) fn is_trivial_type_ref_name(name: &str) -> bool {
    let trimmed = name.trim();
    trimmed.len() <= 1 || trimmed.starts_with('@')
}

pub(crate) fn span_line(
    files: &[DoctorFile],
    path: &Path,
    span_ref: Option<lazuli_ir::SpanRef>,
    fallback: usize,
) -> usize {
    span_ref
        .and_then(|span| {
            files
                .iter()
                .find(|file| file.path.as_path() == path)
                .map(|file| line_col_for_offset(&file.source, span.start).0)
        })
        .unwrap_or(fallback.max(1))
}

pub(crate) fn feature_uses_missing_diagnostics(
    files: &[DoctorFile],
    tier3_facts: &[Tier3FeatureFacts],
    feature_resources: &BTreeMap<String, BTreeMap<String, ResourceFact>>,
    feature_uses: &BTreeMap<String, BTreeSet<String>>,
) -> Vec<DoctorDiagnostic> {
    let type_owners = DoctorCrossFeatureTypeIndex::build(tier3_facts, feature_resources);
    let mut missing: BTreeMap<(String, String), MissingUsesRef> = BTreeMap::new();

    for (feature_name, resources) in feature_resources {
        for resource in resources.values() {
            for field in resource.fields.values() {
                record_missing_uses_ref(
                    &mut missing,
                    &type_owners,
                    feature_name,
                    &field.type_ref,
                    &resource.path,
                    field.line.max(resource.line).max(1),
                );
            }
        }
    }

    for fact in tier3_facts {
        for record in &fact.records {
            for field in &record.fields {
                record_missing_uses_ref(
                    &mut missing,
                    &type_owners,
                    &fact.feature,
                    &field.type_ref,
                    &fact.path,
                    span_line(files, &fact.path, field.span_ref, fact.feature_line),
                );
            }
        }

        for command in &fact.commands {
            let command_line = fact
                .command_lines
                .get(&command.name)
                .copied()
                .unwrap_or_else(|| {
                    span_line(files, &fact.path, command.span_ref, fact.feature_line)
                })
                .max(1);

            for slot in &command.route {
                record_missing_uses_ref(
                    &mut missing,
                    &type_owners,
                    &fact.feature,
                    &slot.type_ref,
                    &fact.path,
                    command_line,
                );
            }

            if let ir::CommandInput::Typed(slots) = &command.input {
                for slot in slots {
                    record_missing_uses_ref(
                        &mut missing,
                        &type_owners,
                        &fact.feature,
                        &slot.type_ref,
                        &fact.path,
                        command_line,
                    );
                }
            }

            if let ir::CommandEffect::Returns(returns) = &command.effect {
                record_missing_uses_ref(
                    &mut missing,
                    &type_owners,
                    &fact.feature,
                    &returns.return_type,
                    &fact.path,
                    command_line,
                );
            }
        }

        for query in &fact.queries {
            let query_line = fact
                .query_lines
                .get(query.name())
                .copied()
                .unwrap_or_else(|| {
                    span_line(files, &fact.path, query_span_ref(query), fact.feature_line)
                })
                .max(1);
            for slot in query_params(query) {
                record_missing_uses_ref(
                    &mut missing,
                    &type_owners,
                    &fact.feature,
                    &slot.type_ref,
                    &fact.path,
                    query_line,
                );
            }
        }
    }

    missing
        .into_iter()
        .filter(|((feature, dependency), _)| {
            !feature_uses
                .get(feature)
                .map(|uses| uses.contains(dependency))
                .unwrap_or(false)
        })
        .map(|((feature, dependency), site)| DoctorDiagnostic {
            path: site.path,
            line: site.line,
            column: 1,
            severity: DoctorSeverity::Warning,
            code: "feature_uses_missing".to_owned(),
            message: format!(
                "feature `{feature}` references types declared in feature `{dependency}` but does not declare `uses {dependency}` in its header. Add `uses {dependency}` to make the dependency explicit."
            ),
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        })
        .collect()
}

pub(crate) fn record_missing_uses_ref(
    missing: &mut BTreeMap<(String, String), MissingUsesRef>,
    type_owners: &DoctorCrossFeatureTypeIndex,
    feature: &str,
    type_ref: &ir::TypeRef,
    path: &Path,
    line: usize,
) {
    let mut owners = BTreeSet::new();
    collect_cross_feature_type_ref_owners(&mut owners, type_owners, feature, type_ref);
    for owner in owners {
        missing
            .entry((feature.to_owned(), owner))
            .or_insert_with(|| MissingUsesRef {
                path: path.to_path_buf(),
                line: line.max(1),
            });
    }
}

pub(crate) fn collect_cross_feature_type_ref_owners(
    owners: &mut BTreeSet<String>,
    type_owners: &DoctorCrossFeatureTypeIndex,
    feature: &str,
    type_ref: &ir::TypeRef,
) {
    match type_ref {
        ir::TypeRef::UserDefined(qname) | ir::TypeRef::EnumRef(qname) => {
            if qname.name.starts_with('@') {
                return;
            }
            let owner = qname
                .feature
                .as_deref()
                .or_else(|| type_owners.owner(&qname.name));
            if let Some(owner) = owner {
                if owner != feature {
                    owners.insert(owner.to_owned());
                }
            }
        }
        ir::TypeRef::Many(inner) => {
            collect_cross_feature_type_ref_owners(owners, type_owners, feature, inner);
        }
        ir::TypeRef::Unresolved(name) => {
            // Command/query lowering still carries authored custom
            // types as `Unresolved`; the codegen cross-feature pass
            // resolves these names against the same owner index.
            if name.starts_with('@') {
                return;
            }
            if let Some(owner) = type_owners.owner(name) {
                if owner != feature {
                    owners.insert(owner.to_owned());
                }
            }
        }
        _ => {}
    }
}

pub(crate) fn query_params(query: &ir::Query) -> &[ir::TypedSlot] {
    match query {
        ir::Query::List(query) => &query.params,
        ir::Query::Lookup(query) => &query.params,
        ir::Query::Sql(query) => &query.params,
    }
}

pub(crate) fn query_span_ref(query: &ir::Query) -> Option<ir::SpanRef> {
    match query {
        ir::Query::List(query) => query.span_ref,
        ir::Query::Lookup(query) => query.span_ref,
        ir::Query::Sql(query) => query.span_ref,
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct DoctorCrossFeatureTypeIndex {
    map: BTreeMap<String, String>,
    ambiguous: BTreeMap<String, BTreeSet<String>>,
}

impl DoctorCrossFeatureTypeIndex {
    fn build(
        tier3_facts: &[Tier3FeatureFacts],
        feature_resources: &BTreeMap<String, BTreeMap<String, ResourceFact>>,
    ) -> Self {
        let mut index = Self::default();

        for (feature, resources) in feature_resources {
            for resource_name in resources.keys() {
                index.register(resource_name, feature);
            }
        }
        for fact in tier3_facts {
            for record in &fact.records {
                index.register(&record.name, &fact.feature);
            }
            for enum_decl in &fact.enums {
                index.register(&enum_decl.name, &fact.feature);
            }
        }

        index
    }

    fn register(&mut self, name: &str, feature: &str) {
        if let Some(owners) = self.ambiguous.get_mut(name) {
            owners.insert(feature.to_owned());
            return;
        }

        match self.map.remove(name) {
            Some(existing) if existing == feature => {
                self.map.insert(name.to_owned(), existing);
            }
            Some(existing) => {
                let mut owners = BTreeSet::new();
                owners.insert(existing);
                owners.insert(feature.to_owned());
                self.ambiguous.insert(name.to_owned(), owners);
            }
            None => {
                self.map.insert(name.to_owned(), feature.to_owned());
            }
        }
    }

    fn owner(&self, name: &str) -> Option<&str> {
        self.map.get(name).map(String::as_str)
    }
}
