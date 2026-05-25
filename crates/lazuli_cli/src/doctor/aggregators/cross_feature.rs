//! Cross-feature URL / audience / scope diagnostics.
//!
//! Three producers that all need workspace-wide context — either the
//! per-file `.lzx` documents (audience collection), the loaded
//! `app.lzi` manifest (URLs), or the Tier 3 feature facts joined
//! against the local policies catalog (scope-owner column check):
//!
//! - `collect_known_audiences` — fact extractor for downstream agent /
//!   route checks (not a diagnostic producer itself; lives here so the
//!   `.lzx` audience walker is colocated with the producers that
//!   consume it).
//! - `app_urls_missing_diagnostics` (`app_urls_missing`) — warn when
//!   `app.lzi` ships no `urls` block.
//! - `scope_owner_column_diagnostics` (`SCOPE-OWNER-COLUMN-001`) —
//!   warn when a command's policy includes `@scope.owner` or
//!   `@scope.same_org` but the targeted resource has no matching
//!   ownership / tenant column.

use std::collections::{BTreeMap, BTreeSet};

use crate::doctor::{
    DoctorAppManifest, DoctorDiagnostic, DoctorFile, DoctorSeverity, Tier3FeatureFacts,
};

/// Collect every audience that's a first-class declaration in the
/// workspace. Today, surfaces in `.lzx` files are the canonical source
/// (`surface customer web` ... `audience admin`). A future cut may
/// elevate audiences to top-level `app.lzi` declarations; until then,
/// `.lzi` text scans are intentionally avoided — counting `audience X`
/// references inside e.g. `expose http audience X` would defeat the
/// `agent_expose_audience_unknown_diagnostics` check by self-resolving.
pub(crate) fn collect_known_audiences(files: &[DoctorFile]) -> BTreeSet<String> {
    let mut audiences = BTreeSet::new();
    for file in files {
        if let Some(document) = file.lzx.as_ref() {
            for surface in &document.surfaces {
                for audience in &surface.audiences {
                    audiences.insert(audience.name.clone());
                }
            }
        }
    }
    audiences
}

/// `app_urls_missing` — warn when `app.lzi` declares no `urls` block.
/// Without that block, auth callbacks, CORS allowlist, and frontend
/// redirect targets cannot be configured.
pub(crate) fn app_urls_missing_diagnostics(
    app: Option<&DoctorAppManifest>,
) -> Vec<DoctorDiagnostic> {
    let Some(app_manifest) = app else {
        return Vec::new();
    };
    if !app_manifest.manifest.urls.is_empty() {
        return Vec::new();
    }

    vec![DoctorDiagnostic {
        path: app_manifest.path.clone(),
        line: 1,
        column: 1,
        severity: DoctorSeverity::Warning,
        code: "app_urls_missing".to_owned(),
        message: APP_URLS_MISSING_MESSAGE.to_owned(),
        category: None,
        feature_name: None,
        construct: None,
        fix: None,
        group: None,
    }]
}

const APP_URLS_MISSING_MESSAGE: &str = "app declares no `urls` block — auth callbacks, CORS allowlist, and frontend redirect targets cannot be configured. Add a `urls` block to app.lzi with at least one environment URL (e.g., `urls\n  dev: \"http://localhost:3000\"`).";

