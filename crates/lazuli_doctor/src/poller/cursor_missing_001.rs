//! POLLER-CURSOR-MISSING-001 — `poller` source resource lacks one or
//! more cursor fields declared on the cursor block.
//!
//! Severity: error / error.
//! Reference: docs/proposals/poller-vocab.md §5.

use std::path::{Path, PathBuf};

use lazuli_ir::Feature;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub path: PathBuf,
    pub feature: String,
    pub poller: String,
    pub source: String,
    pub missing: Vec<String>,
}

impl Finding {
    pub const CODE: &'static str = "POLLER-CURSOR-MISSING-001";

    pub fn message(&self) -> String {
        format!(
            "poller `{}` cursor references field(s) `{}` not present on `{}` — declare them on the source resource (`next_check_at: DateTime required`, `resolved_at: DateTime`, `attempts: Integer = 0`)",
            self.poller,
            self.missing.join(", "),
            self.source,
        )
    }
}

pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    let mut findings = Vec::new();
    for poller in &feature.pollers {
        let Some(resource) = feature.resources.iter().find(|r| r.name == poller.source) else {
            // Cross-feature / unknown source is handled by a separate
            // rule (`POLLER-SOURCE-CROSS-FEATURE-001`); skip here.
            continue;
        };
        let mut missing = Vec::new();
        for needed in [
            &poller.cursor.next_at_field,
            &poller.cursor.resolved_at_field,
            &poller.cursor.attempts_field,
        ] {
            if !resource.fields.iter().any(|f| &f.name == needed) {
                missing.push(needed.clone());
            }
        }
        if !missing.is_empty() {
            findings.push(Finding {
                path: path.to_path_buf(),
                feature: feature.name.clone(),
                poller: poller.name.clone(),
                source: poller.source.clone(),
                missing,
            });
        }
    }
    findings
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        BuiltinType, Defaults, Feature, Field, FieldConstraints, HandlerRef, IdempotencyKey,
        Path as IrPath, Poller, PollerBackoff, PollerCursor, PollerRetry, PollerState,
        PollerStateKind, PollerTick, Policies, Resource, TypeRef,
    };

    fn mk_poller(src: &str, next_at: &str, resolved_at: &str, attempts: &str) -> Poller {
        Poller {
            name: "p".into(),
            source: src.into(),
            cursor: PollerCursor {
                next_at_field: next_at.into(),
                resolved_at_field: resolved_at.into(),
                attempts_field: attempts.into(),
                span_ref: None,
            },
            retry: PollerRetry {
                max_attempts: 30,
                backoff: PollerBackoff::Fixed { base: None },
                span_ref: None,
            },
            states: vec![PollerState {
                name: "pending".into(),
                kind: PollerStateKind::Initial,
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

    fn mk_field(name: &str) -> Field {
        Field {
            name: name.into(),
            type_ref: TypeRef::Builtin(BuiltinType::Text),
            required: false,
            unique: false,
            slug: false,
            default: None,
            derived_from: None,
            constraints: FieldConstraints::default(),
            full_text: false,
            previous_names: vec![],
            pii: None,
            span_ref: None,
        }
    }

    fn mk_feature(resource_fields: Vec<&str>, poller: Poller) -> Feature {
        Feature {
            name: "f".into(),
            purpose: None,
            non_goals: vec![],
            context_path: None,
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
                fields: resource_fields.into_iter().map(mk_field).collect(),
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
            pollers: vec![poller],
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
            span_ref: None,
        }
    }

    #[test]
    fn fires_when_cursor_field_missing() {
        let p = mk_poller("Src", "next_check_at", "resolved_at", "attempts");
        let feat = mk_feature(vec!["next_check_at"], p);
        let findings = check(&feat, Path::new("f.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].missing.len(), 2);
        assert!(findings[0].message().contains("resolved_at"));
    }

    #[test]
    fn quiet_when_all_present() {
        let p = mk_poller("Src", "next_check_at", "resolved_at", "attempts");
        let feat = mk_feature(vec!["next_check_at", "resolved_at", "attempts"], p);
        assert!(check(&feat, Path::new("f.lzi")).is_empty());
    }
}
