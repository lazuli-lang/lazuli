//! Lazuli source discovery and module loading.
//!
//! Carved out of `main.rs` as part of Wave R6-5 (Rails-style refactor)
//! and re-split in R9 into sibling files. This module owns the
//! file-system walkers and IR builders that turn a `.lzi` / `.lzx`
//! file or directory into the typed module shapes that every `lazuli`
//! subcommand consumes:
//!
//! - **Discovery walkers** + **source aggregation** ([`collectors`]):
//!   `collect_package_lzi_files`, `collect_package_lzx_files`,
//!   `collect_lzx_experience_module`, `read_package_lzi_source`.
//! - **`.lzx` bundling + surface attachment** ([`lzx_bundle`]):
//!   `LzxBundle`, `collect_lzx_bundle`, `attach_lzx_surfaces`.
//! - **Module builders** (this file): `build_module_from_path` and
//!   `build_module_with_source_from_path` lower the discovered files
//!   into `lazuli_ir::Module` (the source-map variant returns a
//!   `lazuli_ir::SourceMap` and per-feature `FileId` table for codegen
//!   `//line` directives).
//! - **Project root resolution + plan/gate aggregation** (this file):
//!   `project_root_for_input`, `collect_plan_gate_facts_for_generate`.
//!
//! No behavior change vs. the pre-split build.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use lazuli_analyzer::source_map::SourceMapResolver as _;

use crate::app_manifest;
use crate::lazurite_manifest;
use crate::plugin_manifest;
use crate::plugin_semantic_resolver;

mod collectors;
mod lzx_bundle;

pub(crate) use collectors::{
    collect_lzx_experience_module, collect_package_lzi_files, collect_package_lzx_files,
    read_package_lzi_source,
};
pub(crate) use lzx_bundle::{LzxBundle, collect_lzx_bundle};

use lzx_bundle::attach_lzx_surfaces;

