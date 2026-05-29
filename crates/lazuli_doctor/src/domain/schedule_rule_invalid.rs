//! SCHEDULE-RULE-001 — a `schedule_rule from @fn.<name>(<rule_arg>) offset
//! <offset>` field references an undeclared binding `@fn`, an unknown rule
//! argument field, or an invalid offset.
//!
//! W4 GAP-08. Sibling of COMPUTED-DATE-EXPR-001: where the W3
//! `computed_date` picks its base from a same-resource `Date` field, the W4
//! `schedule_rule` picks it via a bound `@fn` selected by a rule-enum arg.
//! This rule fires when, for a field whose `computed_date.base` is the
//! `Rule { rule, fn_ref }` form:
//!  - `fn_ref` is not a declared `fn` (Function) extension on the feature, OR
//!  - the rule argument references a same-resource field that isn't declared
//!    (bare-identifier args only; `input.*` / `ctx.*` prefixed args are
//!    runtime-resolved and accepted), OR
//!  - the `offset <field>` operand is not a declared `Integer` field
//!    (integer literals always type-check).
//!
//! Severity: `error`. A `schedule_rule` whose `@fn` isn't registered, whose
//! rule arg can't bind, or whose offset isn't an integer would emit Go that
//! fails at runtime — Rule Zero (vocabulary, not silent miscompute).
//! The offset check mirrors COMPUTED-DATE-EXPR-001's offset walk verbatim.
//!
//! Reference: GAP-08 (`docs/proposals/ir-pauta-gaps-bundle-2026-05-28.md`).

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use lazuli_ir::{
    BuiltinType, ComputedDateBase, ComputedDateOffset, ExtensionContract, Feature, Resource,
    TypeRef,
};

/// What about a `schedule_rule` field failed to resolve.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reason {
    /// `@fn.<name>` names a binding fn with no `fn` (Function) extension.
    FnUnknown,
    /// The rule argument names a same-resource field that isn't declared.
    RuleArgUnknown,
    /// `offset <field>` names a field not declared on the resource.
    OffsetUnknown,
    /// `offset <field>` is declared but is not an `Integer` field.
    OffsetNotInteger,
}

impl Reason {
    fn describe(&self, operand: &str) -> String {
        match self {
            Reason::FnUnknown => {
                format!("`@fn.{operand}` is not a declared `fn` (Function) extension")
            }
            Reason::RuleArgUnknown => {
                format!("rule argument `{operand}` is not a declared field")
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

/// One SCHEDULE-RULE-001 finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` path that hosts the resource.
    pub path: PathBuf,
    /// Feature containing the resource.
    pub feature: String,
    /// Resource whose `schedule_rule` field is unresolved.
    pub resource: String,
    /// The `schedule_rule` field whose base/offset failed to resolve.
    pub field: String,
    /// The offending operand (fn name / rule arg / offset identifier).
    pub operand: String,
    /// What failed.
    pub reason: Reason,
}

impl Finding {
    /// Stable diagnostic code used by the dispatcher and JSON output.
    pub const CODE: &'static str = "SCHEDULE-RULE-001";

    /// Render the user-facing diagnostic body.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use std::path::PathBuf;
    /// use lazuli_doctor::domain::schedule_rule_invalid::{Finding, Reason};
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("activity.lzi"),
    ///     feature: "activity".into(),
    ///     resource: "Activity".into(),
    ///     field: "due_date".into(),
    ///     operand: "pick_date".into(),
    ///     reason: Reason::FnUnknown,
    /// };
    /// assert!(f.message().contains("due_date"));
    /// assert!(f.message().contains("pick_date"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "`schedule_rule` field `{}` on resource `{}` is invalid: {}. The \
             base `@fn.<name>` must be a declared `fn` extension, the rule \
             argument must reference a declared field (or be an `input.*` / \
             `ctx.*` ref), and the `offset` must reference a declared \
             `Integer` field or be an integer literal.",
            self.field,
            self.resource,
            self.reason.describe(&self.operand),
        )
    }
}

/// Run SCHEDULE-RULE-001 over one feature.
///
/// Only fields whose `computed_date.base` is the W4 `Rule` form are checked.
/// The `fn_ref` is resolved against the feature's `fn` (Function)
/// extensions; the rule argument + offset are resolved against the
/// declaring resource's own fields.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::domain::schedule_rule_invalid::check;
///
/// let findings = check(&feature, Path::new("activity.lzi"));
/// ```
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    let declared_fns: HashSet<&str> = feature
        .extensions
        .iter()
        .filter(|ext| matches!(ext.contract, ExtensionContract::Function { .. }))
        .map(|ext| ext.name.as_str())
        .collect();

    let mut findings = Vec::new();

