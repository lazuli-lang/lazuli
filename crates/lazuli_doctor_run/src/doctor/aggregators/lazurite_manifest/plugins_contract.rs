//! `PLUGIN-CONTRACT-001` — the adapter wiring-contract gate (spec 0022).
//!
//! Fires when an adapter plugin declared in `Lazurite.toml [plugins]`
//! declares (via 0021's typed `implements` / `[binds].interface`) a Go
//! bucket interface that is not in the known-bucket catalog, or whose
//! capability the app binds to a different plugin.
//!
//! This is the thin aggregator BRIDGE: it walks the declared plugins,
//! loads each typed `manifest.toml`, and delegates the classification to
//! the SHARED `lazuli_manifest::plugin_contract::classify_adapter_contract`
//! — the same function `lazuli plugin verify`'s L3 link calls — then maps
//! the resulting `ContractStatus` into a `DoctorDiagnostic` tagged with
//! `lazuli_doctor::correctness::plugin_contract_001::Finding::CODE`. The
//! two surfaces share ONE classifier, so they cannot diverge (pinned by
//! the `verify_and_doctor_agree_drift_guard` test in `lazuli_cli`).
//!
//! Anchoring: the unknown-interface case anchors at the plugin's
//! `manifest.toml` (where the bad `implements` lives); the
//! unbound-capability case anchors at `Lazurite.toml` (where bindings are
//! expressed).
//!
//! Registry view: v1 reads no statically-expressed capability bindings, so
//! the view is empty and only the unknown-interface arm ever fires on real
//! pilots — consistent with `PLUGIN-UNUSED-001` being a warning, never a
//! false FAIL on a pre-binding adapter.

use lazuli_doctor::correctness::plugin_contract_001::{Finding, Reason};
use lazuli_manifest::plugin_contract::{ContractStatus, RegistryView, classify_adapter_contract};

use crate::doctor::{DoctorDiagnostic, DoctorPackage, DoctorSeverity};

/// Build the `PLUGIN-CONTRACT-001` diagnostics for every declared plugin
/// whose typed manifest declares a broken adapter contract.
pub(super) fn check_plugin_contract(
    manifest: &lazuli_manifest::lazurite_manifest::Manifest,
    package: &DoctorPackage,
) -> Vec<DoctorDiagnostic> {
    let project_root = super::plugin_resolution_view::authoritative_project_root(package);
    // v1: the framework expresses no statically-readable capability
    // bindings, so the registry view is empty (the unbound-capability arm
    // never false-FAILs a pre-binding adapter).
    let registry = RegistryView::empty();

    let mut diagnostics = Vec::new();
    for plugin_ref in manifest.plugins.keys() {
        let Some(plugin_root) = lazuli_manifest::plugin_manifest::resolve_plugin_root(
            manifest,
            &project_root,
            plugin_ref,
        ) else {
            continue;
        };
        let typed = match lazuli_manifest::plugin_manifest::load_plugin_manifest(&plugin_root) {
            Ok(Some(m)) => m,
            // Missing / unreadable manifest is owned by
            // PLUGIN-MANIFEST-MISSING; don't double-flag here.
            Ok(None) | Err(_) => continue,
        };

        let status = classify_adapter_contract(&typed, plugin_ref, &registry);
        let (reason, anchor) = match status {
            ContractStatus::NotAnAdapter | ContractStatus::Ok => continue,
            ContractStatus::UnknownInterface { declared, nearest } => (
                Reason::UnknownInterface {
                    declared,
                    nearest: nearest.to_string(),
                },
                plugin_root.join(lazuli_manifest::plugin_manifest::PLUGIN_MANIFEST_FILENAME),
            ),
            ContractStatus::UnboundCapability {
                capability,
                bound_to,
            } => (
                Reason::UnboundCapability {
                    capability,
                    bound_to,
                },
                project_root.join("Lazurite.toml"),
            ),
        };

        let finding = Finding {
            plugin_ref: plugin_ref.clone(),
            anchor: anchor.clone(),
            reason,
        };
        diagnostics.push(DoctorDiagnostic {
            path: anchor,
            line: 1,
            column: 1,
            severity: DoctorSeverity::Error,
            code: Finding::CODE.to_owned(),
            message: finding.message(),
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    }
    diagnostics
}
