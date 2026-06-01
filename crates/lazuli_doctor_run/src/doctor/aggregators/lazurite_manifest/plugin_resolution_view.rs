//! The doctor's single authoritative plugin-resolution view — the
//! doctor-time mirror of codegen's `resolve_module_plugins`.
//!
//! Fires when a `@semantic.<X>` reference would NOT resolve on the
//! generate path: this module owns the ONE step the three
//! plugin-semantic checks share — "resolve the authoritative alias map
//! for this package" — so they can never again disagree on the project
//! root. Before 0020, each check called `build_alias_map(Some(manifest),
//! &package.project_root)` with `package.project_root` coming from
//! `doctor_project_root` (a pass-through, NO upward walk). Codegen
//! meanwhile walked UP to the repo-root `Lazurite.toml`. The two surfaces
//! fed the SAME `build_alias_map` DIFFERENT roots, so `lazuli doctor app`
//! and `lazuli generate go app` could resolve the same project
//! differently — doctor-green ≠ codegen-green.
//!
//! This helper closes that seam by construction: it walks UP through the
//! SHARED `lazuli_manifest::lazurite_manifest::find_project_root` (the
//! exact function 0019's codegen resolver uses), loads the manifest from
//! THAT root, and builds the alias map from the same `(manifest, root)`
//! pair codegen feeds. Same function, same root, same manifest ⇒ same
//! map. The existing `SEMANTIC-PLUGIN-001` inline check then evaluates
//! `@semantic.<X>` against the authoritative map, so it fires exactly
//! when generate would leave a residual `UserDefined("@semantic.*")` and
//! stays silent when generate resolves.

use std::collections::BTreeMap;
use std::path::PathBuf;

use lazuli_manifest::lazurite_manifest::{self, Manifest};
use lazuli_manifest::plugin_manifest::{
    PluginManifestError, ResolvedPluginSemantic, build_alias_map,
};

use crate::doctor::DoctorPackage;

/// The AUTHORITATIVE project root for this doctor invocation — the same
/// upward `Lazurite.toml` walk codegen uses. Run from an `app/` features
/// subdir, this ascends to the repo-root manifest exactly as
/// `lazuli generate go app` does. Falls back to the doctor's input root
/// (`package.project_root`) when no ancestor manifest exists — preserving
/// the silent single-file / no-manifest case (empty map).
pub(super) fn authoritative_project_root(package: &DoctorPackage) -> PathBuf {
    lazurite_manifest::find_project_root(&package.project_root)
        .unwrap_or_else(|| package.project_root.clone())
}

/// Build the package's `@semantic.<Name> → ResolvedPluginSemantic` alias
/// map from the AUTHORITATIVE upward-walked root — byte-identical to the
/// map codegen builds for the same input. The three plugin-semantic
/// checks share this ONE entry, so they cannot diverge on root.
///
/// Steps (mirroring `resolve_module_plugins`):
/// 1. Walk UP to the manifest root (shared `find_project_root`).
/// 2. Load the manifest from THAT root (not `package.lazurite_manifest`,
///    which may have been loaded from a divergent pass-through root).
/// 3. `build_alias_map(manifest, &root)` — the SAME function codegen
///    calls. Map-construction errors propagate; the callers already turn
///    `Err` into a project-anchored `SEMANTIC-PLUGIN-001`.
///
/// No manifest at the authoritative root → empty map (the silent
/// single-file / no-`[plugins]` case), exactly as `build_alias_map(None,
/// _)` and codegen's no-manifest bail behave.
pub(crate) fn authoritative_alias_map(
    package: &DoctorPackage,
) -> Result<BTreeMap<String, ResolvedPluginSemantic>, PluginManifestError> {
    let root = authoritative_project_root(package);
    let manifest: Option<Manifest> =
        lazurite_manifest::load(&root).map_err(|err| PluginManifestError::Read {
            plugin: "Lazurite.toml".to_owned(),
            path: root.join("Lazurite.toml"),
            message: err.to_string(),
        })?;
    build_alias_map(manifest.as_ref(), &root)
}
