//! `lazuli inspect` — typed IR projector and canonical-source expander.
//!
//! Carved out of `main.rs` as part of Wave R3-D (Rails-style refactor).
//! This module owns every artifact `lazuli inspect` produces:
//!
//! - **Entry point** (`inspect_command`): dispatches on `--format`
//!   (Json vs Lazuli), `--expand=<axes>` (typed projection axes), and
//!   `--include=<lists>` (sidecar resources like the lifted manifest).
//! - **Symbol mode** (`inspect_symbol_*`): the per-symbol lookup
//!   surface (`lazuli inspect symbol:<name>`) used by the LSP for
//!   go-to-definition. Resolves cross-file, cross-feature, and
//!   plugin-aliased references.
//! - **Canonical source + expansions** (`inspect_canonical_source`,
//!   `expand_canonical_source`): the text-level expansion engine that
//!   surfaces inferred targets, expand-set projections, and event
//!   group decomposition.
//! - **Per-feature projections**: every `inspect_<axis>` (storage,
//!   auth, agents, jobs, webhooks, security, locators, defaults,
//!   tests, requirements, external_calls, notifications, ...). Each
//!   projection is a pure function `&[String] -> Vec<Inspect<Axis>>`
//!   that walks the trimmed source lines for a feature block.
//! - **IR projectors** (`project_job`, `project_webhook`,
//!   `project_event_group`, `project_aggregate`, `project_invariant`,
//!   `project_file_capability`, `project_auth`,
//!   `project_defaults_from_ir`): convert lifted `lazuli_ir::*`
//!   shapes into the JSON-stable `Inspect<X>` shapes serialized by
//!   `inspect_json_value`.
//! - **Helpers**: predicate / target-expr / let-binding / command
//!   effect renderers; type-ref + policy-ref + path printers; the
//!   `leading_spaces` / `is_identifier` / `is_type_name` /
//!   `namespace_references` line-walker primitives that the
//!   projectors share.
//!
//! ABI: `lazuli inspect --help` and `lazuli inspect ...` are
//! byte-identical with the pre-split build. The CLI flag enums
//! (`InspectFormat`, `InspectInclude`) stay in `main.rs` because
//! they're `#[derive(ValueEnum)]` on `Commands::Inspect`; the rest of
//! the cluster — including the `ExpandSet` aggregator, the
//! `Inspect<X>` serializable shapes, and all per-axis projectors —
//! lives here.
//!
//! Cross-refs:
//! - `cmd_mcp` reaches for `crate::ExpandSet`,
//!   `crate::parse_expand_set`, and `crate::inspect_json_value`;
//!   `main.rs` re-exports them so the MCP server keeps compiling
//!   without touching its import block.
//! - `tests.rs` reaches for `inspect_canonical_source`,
//!   `inspect_json_value`, `expand_canonical_source`,
//!   `parse_expand_set`, `render_inspect_symbol_lazuli`, all
//!   re-exported through main.rs.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Serialize;

use crate::casing::pascal_case;
use crate::{
    InspectFormat, InspectInclude, app_manifest, build_module_from_path,
    build_module_with_source_from_path, lazurite_manifest, plugin_manifest, plugin_semantic_resolver,
    project_root_for_input, read_package_lzi_source,
};

mod expand_set;

pub(crate) use expand_set::{ExpandSet, parse_expand_set};

pub(crate) fn inspect_command(
    input: &Path,
    expand: &str,
    format: InspectFormat,
    include: &[InspectInclude],
) -> Result<()> {
    // Symbol-mode dispatch per docs/proposals/lsp-symbol-origin.md §5.3.
    // When `input` is a bare or dotted symbol name (not a path), look it up
    // in the SymbolOriginIndex and emit the JSON shape from §5.2 instead of
    // the path-mode inspect output.
    if let Some(symbol) = inspect_symbol_arg(input) {
        return inspect_symbol_command(symbol, format);
    }

    let expansions = parse_expand_set(expand)?;
    let source_path = inspect_source_path(input);
    let source = if input.is_dir() && expansions.any() {
        read_package_lzi_source(input)?
    } else {
        fs::read_to_string(&source_path)
            .with_context(|| format!("failed to read {}", source_path.display()))?
    };
    let report_input = if input.is_dir() && expansions.any() {
        input
    } else {
        source_path.as_path()
    };

    match format {
        InspectFormat::Json => {
            // B3 — `input` carries the directory the author passed
            // (often `.`), while `source_path` has already resolved to
            // `app/app.lzi`. The manifest lives at the *original*
            // directory; pass both so the plugin alias-map lookup
            // anchors at the right Lazurite.toml.
            let output = inspect_json_value(&source, report_input, input, expansions, include)?;
            println!("{}", serde_json::to_string_pretty(&output)?);
        }
        InspectFormat::Lazuli => {
            if expansions.any() {
                print!("{}", expand_canonical_source_with(&source, expansions));
            } else {
                // Default human projection: the C4 / M3 features-summary
                // renderer, which annotates each opted-in resource with
                // `(conventions: <bundle>)` and tags synth-derived
                // commands/queries per `ir-resource-conventions-crud.md`
                // §11 + `ir-resource-conventions-me.md` §8. Falls back to
                // the verbatim source echo when the canonical-indent
                // slice can't be parsed/lowered — `inspect` is a
                // read-only projection, not a check, so a parse failure
                // here must not flip the command into an error path.
                print!("{}", render_lazuli_features_summary(&source));
            }
        }
    }

    Ok(())
}