/// Build a `lazuli_ir::Module` from a `.lzi` file or directory by
/// walking every `.lzi` file in the canonical fixture and lowering its
/// `feature` blocks through the canonical-indent slice (Phase L Tier
/// 4). Files without typed feature skeletons (e.g. `app.lzi`,
/// `registry.lzi`) feed `AppManifest` / `AppRegistry`.
pub(crate) fn build_module_from_path(input: &Path) -> Result<lazuli_ir::Module> {
    let mut module = lazuli_ir::Module {
        workspace: None,
        contracts: Vec::new(),
        app: None,
        registry: None,
        profiles: Vec::new(),
        design: None,
        rbac: None,
        features: Vec::new(),
    };

    // L0 #2 — `design.lzi` lives at project root, peer to `app.lzi` /
    // `registry.lzi`. Only parse when we're building from a directory;
    // single-file input mode skips the design pipeline.
    if input.is_dir() {
        let design_path = lazurite_manifest::resolve_in_app_dir(input, "design.lzi");
        if design_path.is_file() {
            let source = fs::read_to_string(&design_path)
                .with_context(|| format!("reading {}", design_path.display()))?;
            match lazuli_syntax::parse_design_document(&source) {
                Ok(ast) => match lazuli_analyzer::lower_design(&ast) {
                    Ok(design) => module.design = Some(design),
                    Err(err) => eprintln!(
                        "lazuli: skipping {}: design lower failed: {:?}",
                        design_path.display(),
                        err
                    ),
                },
                Err(err) => eprintln!(
                    "lazuli: skipping {}: design parse failed: {:?}",
                    design_path.display(),
                    err
                ),
            }
        }
    }

    let files: Vec<PathBuf> = if input.is_dir() {
        let mut out = Vec::new();
        collect_package_lzi_files(input, &mut out)?;
        out.sort();
        out
    } else {
        vec![input.to_path_buf()]
    };

    // Collect every parse/lower FAILURE across all files instead of
    // stopping at the first; a partial codegen run on a broken feature is
    // worse than a hard stop, and reporting all failures at once saves a
    // fix-rebuild-rediscover loop. Genuinely-optional warnings (the design
    // pipeline above) stay non-fatal — only parse/lower of feature
    // skeletons becomes fatal.
    let mut load_failures: Vec<String> = Vec::new();

    for path in &files {
        let source =
            fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        // App / registry / workspace manifests
        if module.app.is_none() {
            module.app = app_manifest::parse_app_manifest(&source);
        }
        if module.registry.is_none() {
            module.registry = app_manifest::parse_app_registry(&source);
        }
        if module.workspace.is_none() {
            module.workspace = app_manifest::parse_app_workspace(&source);
        }
        let contracts = app_manifest::parse_app_contracts(&source);
        if !contracts.is_empty() {
            module.contracts.extend(contracts);
        }
        let profiles = app_manifest::parse_app_profiles(&source);
        if !profiles.is_empty() {
            module.profiles.extend(profiles);
        }
        // Features via canonical-indent slice. A parse OR lower failure is
        // FATAL: skipping the feature would silently emit incomplete
        // codegen for a broken `.lzi`. Record the per-feature error text
        // and continue scanning so all failures surface at once, then abort
        // the command below.
        match lazuli_syntax::parse_feature_skeletons(&source) {
            Ok(skeletons) => {
                for ast in skeletons {
                    match lazuli_analyzer::lower_feature_skeleton(&ast) {
                        Ok(feature) => module.features.push(feature),
                        Err(err) => load_failures.push(format!(
                            "{}: feature lower failed: {:?}",
                            path.display(),
                            err
                        )),
                    }
                }
            }
            Err(err) => load_failures.push(format!(
                "{}: feature parse failed: {:?}",
                path.display(),
                err
            )),
        }
    }

    if !load_failures.is_empty() {
        anyhow::bail!(
            "lazuli: {} feature(s) failed to parse/lower:\n  {}",
            load_failures.len(),
            load_failures.join("\n  ")
        );
    }

    lazuli_analyzer::resolve_invalidates_targets(&mut module)
        .context("failed to resolve command invalidates targets")?;

    // L0 #3 — walk `features/<feat>/<feat>.{web,mobile}.lzx` and attach
    // the lowered `Surface` to the matching `Feature`. Skipped in
    // single-file input mode (no surrounding `features/` tree to walk).
    if input.is_dir() {
        attach_lzx_surfaces(input, &mut module);
    }

    // B3 — plugin-contributed `@semantic.<Name>` resolution. Reads the
    // app's `Lazurite.toml [plugins]`, opens each plugin's
    // `manifest.toml`, builds the alias map, and rewrites
    // `TypeRef::UserDefined("@semantic.<Name>")` field references to
    // `TypeRef::Builtin(BuiltinType::SemanticPluginType { ... })` so
    // codegen, doctor, and inspect see the typed shape.
    // Map failures are non-fatal here so a single-file `lazuli check`
    // (no project root) still works; the doctor surfaces conflicts /
    // unresolved aliases as `SEMANTIC-PLUGIN-001` against the field
    // site. See `docs/proposals/semantic-types-plugin-locales.md`.
    if input.is_dir() {
        let project_root = project_root_for_input(input);
        if let Ok(manifest) = lazurite_manifest::load(&project_root)
            && let Ok(alias_map) =
                plugin_manifest::build_alias_map(manifest.as_ref(), &project_root)
        {
            plugin_semantic_resolver::apply_plugin_semantic_resolution(&mut module, &alias_map);
        }
    }

    Ok(module)
}

