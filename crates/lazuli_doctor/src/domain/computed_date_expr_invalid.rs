//! COMPUTED-DATE-EXPR-001 — a `computed_date from <base> offset <offset>`
//! field references a base/offset that doesn't type-check on the same
//! resource.
//!
//! W3 GAP-03. Fires when, for a field whose kind is `computed_date`:
//!  - the `from <base>` operand is not a declared field on the resource,
//!    OR is declared but is not a `Date` field, OR
//!  - the `offset <offset>` operand is a field reference that is not a
//!    declared field, OR is declared but is not an `Integer` field.
//!    (Integer literals always type-check — they are a count of days.)
//!
//! Severity: `error`. A computed-date column whose base/offset can't bind
//! to a typed field would emit Go (`time.Parse` on a non-Date column,
//! `int(m.<offset>)` on a non-Integer column) that fails to compile or
//! computes garbage — Rule Zero (vocabulary, not silent miscompute).
//! Mirrors the same-resource field-reference walk in
//! `constraint_unique_when_invalid` so the two checks stay aligned.
//!
//! Reference: GAP-03 (`docs/proposals/ir-pauta-gaps-bundle-2026-05-28.md`).

use std::path::{Path, PathBuf};

use lazuli_ir::{BuiltinType, ComputedDateOffset, Feature, TypeRef};

/// What about a `computed_date` field failed to resolve.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reason {
    /// `from <base>` names a field not declared on the resource.
    BaseUnknown,
    /// `from <base>` is declared but is not a `Date` field.
    BaseNotDate,
    /// `offset <field>` names a field not declared on the resource.
    OffsetUnknown,
    /// `offset <field>` is declared but is not an `Integer` field.
    OffsetNotInteger,
}

impl Reason {
    fn describe(&self, operand: &str) -> String {
        match self {
            Reason::BaseUnknown => {
                format!("base `{operand}` is not a declared field")
            }
            Reason::BaseNotDate => {
                format!("base `{operand}` is not a `Date` field")
            }
            Reason::OffsetUnknown => {
                format!("offset `{operand}` is not a declared field")
            }
            Reason::OffsetNotInteger => {
                format!("offset `{operand}` is not an `Integer` field")
            }
        }
    }
}

/// One COMPUTED-DATE-EXPR-001 finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` path that hosts the resource.
    pub path: PathBuf,
    /// Feature containing the resource.
    pub feature: String,
    /// Resource whose `computed_date` field is unresolved.
    pub resource: String,
    /// The `computed_date` field whose base/offset failed to resolve.
    pub field: String,
    /// The offending operand (the base or offset identifier).
    pub operand: String,
    /// What failed.
    pub reason: Reason,
}

impl Finding {
    /// Stable diagnostic code used by the dispatcher and JSON output.
    pub const CODE: &'static str = "COMPUTED-DATE-EXPR-001";

    /// Render the user-facing diagnostic body.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use std::path::PathBuf;
    /// use lazuli_doctor::domain::computed_date_expr_invalid::{Finding, Reason};
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("campaign.lzi"),
    ///     feature: "campaign".into(),
    ///     resource: "Campaign".into(),
    ///     field: "due_date".into(),
    ///     operand: "ghost".into(),
    ///     reason: Reason::BaseUnknown,
    /// };
    /// assert!(f.message().contains("due_date"));
    /// assert!(f.message().contains("ghost"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "`computed_date` field `{}` on resource `{}` is invalid: {}. The \
             `from` base must reference a declared `Date` field and the `offset` \
             must reference a declared `Integer` field or be an integer literal.",
            self.field,
            self.resource,
            self.reason.describe(&self.operand),
        )
    }
}

