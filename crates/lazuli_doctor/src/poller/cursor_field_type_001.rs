//! POLLER-CURSOR-FIELD-TYPE-001 — `cursor.eligible_when` fields must be
//! `DateTime`.
//!
//! Pollers walk pending rows by checking `eligible_when` timestamps;
//! integer/text columns can't be compared with `ctx.now()`. Doctor rejects
//! any non-DateTime field referenced in `cursor.eligible_when`.
//!
//! Severity: error / error.
//! Reference: docs/proposals/poller-vocab.md §5.

use std::path::{Path, PathBuf};

use lazuli_ir::{BuiltinType, Feature, Poller, Resource, TypeRef};

/// Stable diagnostic code emitted with every finding from this rule.
pub const CODE: &str = "POLLER-CURSOR-FIELD-TYPE-001";

/// One POLLER-CURSOR-FIELD-TYPE-001 finding — a poller's
/// `cursor.eligible_when` field is declared with a type other than
/// `DateTime`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file the poller was authored in.
    pub path: PathBuf,
    /// Feature name (mirrors the `.lzi` feature header).
    pub feature: String,
    /// Poller carrying the offending cursor reference.
    pub poller: String,
    /// Resource the poller reads from (`source`).
    pub source: String,
    /// Cursor field the poller references.
    pub field: String,
    /// Debug name of the actual type the field carries.
    pub found: String,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding (alias of the
    /// module-level [`CODE`]).
    pub const CODE: &'static str = CODE;

    /// Render the "cursor field must be DateTime" message, naming the
    /// poller, field, and the actual type that was found.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::poller::cursor_field_type_001::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("messages.lzi"),
    ///     feature: "messages".into(),
    ///     poller: "deliver_pending".into(),
    ///     source: "Message".into(),
    ///     field: "send_at".into(),
    ///     found: "Text".into(),
    /// };
    /// assert!(f.message().contains("DateTime"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "{}: poller '{}' cursor.eligible_when field '{}' must be DateTime; found {}.",
            Self::CODE,
            self.poller,
            self.field,
            self.found,
        )
    }
}

/// Walk every poller in `feature` and emit a finding for each
/// `cursor.eligible_when` reference whose target field on the source
/// resource is not `DateTime`. Pollers with cross-feature / unknown
/// sources are skipped (handled by sibling poller-source rules).
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::poller::cursor_field_type_001::check;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower a feature with a poller cursor on a Text field");
/// let _ = check(&feature, Path::new("messages.lzi"));
/// ```
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    let mut findings = Vec::new();
    for poller in &feature.pollers {
        let Some(resource) = feature.resources.iter().find(|r| r.name == poller.source) else {
            // Cross-feature / unknown source is handled by separate poller
            // source rules; skip here so this rule only reports type mismatch.
            continue;
        };

        findings.extend(
            check_poller(poller, resource)
                .into_iter()
                .map(|mut finding| {
                    finding.path = path.to_path_buf();
                    finding.feature = feature.name.clone();
                    finding
                }),
        );
    }
    findings
}

fn check_poller(poller: &Poller, source: &Resource) -> Vec<Finding> {
    let mut findings = Vec::new();
    for field_name in [
        &poller.cursor.next_at_field,
        &poller.cursor.resolved_at_field,
    ] {
        let Some(field) = source.fields.iter().find(|f| &f.name == field_name) else {
            // POLLER-CURSOR-MISSING-001 owns missing cursor fields.
            continue;
        };

        if !matches!(field.type_ref, TypeRef::Builtin(BuiltinType::DateTime)) {
            findings.push(Finding {
                path: PathBuf::new(),
                feature: String::new(),
                poller: poller.name.clone(),
                source: source.name.clone(),
                field: field.name.clone(),
                found: type_name(&field.type_ref),
            });
        }
    }
    findings
}