    for resource in &feature.resources {
        for field in &resource.fields {
            let Some(cd) = &field.computed_date else {
                continue;
            };
            let ComputedDateBase::Rule { rule, fn_ref } = &cd.base else {
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

            // 1. `@fn.<name>` must be a declared `fn` extension.
            if !declared_fns.contains(fn_ref.as_str()) {
                push(fn_ref.clone(), Reason::FnUnknown);
            }

            // 2. rule arg must reference a declared field — unless it is a
            //    runtime-resolved `input.*` / `ctx.*` ref (accepted as-is).
            if !is_runtime_ref(rule) {
                // Strip a leading dotted path's first segment for matching
                // (e.g. `row.rule` → `rule` is not how fields are named;
                // a bare same-resource field is expected for non-prefixed).
                let bare = rule.split('.').next().unwrap_or(rule);
                if find_field_type(resource, bare).is_none() {
                    push(rule.clone(), Reason::RuleArgUnknown);
                }
            }

            // 3. offset: a field ref must be a declared `Integer` field;
            //    an integer literal always type-checks. (Mirrors
            //    COMPUTED-DATE-EXPR-001 verbatim.)
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

/// `input.*` / `ctx.*` rule arguments are resolved by the runtime, not
/// against a declared field — accept them.
fn is_runtime_ref(rule: &str) -> bool {
    rule.starts_with("input.") || rule.starts_with("ctx.")
}

/// Look up the `TypeRef` of a field on the resource by bare name.
fn find_field_type<'a>(resource: &'a Resource, name: &str) -> Option<&'a TypeRef> {
    resource
        .fields
        .iter()
        .find(|f| f.name == name)
        .map(|f| &f.type_ref)
}

fn is_integer(type_ref: &TypeRef) -> bool {
    matches!(type_ref, TypeRef::Builtin(BuiltinType::Integer))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        ComputedDate, ComputedDateBase, Defaults, Extension, ExtensionContract, Field,
        FieldConstraints, PathRef, Policies, Resource, TypeRef,
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

    fn mk_rule_field(name: &str, rule: &str, fn_ref: &str, offset: ComputedDateOffset) -> Field {
        let mut f = mk_field(name, BuiltinType::Date);
        f.computed_date = Some(ComputedDate {
            base: ComputedDateBase::Rule {
                rule: rule.into(),
                fn_ref: fn_ref.into(),
            },
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

    fn mk_fn_extension(name: &str) -> Extension {
        Extension {
            name: name.into(),
            contract: ExtensionContract::Function {
                input: TypeRef::Builtin(BuiltinType::Json),
                output: TypeRef::Builtin(BuiltinType::Date),
            },
            resolved_path: PathRef::authored("./handlers/x.go"),
            previous_names: vec![],
            span_ref: None,
        }
    }

    fn mk_feature(resources: Vec<Resource>, extensions: Vec<Extension>) -> Feature {
        Feature {
            name: "activity".into(),
            purpose: None,
            non_goals: vec![],
            context_path: None,
            knowledge: None,
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
            extensions,
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
    fn positive_valid_schedule_rule_passes() {
        // declared @fn, input.* rule arg, Integer offset field.
        let resource = mk_resource(
            "Activity",
            vec![
                mk_field("offset_days", BuiltinType::Integer),
                mk_rule_field(
                    "due_date",
                    "input.rule",
                    "activity_date_rule",
                    ComputedDateOffset::Field("offset_days".into()),
                ),
            ],
        );
        let feature = mk_feature(vec![resource], vec![mk_fn_extension("activity_date_rule")]);
        assert!(check(&feature, Path::new("a.lzi")).is_empty());
    }

    #[test]
    fn negative_unknown_fn_fires() {
        let resource = mk_resource(
            "Activity",
            vec![mk_rule_field(
                "due_date",
                "input.rule",
                "ghost_rule",
                ComputedDateOffset::Literal(7),
            )],
        );
        let feature = mk_feature(vec![resource], vec![]);
        let findings = check(&feature, Path::new("a.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].reason, Reason::FnUnknown);
        assert_eq!(findings[0].operand, "ghost_rule");
        assert_eq!(Finding::CODE, "SCHEDULE-RULE-001");
    }

    #[test]
    fn negative_unknown_rule_arg_field_fires() {
        // bare (non-input/ctx) rule arg that isn't a declared field.
        let resource = mk_resource(
            "Activity",
            vec![mk_rule_field(
                "due_date",
                "ghost_field",
                "activity_date_rule",
                ComputedDateOffset::Literal(7),
            )],
        );
        let feature = mk_feature(vec![resource], vec![mk_fn_extension("activity_date_rule")]);
        let findings = check(&feature, Path::new("a.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].reason, Reason::RuleArgUnknown);
    }

    #[test]
    fn negative_offset_not_integer_fires() {
        let resource = mk_resource(
            "Activity",
            vec![
                mk_field("some_date", BuiltinType::Date),
                mk_rule_field(
                    "due_date",
                    "input.rule",
                    "activity_date_rule",
                    ComputedDateOffset::Field("some_date".into()),
                ),
            ],
        );
        let feature = mk_feature(vec![resource], vec![mk_fn_extension("activity_date_rule")]);
        let findings = check(&feature, Path::new("a.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].reason, Reason::OffsetNotInteger);
    }

    #[test]
    fn w3_computed_date_field_base_is_ignored() {
        // A W3 `computed_date` (Field base) must not be touched by this rule.
        let mut f = mk_field("due_date", BuiltinType::Date);
        f.computed_date = Some(ComputedDate {
            base: ComputedDateBase::Field("start".into()),
            offset: ComputedDateOffset::Literal(7),
        });
        let resource = mk_resource("Activity", vec![mk_field("start", BuiltinType::Date), f]);
        let feature = mk_feature(vec![resource], vec![]);
        assert!(check(&feature, Path::new("a.lzi")).is_empty());
    }
}
