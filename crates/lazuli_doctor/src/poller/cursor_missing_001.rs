//! POLLER-CURSOR-MISSING-001 — `poller` source resource lacks one or
//! more cursor fields declared on the cursor block.
//!
//! Severity: error / error.
//! Reference: docs/proposals/poller-vocab.md §5.

use std::path::{Path, PathBuf};

use lazuli_ir::Feature;

/// One POLLER-CURSOR-MISSING-001 finding — the source resource is
/// missing one or more fields the poller's cursor block references.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file the poller was authored in.
    pub path: PathBuf,
    /// Feature name (mirrors the `.lzi` feature header).
    pub feature: String,
    /// Poller whose cursor block has dangling field refs.
    pub poller: String,
    /// Source resource that lacks the fields.
    pub source: String,
    /// Names of the missing fields (subset of
    /// `next_at_field` / `resolved_at_field` / `attempts_field`).
    pub missing: Vec<String>,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "POLLER-CURSOR-MISSING-001";

    /// Render the "cursor refs missing on source" message, naming the
    /// poller, missing fields, and the source resource. The text
    /// includes canonical scaffolds (`next_check_at: DateTime required`,
    /// ...) so authors can paste them.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::poller::cursor_missing_001::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("messages.lzi"),
    ///     feature: "messages".into(),
    ///     poller: "deliver_pending".into(),
    ///     source: "Message".into(),
    ///     missing: vec!["next_check_at".into()],
    /// };
    /// assert!(f.message().contains("next_check_at"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "poller `{}` cursor references field(s) `{}` not present on `{}` — declare them on the source resource (`next_check_at: DateTime required`, `resolved_at: DateTime`, `attempts: Integer = 0`)",
            self.poller,
            self.missing.join(", "),
            self.source,
        )
    }
}

/// Walk every poller in `feature` and emit a finding for each whose
/// cursor block references fields the source resource doesn't declare.
/// Cross-feature / unknown sources are skipped (handled by sibling
/// `POLLER-SOURCE-CROSS-FEATURE-001`).
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::poller::cursor_missing_001::check;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower a feature with a poller missing next_check_at on source");
/// let _ = check(&feature, Path::new("messages.lzi"));
/// ```
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
        Path as IrPath, Policies, Poller, PollerBackoff, PollerCursor, PollerRetry, PollerState,
        PollerStateKind, PollerTick, Resource, TypeRef,
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
                lifecycle_routes: None,
                polymorphic_refs: Vec::new(),
                many_through: Vec::new(),
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
            synth_origins: std::collections::BTreeMap::new(),
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