/// Run COMPUTED-DATE-EXPR-001 over one feature.
///
/// Only fields whose `computed_date` slot is `Some` are checked. The base
/// and offset references are resolved against the declaring resource's own
/// fields (same-resource only — there is no cross-resource computed date).
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::domain::computed_date_expr_invalid::check;
///
/// let findings = check(&feature, Path::new("campaign.lzi"));
/// ```
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    let mut findings = Vec::new();

    for resource in &feature.resources {
        for field in &resource.fields {
            let Some(cd) = &field.computed_date else {
                continue;
            };

            let mut push = |operand: String, reason: Reason| {
                findings.push(Finding {
                    path: path.to_path_buf(),
                    feature: feature.name.clone(),
                    resource: resource.name.clone(),
                    field: field.name.clone(),
                    operand,
                    reason,
                });
            };

            // 1. base must be a declared `Date` field. The W4 `schedule_rule`
            //    rule-base form (`ComputedDateBase::Rule`) is checked by
            //    SCHEDULE-RULE-001 instead — `field()` returns `None` for it,
            //    so this same-resource field walk only applies to the W3 form.
            if let Some(base_field) = cd.base.field() {
                match find_field_type(resource, base_field) {
                    None => push(base_field.to_owned(), Reason::BaseUnknown),
                    Some(t) if !is_date(t) => push(base_field.to_owned(), Reason::BaseNotDate),
                    Some(_) => {}
                }
            }

            // 2. offset: a field ref must be a declared `Integer` field;
            //    an integer literal always type-checks.
            if let ComputedDateOffset::Field(offset_field) = &cd.offset {
                match find_field_type(resource, offset_field) {
                    None => push(offset_field.clone(), Reason::OffsetUnknown),
                    Some(t) if !is_integer(t) => {
                        push(offset_field.clone(), Reason::OffsetNotInteger)
                    }
                    Some(_) => {}
                }
            }
        }
    }

    findings
}

/// Look up the `TypeRef` of a field on the resource by bare name.
fn find_field_type<'a>(resource: &'a lazuli_ir::Resource, name: &str) -> Option<&'a TypeRef> {
    resource
        .fields
        .iter()
        .find(|f| f.name == name)
        .map(|f| &f.type_ref)
}

fn is_date(type_ref: &TypeRef) -> bool {
    matches!(type_ref, TypeRef::Builtin(BuiltinType::Date))
}