/// Default `--format=lazuli` human projection: parse the canonical-indent
/// slice, lower each `FeatureSkeleton` into IR (which runs the convention
/// synth pass), and render the §11 / §8 features-summary digest with
/// `(conventions: <bundle>)` resource annotations and `[conv:<bundle>]`
/// synth-origin tags.
///
/// Falls back to the verbatim source on any parse/lower failure — inspect
/// is a read-only projection per `docs/canonical-semantics.md`, so a
/// downstream parser bug must not block the human view. The fallback
/// preserves pre-features-summary behavior for any document the
/// canonical-indent slice doesn't yet understand.
fn render_lazuli_features_summary(source: &str) -> String {
    let Ok(skeletons) = lazuli_syntax::parse_feature_skeletons(source) else {
        return source.to_owned();
    };
    if skeletons.is_empty() {
        return source.to_owned();
    }
    let mut features = Vec::with_capacity(skeletons.len());
    for skeleton in &skeletons {
        match lazuli_analyzer::lower_feature_skeleton(skeleton) {
            Ok(feature) => features.push(feature),
            Err(_) => return source.to_owned(),
        }
    }
    crate::inspect::features_summary::render_features_summary(&features)
}

/// Detect symbol-mode arguments per `docs/proposals/lsp-symbol-origin.md` §5.3.
///
/// Returns `Some(arg)` when the input is a bare or dotted symbol name (e.g.
/// `Gender`, `host.Gender`), and `None` when path-mode rules apply:
/// - contains a path separator (`/` or `\`)
/// - ends in `.lzi`
/// - is `.` or `..`
/// - points to an existing file or directory
///
/// The disambiguation is lexical first (separator/extension/sentinel) and
/// filesystem-aware second (existing path → path mode). Authors who want
/// the feature-named symbol when a directory shares the name can qualify
/// via `<feature>.<Type>`.
mod symbol;

use symbol::{inspect_symbol_arg, inspect_symbol_command};
pub(crate) use symbol::render_inspect_symbol_lazuli;


pub(crate) fn inspect_source_path(input: &Path) -> PathBuf {
    if input.is_dir() {
        return lazurite_manifest::resolve_in_app_dir(input, "app.lzi");
    }

    input.to_path_buf()
}

pub(crate) fn inspect_json_value(
    source: &str,
    input: &Path,
    project_root_hint: &Path,
    expansions: ExpandSet,
    include: &[InspectInclude],
) -> Result<serde_json::Value> {
    // Prefer the caller-supplied project root (the directory the
    // author passed on the command line). When the hint isn't a
    // directory (typical for single-file `lazuli inspect host.lzi`
    // invocations), walk upward from the input's parent to find a
    // directory that contains `Lazurite.toml` — without this, the
    // single-file path never sees the manifest and B3's plugin
    // alias map stays empty.
    let project_root = if project_root_hint.is_dir() {
        project_root_hint.to_path_buf()
    } else {
        let mut candidate: PathBuf = project_root_for_input(input);
        // Bounded walk-up so we don't escape the workspace; 8 levels
        // is generous for `app/features/<name>/<file>.lzi` layouts.
        for _ in 0..8 {
            if candidate.join("Lazurite.toml").is_file()
                || candidate.join("lazurite.toml").is_file()
            {
                break;
            }
            let Some(parent) = candidate.parent().map(Path::to_path_buf) else {
                break;
            };
            if parent == candidate {
                break;
            }
            candidate = parent;
        }
        candidate
    };
    // B3 — build the plugin alias map up front so the inspect report
    // and the optional `plugin_semantic_types` manifest projection
    // share a single source of truth. The map is read once per
    // inspect invocation per
    // `docs/proposals/semantic-types-plugin-locales.md` §IR and resolution.
    let alias_map = lazurite_manifest::load(&project_root)
        .ok()
        .flatten()
        .and_then(|manifest| {
            plugin_manifest::build_alias_map(Some(&manifest), &project_root).ok()
        })
        .unwrap_or_default();
    let report =
        inspect_canonical_source_with_aliases(source, input, expansions, &alias_map);
    let manifest = lazurite_manifest::load(&project_root).with_context(|| {
        format!(
            "failed to read {}",
            project_root.join("Lazurite.toml").display()
        )
    })?;

    if let Some(manifest) = manifest {
        // B3 — surface the plugin-contributed `@semantic.<Name>` alias
        // map alongside the existing manifest projection so agents
        // reading `lazuli inspect --include=manifest --format=json`
        // discover which aliases are active and where each resolves.
        // The per-alias entry carries the proposal-mandated keys:
        // `kind`, `plugin`, `name`, `alias`, `carrier`, `origin`.
        let plugin_semantic_types =
            inspect_plugin_semantic_types(&manifest, &project_root);
        return Ok(serde_json::json!({
            "ir": report,
            "manifest": manifest.inspect_view(),
            "plugin_semantic_types": plugin_semantic_types,
        }));
    }

    if include.contains(&InspectInclude::Manifest) {
        return Ok(serde_json::json!({
            "ir": report,
            "manifest": serde_json::Value::Null,
            "plugin_semantic_types": serde_json::Value::Array(Vec::new()),
        }));
    }

    Ok(serde_json::to_value(report)?)
}