/// `SCOPE-OWNER-COLUMN-001` — warn when a command's policy includes
/// `@scope.owner` or `@scope.same_org` but the targeted resource has
/// no matching ownership / tenant column. Codegen silently skips the
/// WHERE binding in that case (per `crates/lazuli_codegen_go/src/emitter/command.rs`
/// `resolve_scope_bindings`); this diagnostic surfaces the silent-skip
/// at design time so authors don't ship a policy that's only enforced
/// at the role-check gate.
///
/// Closed-catalog column priorities mirror codegen exactly:
/// - `@scope.owner` searches `user_id` > `user` > `owner_id` > `owner`.
/// - `@scope.same_org` searches `org_id` > `org` > `tenant_id` > `tenant`.
pub(crate) fn scope_owner_column_diagnostics(
    tier3_facts: &[Tier3FeatureFacts],
) -> Vec<DoctorDiagnostic> {
    use lazuli_ir::{CommandEffect, PolicyRef};

    const OWNER_COLUMNS: &[&str] = &["user_id", "user", "owner_id", "owner"];
    const SAME_ORG_COLUMNS: &[&str] = &["org_id", "org", "tenant_id", "tenant"];

    let mut diagnostics = Vec::new();

    for feature in tier3_facts {
        let local_policies: BTreeMap<&str, &Vec<String>> = feature
            .policies
            .categories
            .iter()
            .map(|c| (c.name.as_str(), &c.atoms))
            .collect();

        for command in &feature.commands {
            // Only `Updates` and `Deletes` benefit from row-scoping;
            // `Creates`/`Returns`/`None` have no target row.
            let resource_qname = match &command.effect {
                CommandEffect::Updates(u) => &u.resource,
                CommandEffect::Deletes(d) => &d.resource,
                _ => continue,
            };

            // Only resolve when the resource lives in this feature.
            // Cross-feature scope lowering is a follow-up (would need
            // the module-level resource index).
            if let Some(feature_part) = &resource_qname.feature {
                if feature_part != &feature.feature {
                    continue;
                }
            }
            let Some(resource) = feature
                .resources
                .iter()
                .find(|r| r.name == resource_qname.name)
            else {
                continue;
            };

            // Resolve the command's policy atom list. Mirrors the
            // codegen-side `command_policy_atoms` helper plus the
            // `populate_commands_from_ir` atom-resolution logic so
            // the diagnostic fires on exactly the cases codegen would
            // silently skip — and on the canonical `@policy.<name>`
            // form (parsed as `PolicyRef::Atom("policy.<name>")` with
            // a local-policies lookup) as well as the rare bare
            // `PolicyRef::Local`.
            let atoms: Vec<String> = match &command.policy {
                PolicyRef::Local(name) => local_policies
                    .get(name.as_str())
                    .map(|atoms| (*atoms).clone())
                    .unwrap_or_default(),
                PolicyRef::Atom(atom) => {
                    if let Some(local) = atom.strip_prefix("policy.") {
                        local_policies
                            .get(local)
                            .map(|atoms| (*atoms).clone())
                            .unwrap_or_else(|| vec![format!("@{atom}")])
                    } else {
                        vec![format!("@{atom}")]
                    }
                }
                _ => continue,
            };

            for atom in &atoms {
                let (priority, axis_label) = match atom.as_str() {
                    "@scope.owner" => (OWNER_COLUMNS, "owner"),
                    "@scope.same_org" => (SAME_ORG_COLUMNS, "org"),
                    _ => continue,
                };
                let has_match = priority
                    .iter()
                    .any(|c| resource.fields.iter().any(|f| f.name == *c));
                if has_match {
                    continue;
                }
                let line = feature
                    .command_lines
                    .get(&command.name)
                    .copied()
                    .unwrap_or(feature.feature_line);
                diagnostics.push(DoctorDiagnostic {
                    path: feature.path.clone(),
                    line,
                    column: 1,
                    severity: DoctorSeverity::Warning,
                    code: "SCOPE-OWNER-COLUMN-001".to_owned(),
                    message: format!(
                        "command `{}.{}` policy includes `{atom}` but resource `{}` has none of `{}`; codegen will skip the auto-injected WHERE binding and the row-scope will only be enforced by the role check (not the DB).",
                        feature.feature,
                        command.name,
                        resource.name,
                        priority.join("`, `"),
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
                // Don't flag the same command twice for the same atom.
                let _ = axis_label;
                break;
            }
        }
    }

    diagnostics
}