pub(crate) fn build_module_with_source_from_path(
    input: &Path,
) -> Result<(
    lazuli_ir::Module,
    lazuli_ir::SourceMap,
    BTreeMap<String, lazuli_ir::FileId>,
)> {
    let mut module = lazuli_ir::Module {
        workspace: None,
        contracts: Vec::new(),
        app: None,
        registry: None,
        profiles: Vec::new(),
        design: None,
        rbac: None,
        features: Vec::new(),
    };
    let mut source_map = lazuli_ir::SourceMap { files: Vec::new() };
    let mut feature_file_ids = BTreeMap::new();

    // L0 #2 — Optional `design.lzi` at the input root. Mirrors
    // `build_module_from_path`; emitters and SDK projections consume
    // `module.design` when present.
    if input.is_dir() {
        let design_path = lazurite_manifest::resolve_in_app_dir(input, "design.lzi");
        if design_path.is_file() {
            let source = fs::read_to_string(&design_path)
                .with_context(|| format!("reading {}", design_path.display()))?;
            if let Ok(ast) = lazuli_syntax::parse_design_document(&source)
                && let Ok(design) = lazuli_analyzer::lower_design(&ast)
            {
                module.design = Some(design);
            }
        }
    }

    let files: Vec<PathBuf> = if input.is_dir() {
        let mut out = Vec::new();
        collect_package_lzi_files(input, &mut out)?;
        out.sort();
        out
    } else {
        vec![input.to_path_buf()]
    };

    let source_root = if input.is_dir() {
        input
    } else {
        input.parent().unwrap_or_else(|| Path::new("."))
    };

    // Mirror `build_module_from_path`: a parse/lower failure is FATAL on
    // the source-map build path too (used by `lazuli generate go`), so a
    // broken feature can never produce silently-incomplete codegen. Collect
    // all failures, then abort below.
    let mut load_failures: Vec<String> = Vec::new();

    for (idx, path) in files.iter().enumerate() {
        let file_id =
            u16::try_from(idx + 1).context("too many source files for SourceMap FileId")?;
        let source =
            fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let source_path = path
            .strip_prefix(source_root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        source_map
            .files
            .push(lazuli_ir::SourceMap::build_source_file(
                file_id,
                source_path,
                &source,
            ));

        if module.app.is_none() {
            module.app = app_manifest::parse_app_manifest(&source);
        }
        if module.registry.is_none() {
            module.registry = app_manifest::parse_app_registry(&source);
        }
        if module.workspace.is_none() {
            module.workspace = app_manifest::parse_app_workspace(&source);
        }
        let contracts = app_manifest::parse_app_contracts(&source);
        if !contracts.is_empty() {
            module.contracts.extend(contracts);
        }
        let profiles = app_manifest::parse_app_profiles(&source);
        if !profiles.is_empty() {
            module.profiles.extend(profiles);
        }
        match lazuli_syntax::parse_feature_skeletons(&source) {
            Ok(skeletons) => {
                for ast in skeletons {
                    match lazuli_analyzer::lower_feature_skeleton(&ast) {
                        Ok(feature) => {
                            feature_file_ids.insert(feature.name.clone(), file_id);
                            module.features.push(feature);
                        }
                        Err(err) => load_failures.push(format!(
                            "{}: feature lower failed: {:?}",
                            path.display(),
                            err
                        )),
                    }
                }
            }
            Err(err) => load_failures.push(format!(
                "{}: feature parse failed: {:?}",
                path.display(),
                err
            )),
        }
    }

    if !load_failures.is_empty() {
        anyhow::bail!(
            "lazuli: {} feature(s) failed to parse/lower:\n  {}",
            load_failures.len(),
            load_failures.join("\n  ")
        );
    }

    lazuli_analyzer::resolve_invalidates_targets(&mut module)
        .context("failed to resolve command invalidates targets")?;

    // L0 #3 — attach lowered `.lzx` surfaces alongside the source-map
    // build path (mirrors `build_module_from_path`).
    if input.is_dir() {
        attach_lzx_surfaces(input, &mut module);
    }

    Ok((module, source_map, feature_file_ids))
}

pub(crate) fn project_root_for_input(input: &Path) -> PathBuf {
    if input.is_dir() {
        return input.to_path_buf();
    }

    input
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf()
}

/// PG.C — walk `.lzi` files under `input` and aggregate plan-and-gate
/// facts in the codegen emit shape. Returns `None` when no plan
/// blocks, gate directives, or subscription anchors are declared
/// (codegen skips `dist/go/plan/catalog.gen.go`).
pub(crate) fn collect_plan_gate_facts_for_generate(
    input: &Path,
) -> Option<lazuli_codegen_go::PlanGateEmitFacts> {
    let mut plan_blocks: Vec<lazuli_syntax::PlanBlockAst> = Vec::new();
    let mut feature_gates: Vec<(String, lazuli_syntax::FeatureGatesAst)> = Vec::new();
    let mut anchor: Option<lazuli_ir::SubscriptionAnchor> = None;

    let project_root = project_root_for_input(input);
    let mut stack: Vec<PathBuf> = vec![project_root];
    while let Some(path) = stack.pop() {
        if path.is_dir() {
            let Ok(entries) = fs::read_dir(&path) else {
                continue;
            };
            for entry in entries.flatten() {
                stack.push(entry.path());
            }
            continue;
        }
        if path.extension().and_then(|s| s.to_str()) != Some("lzi") {
            continue;
        }
        let Ok(source) = fs::read_to_string(&path) else {
            continue;
        };
        if anchor.is_none()
            && let Some(a) = lazuli_analyzer::parse_subscription_anchor(&source)
        {
            anchor = Some(a);
        }
        if let Ok(blocks) = lazuli_syntax::parse_plan_blocks(&source) {
            plan_blocks.extend(blocks);
        }
        if let Ok(fg) = lazuli_syntax::parse_feature_gates(&source)
            && !fg.callables.is_empty()
        {
            let feature_name = source
                .lines()
                .find_map(|l| {
                    l.trim_start()
                        .strip_prefix("feature ")
                        .map(|s| s.to_owned())
                })
                .and_then(|s| s.split_whitespace().next().map(|s| s.to_owned()))
                .unwrap_or_else(|| {
                    path.file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("unknown")
                        .to_owned()
                });
            feature_gates.push((feature_name, fg));
        }
    }

    if plan_blocks.is_empty() && feature_gates.is_empty() && anchor.is_none() {
        return None;
    }
    let facts = lazuli_analyzer::aggregate_plan_gate_facts(&plan_blocks, &feature_gates, anchor);
    Some(lazuli_codegen_go::PlanGateEmitFacts {
        catalog: facts.catalog,
        subscription_anchor: facts.subscription_anchor,
        gates: facts.gates,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// A minimal, known-good single-feature `.lzi`.
    const VALID_FEATURE: &str = "feature hello\n  domain\n    record GreetOutput\n      message: Text required\n\n  command greet\n    input\n      name: Text required\n    returns GreetOutput\n    policy @policy.public\n    handler @fn.greet\n";

    /// The valid feature plus a `job` block carrying an unknown child —
    /// `parse_feature_skeletons` rejects it (strict job-children grammar).
    const PARSE_BROKEN_FEATURE: &str = "feature hello\n  domain\n    record GreetOutput\n      message: Text required\n\n  job nightly\n    bogus_unknown_child \"not a valid job child\"\n";

    fn write_temp_lzi(contents: &str) -> tempfile::TempPath {
        let mut file = tempfile::Builder::new()
            .suffix(".lzi")
            .tempfile()
            .expect("create temp .lzi");
        file.write_all(contents.as_bytes())
            .expect("write temp .lzi");
        file.flush().expect("flush temp .lzi");
        file.into_temp_path()
    }

    /// Positive control: a valid feature loads to an `Ok` module with the
    /// feature present (proves the broken-case assertion below is about the
    /// error, not an unrelated loader failure).
    #[test]
    fn build_module_from_path_loads_valid_feature() {
        let path = write_temp_lzi(VALID_FEATURE);
        let module = build_module_from_path(&path).expect("valid feature should load");
        assert_eq!(module.features.len(), 1, "expected the one feature to load");
    }

    /// Regression: a parse failure is now a HARD ERROR (was eprintln-and-skip
    /// → silent Ok with an incomplete module). Must return `Err`, not
    /// `Ok`-with-the-feature-skipped.
    #[test]
    fn build_module_from_path_errors_on_parse_failure() {
        let path = write_temp_lzi(PARSE_BROKEN_FEATURE);
        let result = build_module_from_path(&path);
        assert!(
            result.is_err(),
            "a parse failure must abort the loader, not skip the feature"
        );
        let msg = format!("{:#}", result.unwrap_err());
        assert!(
            msg.contains("failed to parse/lower"),
            "error should name the parse/lower failure, got: {msg}"
        );
    }

    /// The source-map build path (used by `lazuli generate go`) must abort
    /// on a parse failure too — silent partial codegen is the worst case.
    #[test]
    fn build_module_with_source_from_path_errors_on_parse_failure() {
        let path = write_temp_lzi(PARSE_BROKEN_FEATURE);
        let result = build_module_with_source_from_path(&path);
        assert!(
            result.is_err(),
            "the source-map loader must abort on a parse failure too"
        );
    }
}