/// B3 — flatten the resolved plugin semantic alias map into the
/// proposal §IR-and-resolution shape: each entry exposes
/// `{ kind, plugin, name, alias, carrier, origin, validator,
/// formatter }`. Sorted by alias.
fn inspect_plugin_semantic_types(
    manifest: &lazurite_manifest::Manifest,
    project_root: &Path,
) -> serde_json::Value {
    let map = match plugin_manifest::build_alias_map(Some(manifest), project_root) {
        Ok(map) => map,
        Err(err) => {
            // Surface the failure so consumers see something rather
            // than silently emitting an empty array.
            return serde_json::json!({ "error": err.to_string() });
        }
    };
    let entries: Vec<serde_json::Value> = map
        .into_iter()
        .map(|(alias, resolved)| {
            serde_json::json!({
                "kind": "semantic_plugin",
                "plugin": resolved.plugin_namespace,
                "name": resolved.name,
                "alias": alias,
                "carrier": format!("{:?}", resolved.carrier),
                "validator": resolved.validator,
                "formatter": resolved.formatter,
                "origin": format!("plugin manifest:{}", resolved.plugin_namespace),
            })
        })
        .collect();
    serde_json::Value::Array(entries)
}

mod report_types;

// Re-export inside the inspect tree so sibling submodules (projectors, etc.)
// can `use super::InspectJob;` etc. The structs are `pub(super)` inside
// report_types — their effective visibility stays at inspect-internal scope.
pub(in crate::commands::inspect) use report_types::*;
pub(crate) use report_types::{InspectReport, InspectFeature};

mod canonical_source;

// Re-export the orchestrator's public entry point for cmd_mcp / tests
// / the inspect-feature-summary renderer; the with-aliases variant
// stays scoped to inspect-internal callers (`inspect_json_value`
// reaches in directly). `Tier3FeatureSlice` is re-exported so the
// per-axis projections under `projections/` can keep their
// `super::super::Tier3FeatureSlice` paths stable.
pub(crate) use canonical_source::inspect_canonical_source;
pub(in crate::commands::inspect) use canonical_source::{
    Tier3FeatureSlice, inspect_canonical_source_with_aliases,
};


// -----------------------------------------------------------------------------
// Inspect-internal sibling modules.
// -----------------------------------------------------------------------------
//
// These are all `pub(in crate::commands::inspect)` (`mod` is private by
// default; the items each module re-exports drive the scope). The
// inspect/mod.rs orchestrator only needs the `mod` declarations so the
// submodules participate in the binary. Direct `use` clauses live in
// the modules that need them (`canonical_source`, the `projections/`
// subtree, `expand`, `text_walkers`).

mod expand;
mod projections;
mod projectors;
mod security;
mod text_walkers;

pub(in crate::commands::inspect) mod formatters;

// Re-export the text-walker helpers at the inspect root so existing
// sibling modules (security.rs, expand.rs) that historically reached
// for `super::<helper>` keep compiling without churn. Every helper is
// `pub(super)` in text_walkers and stays at inspect-internal scope.
pub(in crate::commands::inspect) use text_walkers::{
    command_blocks, command_name, direct_child_value, direct_child_values,
    field_name_from_typed_line, named_top_block_name, parse_audit, query_blocks, query_name,
    security_markers, top_level_blocks,
};

use expand::expand_canonical_source_with;

#[cfg(test)]
#[allow(unused_imports)]
pub(crate) use expand::expand_canonical_source;

