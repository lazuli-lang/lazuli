//! `deploy` block line validator for `app`.
//!
//! `deploy` carries the contract-level deploy hygiene knobs that
//! every Lazuli runtime must respect: migration ordering, rollback
//! policy, destructive-migration gating, deploy strategy, and the
//! optional hooks / checkpoints from the Migrations bucket cycle.
//! The validator below is the file-local shape check; the catalog
//! values themselves are doctor-enforced (`DEPLOY-STRATEGY-001`).
//!
//! `validate_app_deploy_line` also mutates the running
//! `AppOperationalFacts` so the block-level diagnostics in
//! `app/mod.rs` can flag missing `migrations` or `rollback`.

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use super::AppOperationalFacts;
use crate::simple_canonical_diagnostic;

pub(crate) fn validate_app_deploy_line(
    diagnostics: &mut Vec<Diagnostic>,
    app: &mut AppOperationalFacts,
    line_index: usize,
    line: &str,
    trimmed: &str,
) {
    let parts: Vec<_> = trimmed.split_whitespace().collect();
    match parts.as_slice() {
        ["migrations", value] if matches!(*value, "before_deploy" | "manual" | "disabled") => {
            app.deploy_has_migrations = true;
        }
        ["migration_lock", value] if matches!(*value, "required" | "optional") => {}
        ["destructive_migrations", value]
            if matches!(*value, "require_approval" | "forbidden" | "manual") => {}
        ["rollback", value]
            if matches!(*value, "on_failed_healthcheck" | "manual" | "disabled") =>
        {
            app.deploy_has_rollback = true;
        }
        // Migrations bucket cycle Route C — five new deploy children.
        // `strategy` catalog enforced downstream by `DEPLOY-STRATEGY-001`.
        ["strategy", value]
            if matches!(*value, "rolling" | "blue_green" | "canary") => {}
        ["lock_timeout", _value] => {}
        ["pre_migration_hook", _value] => {}
        ["post_migration_hook", _value] => {}
        ["checkpoint", _name, _path] => {}
        _ => diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::WARNING,
            "app-deploy-contract",
            "deploy contracts use `migrations before_deploy|manual|disabled`, `migration_lock required|optional`, `destructive_migrations require_approval|forbidden`, `rollback on_failed_healthcheck|manual|disabled`, `strategy rolling|blue_green|canary`, `lock_timeout \"<duration>\"`, `pre_migration_hook \"<path>\"`, `post_migration_hook \"<path>\"`, and `checkpoint <name> \"<path>\"`.",
        )),
    }
}
