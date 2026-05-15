//! POLLER-HANDLER-ORPHAN-001 — `poller resolve via @fn.<name>` references
//! a handler not declared under feature `extensions`.
//!
//! Severity: error / error.
//! Reference: docs/proposals/poller-vocab.md §5.

use std::path::{Path, PathBuf};

use lazuli_ir::{ExtensionContract, Feature};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub path: PathBuf,
    pub feature: String,
    pub poller: String,
    pub handler: String,
}

impl Finding {
    pub const CODE: &'static str = "POLLER-HANDLER-ORPHAN-001";

    pub fn message(&self) -> String {
        format!(
            "poller `{}` references handler `@fn.{}` but no `fn {}: Function[..., ...]` extension is declared in feature `{}`",
            self.poller, self.handler, self.handler, self.feature,
        )
    }
}

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
            requirements: vec![],
            enums: vec![],
            resources: vec![],
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
            pollers: vec![p],
            auth: None,
            surfaces: vec![],
            extensions,
            escape_routes: vec![],
            agents: vec![],
            reports: vec![],
            channels: vec![],
            previous_names: vec![],
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
