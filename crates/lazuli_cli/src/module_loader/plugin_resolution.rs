//! 0019 — the SINGLE plugin-semantic resolution stage.
//!
//! Every module-load path (`build_module_from_path`,
//! `build_module_with_source_from_path`, and — after 0020 — the doctor)
//! funnels through [`resolve_module_plugins`]. The resolver, alias
//! builder, and Go emission arms all already work; the historical bug
//! was that the inline resolver block lived in ONLY one of the two
//! near-duplicate loaders, so `lazuli generate go` (the source-map
//! path) never resolved plugin `@semantic.*` refs and tripped the
//! closed Go semantic table downstream.
//!
//! This module makes drift structurally impossible (one place owns
//! resolution) and makes every failure mode LOUD instead of swallowed:
//!
//! - project-root detection walks UP to the nearest `Lazurite.toml`
//!   ([`find_project_root`]), so running from an `app/` subdir resolves
//!   plugins declared in the repo-root manifest;
//! - `lazurite_manifest::load` / `build_alias_map` errors propagate as
//!   anchored `anyhow` errors (namespace mismatch, unsupported carrier,
//!   conflict, manifest read/parse) instead of `if let Ok` no-ops;
//! - a residual scan, gated on a non-empty `[plugins]` block, converts
//!   any leftover `UserDefined("@semantic.*")` ref into an anchored
//!   hard error naming the alias + the declared plugins.
//!
//! The single legitimate silent case — single-file `lazuli check
//! <one.lzi>` with no project root — stays silent: [`find_project_root`]
//! returns `None` and the stage is a no-op, leaving `@semantic.*`
//! unresolved for the doctor's `SEMANTIC-PLUGIN-001` field anchor.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use lazuli_ir::{CommandInput, Module, TypeRef};

use crate::lazurite_manifest;
use crate::plugin_manifest;
use crate::plugin_semantic_resolver;

/// The single plugin-semantic resolution stage. Both module loaders
/// call this at the end (just before returning their module), and 0020
/// will route the doctor through it too.
///
/// Contract:
/// 1. No project root (single-file `lazuli check`) → silent `Ok(())`.
/// 2. `Lazurite.toml` read error → loud error.
/// 3. No manifest → `Ok(())` (nothing to resolve).
/// 4. `build_alias_map` error (namespace mismatch / unsupported carrier
///    / conflict / manifest read) → propagated LOUDLY (was swallowed).
/// 5. Apply resolution, then — if a non-empty `[plugins]` block exists —
///    residual-scan for unresolved `@semantic.*` refs and `bail!` with
///    an anchored message per distinct alias.
pub fn resolve_module_plugins(module: &mut Module, input: &Path) -> Result<()> {
    let Some(root) = find_project_root(input) else {
        // No project root: single-file check / ad-hoc input. Leave
        // `@semantic.*` unresolved for the doctor to anchor.
        return Ok(());
    };

    let manifest = lazurite_manifest::load(&root).with_context(|| {
        format!(
            "failed to read {}",
            root.join("Lazurite.toml").display()
        )
    })?;

    let Some(manifest) = manifest else {
        // A project root with no manifest contributes no plugins.
        return Ok(());
    };

    // Propagate alias-build errors LOUDLY (namespace mismatch,
    // unsupported carrier, conflict, manifest read/parse). Historically
    // these were dropped by an `if let Ok` and surfaced — if at all —
    // 200 lines downstream as an opaque codegen error.
    let alias_map = plugin_manifest::build_alias_map(Some(&manifest), &root)
        .map_err(|err| anyhow::anyhow!("{err}"))
        .with_context(|| {
            format!(
                "failed to wire plugins declared in {}",
                root.join("Lazurite.toml").display()
            )
        })?;

    plugin_semantic_resolver::apply_plugin_semantic_resolution(module, &alias_map);

    // Loud-failure precondition: only fires when a non-empty `[plugins]`
    // block is declared. Single-file / no-`[plugins]` flows stay silent
    // (a residual `@semantic.*` there is the doctor's job, not ours).
    if manifest.plugins.is_empty() {
        return Ok(());
    }

    let residual = collect_unresolved_semantic_aliases(module);
    if !residual.is_empty() {
        let mut declared: Vec<String> = manifest.plugins.keys().cloned().collect();
        declared.sort();
        let declared_list = declared.join(", ");
        let mut lines: Vec<String> = Vec::new();
        for alias in &residual {
            lines.push(format!(
                "plugin semantic '{alias}' is referenced but no declared plugin provides it — \
declare the contributing plugin in Lazurite.toml [plugins], or check its manifest.toml \
[[semantic_types]] (declared plugins: {declared_list})"
            ));
        }
        anyhow::bail!("{}", lines.join("\n"));
    }

    Ok(())
}

/// Walk UP from `input` to the nearest directory containing a
/// `Lazurite.toml` (or the legacy lowercase `lazurite.toml`). Returns
/// the first such directory, or `None` at the filesystem root.
///
/// `lazuli generate go app` (features under `app/`, manifest at repo
/// root) finds the repo-root manifest. Bounded — terminates at the
/// filesystem root, never loops.
pub fn find_project_root(input: &Path) -> Option<PathBuf> {
    let start: PathBuf = if input.is_dir() {
        input.to_path_buf()
    } else {
        input
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."))
    };

    let mut dir: &Path = &start;
    loop {
        if dir.join("Lazurite.toml").is_file() || dir.join("lazurite.toml").is_file() {
            return Some(dir.to_path_buf());
        }
        match dir.parent() {
            Some(parent) if parent != dir => dir = parent,
            _ => return None,
        }
    }
}

/// Residual scan: collect every distinct `@semantic.*` alias still
/// carried as `TypeRef::UserDefined` after resolution. Mirrors the
/// TypeRef sites walked by `apply_plugin_semantic_resolution` (resource
/// fields, record fields, event payloads, command typed inputs, and
/// `Many` inner wrappers).
fn collect_unresolved_semantic_aliases(module: &Module) -> Vec<String> {
    let mut out: BTreeSet<String> = BTreeSet::new();
    for feature in &module.features {
        for resource in &feature.resources {
            for field in &resource.fields {
                collect_type_ref(&field.type_ref, &mut out);
            }
        }
        for record in &feature.records {
            for field in &record.fields {
                collect_type_ref(&field.type_ref, &mut out);
            }
        }
        for event in &feature.events {
            for field in &event.payload {
                collect_type_ref(&field.type_ref, &mut out);
            }
        }
        for command in &feature.commands {
            if let CommandInput::Typed(slots) = &command.input {
                for slot in slots {
                    collect_type_ref(&slot.type_ref, &mut out);
                }
            }
        }
    }
    out.into_iter().collect()
}

fn collect_type_ref(type_ref: &TypeRef, out: &mut BTreeSet<String>) {
    match type_ref {
        TypeRef::UserDefined(qname) => {
            if qname.name.starts_with("@semantic.") {
                out.insert(qname.name.clone());
            }
        }
        TypeRef::Many(inner) => collect_type_ref(inner, out),
        _ => {}
    }
}
