//! POLLER-TERMINAL-FIELD-ENUM-001 — `terminal_status_field` must point
//! at an enum-typed source resource field.
//!
//! Severity: error / error.
//! Reference: docs/proposals/poller-vocab.md §5.

use std::path::{Path, PathBuf};

use lazuli_ir::{BuiltinType, Feature, QualifiedName, TypeRef};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub path: PathBuf,
    pub feature: String,
    pub poller: String,
    pub field: String,
    pub found_type: String,
}

impl Finding {
    pub const CODE: &'static str = "POLLER-TERMINAL-FIELD-ENUM-001";

    pub fn message(&self) -> String {
        format!(
            "POLLER-TERMINAL-FIELD-ENUM-001: poller '{}' terminal_status_field '{}' must be an enum field; found {}.",
            self.poller, self.field, self.found_type,
        )
    }
}

pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    let mut findings = Vec::new();

    for poller in &feature.pollers {
        let Some(terminal_field) = &poller.terminal_status_field else {
            continue;
        };
        let Some(resource) = feature.resources.iter().find(|r| r.name == poller.source) else {
            // Cross-feature / unknown source is handled by a separate
            // rule (`POLLER-SOURCE-CROSS-FEATURE-001`); skip here.
            continue;
        };
        let Some(field) = resource.fields.iter().find(|f| f.name == *terminal_field) else {
            // Missing terminal fields are handled by
            // `POLLER-TERMINAL-FIELD-MISSING-001`.
            continue;
        };

        if !matches!(field.type_ref, TypeRef::EnumRef(_)) {
            findings.push(Finding {
                path: path.to_path_buf(),
                feature: feature.name.clone(),
                poller: poller.name.clone(),
                field: terminal_field.clone(),
                found_type: type_ref_label(&field.type_ref),
            });
        }
    }

    findings
}

fn type_ref_label(type_ref: &TypeRef) -> String {
    match type_ref {
        TypeRef::Builtin(builtin) => builtin_label(*builtin).to_string(),
        TypeRef::UserDefined(qn) | TypeRef::EnumRef(qn) => qname_label(qn),
        TypeRef::Many(inner) => format!("{}*", type_ref_label(inner)),
        TypeRef::Unresolved(name) => format!("unresolved {name}"),
        TypeRef::Capability(cap) => format!("{cap:?}"),
    }
}

fn qname_label(qn: &QualifiedName) -> String {
    match &qn.feature {
        Some(feature) => format!("{feature}.{}", qn.name),
        None => qn.name.clone(),
    }
}

fn builtin_label(builtin: BuiltinType) -> &'static str {
    match builtin {
        BuiltinType::Id => "Id",
        BuiltinType::Text => "Text",
        BuiltinType::Boolean => "Boolean",
        BuiltinType::Integer => "Integer",
        BuiltinType::Decimal => "Decimal",
        BuiltinType::Date => "Date",
        BuiltinType::DateTime => "DateTime",
        BuiltinType::Json => "Json",
        BuiltinType::SemanticEmail => "@semantic.Email",
        BuiltinType::SemanticMoney => "@semantic.Money",
        BuiltinType::SemanticPhone => "@semantic.Phone",
        BuiltinType::SemanticUrl => "@semantic.Url",
        BuiltinType::SemanticUuid => "@semantic.Uuid",
        BuiltinType::SemanticCurrency => "@semantic.Currency",
        BuiltinType::SemanticGeoPoint => "@semantic.GeoPoint",
        BuiltinType::CapSecret => "@cap.Secret",
        BuiltinType::CapFile => "@cap.File",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        Defaults, EnumDecl, Field, FieldConstraints, HandlerRef, IdempotencyKey, Path as IrPath,
        Policies, Poller, PollerBackoff, PollerCursor, PollerRetry, PollerState, PollerStateKind,
        PollerTick, Resource,
    };

    fn qn(name: &str) -> QualifiedName {
        QualifiedName {
            feature: None,
            name: name.into(),
        }
    }

    fn mk_poller() -> Poller {
        Poller {
            name: "v8_consult_resolver".into(),
            source: "V8PendingConsult".into(),
            cursor: PollerCursor {
                next_at_field: "next_check_at".into(),
                resolved_at_field: "resolved_at".into(),
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
                name: "poll_v8".into(),
                span_ref: None,
            },
            terminal_status_field: Some("final_status".into()),
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

    fn mk_field(name: &str, type_ref: TypeRef) -> Field {
        Field {
            name: name.into(),
            type_ref,
            required: false,
            unique: false,
            slug: false,
            default: None,
            derived_from: None,
            constraints: FieldConstraints::default(),
            full_text: false,
            previous_names: vec![],
            span_ref: None,
        }
    }

    fn mk_feature(terminal_type: TypeRef) -> Feature {
        Feature {
            name: "consults".into(),
            purpose: None,
            non_goals: vec![],
            context_path: None,
            defaults: Defaults::default(),
            uses: vec![],
            uses_spans: Vec::new(),
            requirements: vec![],
            enums: vec![EnumDecl {
                name: "ConsultFinalStatus".into(),
                public_contract: None,
                variants: vec![],
                previous_names: vec![],
                span_ref: None,
            }],
            resources: vec![Resource {
                name: "V8PendingConsult".into(),
                public_contract: None,
                tenancy: None,
                soft_delete: false,
                timestamps: None,
                fields: vec![mk_field("final_status", terminal_type)],
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
            }],
            events: vec![],
            rules: vec![],
            policies: Policies::default(),
            commands: vec![],
            apis: vec![],
            records: vec![],
            queries: vec![],
            workflows: vec![],
            jobs: vec![],
            webhooks: vec![],
            notifications: vec![],
            event_groups: vec![],
            tenant_migrations: vec![],
            translation: None,
            pollers: vec![mk_poller()],
            auth: None,
            surfaces: vec![],
            extensions: vec![],
            escape_routes: vec![],
            agents: vec![],
            reports: vec![],
            channels: vec![],
            caches: vec![],
            aggregates: vec![],
            previous_names: vec![],
            span_ref: None,
        }
    }

    #[test]
    fn quiet_when_terminal_status_field_is_enum() {
        let feat = mk_feature(TypeRef::EnumRef(qn("ConsultFinalStatus")));
        assert!(check(&feat, Path::new("consults.lzi")).is_empty());
    }

    #[test]
    fn fires_when_terminal_status_field_is_text() {
        let feat = mk_feature(TypeRef::Builtin(BuiltinType::Text));
        let findings = check(&feat, Path::new("consults.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].found_type, "Text");
        assert!(findings[0].message().contains("found Text"));
    }

    #[test]
    fn fires_when_terminal_status_field_is_integer() {
        let feat = mk_feature(TypeRef::Builtin(BuiltinType::Integer));
        let findings = check(&feat, Path::new("consults.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].found_type, "Integer");
    }
}
