//! POLLER-HANDLER-ORPHAN-001 — `poller resolve via @fn.<name>` references
//! a handler not declared under feature `extensions`.
//!
//! Severity: error / error.
//! Reference: docs/proposals/poller-vocab.md §5.

use std::path::{Path, PathBuf};

use lazuli_ir::{ExtensionContract, Feature};

/// One POLLER-HANDLER-ORPHAN-001 finding — a poller's `resolve via
/// @fn.<name>` reference doesn't match any `fn` extension declared on
/// the feature.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file the poller was authored in.
    pub path: PathBuf,
    /// Feature name (mirrors the `.lzi` feature header).
    pub feature: String,
    /// Poller carrying the dangling handler reference.
    pub poller: String,
    /// Handler name the poller tried to resolve.
    pub handler: String,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "POLLER-HANDLER-ORPHAN-001";

    /// Render the "no matching `fn` extension" message naming the
    /// poller, handler, and the expected scaffold form.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::poller::handler_orphan_001::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("messages.lzi"),
    ///     feature: "messages".into(),
    ///     poller: "deliver_pending".into(),
    ///     handler: "send_message".into(),
    /// };
    /// assert!(f.message().contains("@fn.send_message"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "poller `{}` references handler `@fn.{}` but no `fn {}: Function[..., ...]` extension is declared in feature `{}`",
            self.poller, self.handler, self.handler, self.feature,
        )
    }
}

/// Walk every poller in `feature` and emit a finding for each whose
/// `resolve via @fn.<name>` reference is not paired with an `fn`
/// extension declaration on the same feature.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::poller::handler_orphan_001::check;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower a feature with a poller resolving a missing handler");
/// let _ = check(&feature, Path::new("messages.lzi"));
/// ```
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    let declared: std::collections::HashSet<&str> = feature
        .extensions
        .iter()
        .filter(|ext| matches!(ext.contract, ExtensionContract::Function { .. }))
        .map(|ext| ext.name.as_str())
        .collect();

    feature
        .pollers
        .iter()
        .filter(|p| p.resolve_handler.namespace == "fn")
        .filter(|p| !declared.contains(p.resolve_handler.name.as_str()))
        .map(|p| Finding {
            path: path.to_path_buf(),
            feature: feature.name.clone(),
            poller: p.name.clone(),
            handler: p.resolve_handler.name.clone(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        BuiltinType, Defaults, Extension, ExtensionContract, Feature, HandlerRef, IdempotencyKey,
        Path as IrPath, PathRef, Poller, PollerBackoff, PollerCursor, PollerRetry, PollerState,
        PollerStateKind, PollerTick, Policies, TypeRef,
    };

    fn mk_poller(handler_name: &str) -> Poller {
        Poller {
            name: "p".into(),
            source: "Src".into(),
            cursor: PollerCursor {
                next_at_field: "n".into(),
                resolved_at_field: "r".into(),
                attempts_field: "a".into(),
                span_ref: None,
            },
            retry: PollerRetry {
                max_attempts: 1,
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
                name: handler_name.into(),
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

    fn mk_feature(p: Poller, extensions: Vec<Extension>) -> Feature {
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
            resources: vec![],
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
            pollers: vec![p],
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

    fn mk_fn_extension(name: &str) -> Extension {
        Extension {
            name: name.into(),
            contract: ExtensionContract::Function {
                input: TypeRef::Builtin(BuiltinType::Json),
                output: TypeRef::Builtin(BuiltinType::Json),
            },
            resolved_path: PathRef::authored("./handlers/x.go"),
            previous_names: vec![],
            span_ref: None,
        }
    }

    #[test]
    fn fires_when_handler_missing() {
        let feat = mk_feature(mk_poller("poll_v8"), vec![]);
        let findings = check(&feat, Path::new("f.lzi"));
        assert_eq!(findings.len(), 1);
        assert!(findings[0].message().contains("poll_v8"));
    }

    #[test]
    fn quiet_when_handler_declared() {
        let feat = mk_feature(mk_poller("poll_v8"), vec![mk_fn_extension("poll_v8")]);
        assert!(check(&feat, Path::new("f.lzi")).is_empty());
    }
}