fn type_name(type_ref: &TypeRef) -> String {
    match type_ref {
        TypeRef::Builtin(builtin) => format!("{builtin:?}"),
        TypeRef::UserDefined(name) => format!("{name:?}"),
        TypeRef::EnumRef(name) => format!("{name:?}"),
        TypeRef::Many(inner) => format!("Many<{}>", type_name(inner)),
        TypeRef::Unresolved(name) => name.clone(),
        TypeRef::Capability(capability) => format!("{capability:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        Defaults, Field, FieldConstraints, HandlerRef, IdempotencyKey, Path as IrPath, Policies,
        PollerBackoff, PollerCursor, PollerRetry, PollerState, PollerStateKind, PollerTick,
    };

    fn mk_poller(name: &str, next_at: &str, resolved_at: &str) -> Poller {
        Poller {
            name: name.into(),
            source: "Src".into(),
            cursor: PollerCursor {
                next_at_field: next_at.into(),
                resolved_at_field: resolved_at.into(),
                attempts_field: "attempts".into(),
                span_ref: None,
            },
            retry: PollerRetry {
                max_attempts: 30,
                backoff: PollerBackoff::Fixed { base: None },
                span_ref: None,
            },
            states: vec![PollerState {
                name: "resolved".into(),
                kind: PollerStateKind::Terminal,
                span_ref: None,
            }],
            resolve_handler: HandlerRef {
                namespace: "fn".into(),
                name: "h".into(),
                span_ref: None,
            },
            terminal_status_field: None,
            terminal_result_field: None,
            tick: PollerTick {
                every: "30s".into(),
                batch: 100,
            },
            tenant_from: None,
            idempotency: IdempotencyKey {
                by: IrPath::from_segments(["row.id"]),
            },
            audit: None,
            emits: vec![],
            retry_quirks: vec![],
            span_ref: None,
        }
    }

    fn mk_field(name: &str, builtin: BuiltinType) -> Field {
        Field {
            name: name.into(),
            type_ref: TypeRef::Builtin(builtin),
            required: false,
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

    fn mk_feature(fields: Vec<(&str, BuiltinType)>, pollers: Vec<Poller>) -> Feature {
        Feature {
            name: "f".into(),
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
            resources: vec![Resource {
                name: "Src".into(),
                public_contract: None,
                tenancy: None,
                soft_delete: false,
                timestamps: None,
                fields: fields
                    .into_iter()
                    .map(|(name, builtin)| mk_field(name, builtin))
                    .collect(),
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
                restrict_on_delete: Vec::new(),
                append_only: false,
            }],
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
            pollers,
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
    fn quiet_for_multiple_pollers_with_datetime_fields() {
        let feat = mk_feature(
            vec![
                ("next_check_at", BuiltinType::DateTime),
                ("resolved_at", BuiltinType::DateTime),
                ("retry_at", BuiltinType::DateTime),
                ("done_at", BuiltinType::DateTime),
            ],
            vec![
                mk_poller("p1", "next_check_at", "resolved_at"),
                mk_poller("p2", "retry_at", "done_at"),
            ],
        );

        assert!(check(&feat, Path::new("f.lzi")).is_empty());
    }

    #[test]
    fn fires_for_integer_eligible_when_field() {
        let feat = mk_feature(
            vec![
                ("next_check_at", BuiltinType::Integer),
                ("resolved_at", BuiltinType::DateTime),
            ],
            vec![mk_poller("p", "next_check_at", "resolved_at")],
        );

        let findings = check(&feat, Path::new("f.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].field, "next_check_at");
        assert_eq!(findings[0].found, "Integer");
    }

    #[test]
    fn fires_for_text_eligible_when_field() {
        let feat = mk_feature(
            vec![
                ("next_check_at", BuiltinType::DateTime),
                ("resolved_at", BuiltinType::Text),
            ],
            vec![mk_poller("p", "next_check_at", "resolved_at")],
        );

        let findings = check(&feat, Path::new("f.lzi"));
        assert_eq!(findings.len(), 1);
        assert!(findings[0].message().contains("found Text"));
    }

    #[test]
    fn mixed_datetime_and_integer_reports_only_integer() {
        let feat = mk_feature(
            vec![
                ("next_check_at", BuiltinType::DateTime),
                ("resolved_at", BuiltinType::Integer),
            ],
            vec![mk_poller("p", "next_check_at", "resolved_at")],
        );

        let findings = check(&feat, Path::new("f.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].field, "resolved_at");
    }
}