fn is_integer(type_ref: &TypeRef) -> bool {
    matches!(type_ref, TypeRef::Builtin(BuiltinType::Integer))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        ComputedDate, ComputedDateBase, Defaults, Field, FieldConstraints, Policies, Resource,
        TypeRef,
    };

    fn mk_field(name: &str, builtin: BuiltinType) -> Field {
        Field {
            name: name.into(),
            type_ref: TypeRef::Builtin(builtin),
            required: true,
            unique: false,
            slug: false,
            default: None,
            derived_from: None,
            computed_date: None,
            constraints: FieldConstraints::default(),
            full_text: false,
            previous_names: vec![],
            pii: None,
            owner_axis: None,
            cross_feature_target: None,
            span_ref: None,
        }
    }

    fn mk_computed(name: &str, base: &str, offset: ComputedDateOffset) -> Field {
        let mut f = mk_field(name, BuiltinType::Date);
        f.computed_date = Some(ComputedDate {
            base: ComputedDateBase::Field(base.into()),
            offset,
        });
        f
    }

    fn mk_resource(name: &str, fields: Vec<Field>) -> Resource {
        Resource {
            name: name.into(),
            public_contract: None,
            tenancy: None,
            soft_delete: false,
            timestamps: None,
            fields,
            constraints: vec![],
            validate: None,
            validates: vec![],
            retention: None,
            previous_names: vec![],
            span_ref: None,
            lifecycle: None,
            invariants: vec![],
            lock: None,
            composite_key: None,
            conventions: Vec::new(),
            lifecycle_routes: None,
            polymorphic_refs: Vec::new(),
            many_through: Vec::new(),
            append_only: false,
        }
    }

    fn mk_feature(resources: Vec<Resource>) -> Feature {
        Feature {
            name: "campaign".into(),
            purpose: None,
            non_goals: vec![],
            context_path: None,
            defaults: Defaults::default(),
            uses: vec![],
            uses_spans: Vec::new(),
            uses_versions: Vec::new(),
            requirements: vec![],
            enums: vec![],
            resources,
            events: vec![],
            rules: vec![],
            policies: Policies::default(),
            errors: None,
            commands: vec![],
            apis: vec![],
            records: vec![],
            queries: vec![],
            resume_routers: vec![],
            workflows: vec![],
            jobs: vec![],
            webhooks: vec![],
            notifications: vec![],
            event_groups: vec![],
            tenant_migrations: vec![],
            translation: None,
            pollers: vec![],
            auth: None,
            surfaces: vec![],
            extensions: vec![],
            escape_routes: vec![],
            agents: vec![],
            reports: vec![],
            channels: vec![],
            caches: vec![],
            aggregates: vec![],
            mcp_servers: vec![],
            previous_names: vec![],
            synth_origins: std::collections::BTreeMap::new(),
            span_ref: None,
        }
    }

    #[test]
    fn positive_valid_computed_date_passes() {
        // base = Date field, offset = Integer field. Both resolve.
        let resource = mk_resource(
            "Campaign",
            vec![
                mk_field("campaign_start", BuiltinType::Date),
                mk_field("offset_days", BuiltinType::Integer),
                mk_computed(
                    "due_date",
                    "campaign_start",
                    ComputedDateOffset::Field("offset_days".into()),
                ),
            ],
        );
        let feature = mk_feature(vec![resource]);
        assert!(check(&feature, Path::new("c.lzi")).is_empty());
    }

    #[test]
    fn positive_integer_literal_offset_passes() {
        let resource = mk_resource(
            "Campaign",
            vec![
                mk_field("campaign_start", BuiltinType::Date),
                mk_computed(
                    "due_date",
                    "campaign_start",
                    ComputedDateOffset::Literal(30),
                ),
            ],
        );
        let feature = mk_feature(vec![resource]);
        assert!(check(&feature, Path::new("c.lzi")).is_empty());
    }

    #[test]
    fn negative_base_not_a_date_field_fires() {
        // base references an Integer field, not a Date field.
        let resource = mk_resource(
            "Campaign",
            vec![
                mk_field("campaign_start", BuiltinType::Integer),
                mk_computed(
                    "due_date",
                    "campaign_start",
                    ComputedDateOffset::Literal(30),
                ),
            ],
        );
        let feature = mk_feature(vec![resource]);
        let findings = check(&feature, Path::new("c.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].reason, Reason::BaseNotDate);
        assert_eq!(findings[0].operand, "campaign_start");
        assert_eq!(Finding::CODE, "COMPUTED-DATE-EXPR-001");
    }

    #[test]
    fn negative_base_unknown_field_fires() {
        let resource = mk_resource(
            "Campaign",
            vec![mk_computed(
                "due_date",
                "ghost",
                ComputedDateOffset::Literal(30),
            )],
        );
        let feature = mk_feature(vec![resource]);
        let findings = check(&feature, Path::new("c.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].reason, Reason::BaseUnknown);
        assert_eq!(findings[0].operand, "ghost");
    }

    #[test]
    fn negative_offset_unknown_field_fires() {
        let resource = mk_resource(
            "Campaign",
            vec![
                mk_field("campaign_start", BuiltinType::Date),
                mk_computed(
                    "due_date",
                    "campaign_start",
                    ComputedDateOffset::Field("ghost".into()),
                ),
            ],
        );
        let feature = mk_feature(vec![resource]);
        let findings = check(&feature, Path::new("c.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].reason, Reason::OffsetUnknown);
        assert_eq!(findings[0].operand, "ghost");
    }

    #[test]
    fn negative_offset_not_integer_fires() {
        // offset references a Date field, not an Integer field.
        let resource = mk_resource(
            "Campaign",
            vec![
                mk_field("campaign_start", BuiltinType::Date),
                mk_field("some_date", BuiltinType::Date),
                mk_computed(
                    "due_date",
                    "campaign_start",
                    ComputedDateOffset::Field("some_date".into()),
                ),
            ],
        );
        let feature = mk_feature(vec![resource]);
        let findings = check(&feature, Path::new("c.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].reason, Reason::OffsetNotInteger);
        assert_eq!(findings[0].operand, "some_date");
    }

    #[test]
    fn non_computed_fields_are_ignored() {
        let resource = mk_resource(
            "Campaign",
            vec![mk_field("campaign_start", BuiltinType::Date)],
        );
        let feature = mk_feature(vec![resource]);
        assert!(check(&feature, Path::new("c.lzi")).is_empty());
    }
}
