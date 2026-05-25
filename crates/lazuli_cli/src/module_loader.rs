//! Lazuli source discovery and module loading.
//!
//! Carved out of `main.rs` as part of Wave R6-5 (Rails-style refactor).
//! This module owns the file-system walkers and IR builders that turn a
//! `.lzi` / `.lzx` file or directory into the typed module shapes that
//! every `lazuli` subcommand consumes:
//!
//! - **Discovery walkers**: [`collect_package_lzi_files`],
//!   [`collect_package_lzx_files`] (recursive, skipping `.git`, `dist`,
//!   `node_modules`, …).
//! - **Source aggregation**: [`read_package_lzi_source`] concatenates
//!   every `.lzi` in the package for `lazuli inspect`.
//! - **Module builders**: [`build_module_from_path`] and
//!   [`build_module_with_source_from_path`] lower the discovered files
//!   into `lazuli_ir::Module` (the source-map variant returns a
//!   `lazuli_ir::SourceMap` and per-feature `FileId` table for codegen
//!   `//line` directives).
//! - **`.lzx` bundling**: [`collect_lzx_experience_module`] +
//!   [`collect_lzx_bundle`] + [`LzxBundle`] lift app/route/experience/
//!   surface IR out of `.lzx` files.
//! - **Surface attachment**: [`attach_lzx_surfaces`] wires per-feature
//!   `.web.lzx`/`.mobile.lzx` into the matching `lazuli_ir::Feature`.
//! - **Project root resolution**: [`project_root_for_input`].
//! - **Plan/gate aggregation**: [`collect_plan_gate_facts_for_generate`].
//!
//! No behavior change vs. the pre-split build.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use lazuli_analyzer::source_map::SourceMapResolver as _;

use crate::app_manifest;
use crate::lazurite_manifest;
use crate::plugin_manifest;
use crate::plugin_semantic_resolver;

/// Recursively collect every `.lzi` file under a package root, skipping
/// well-known noise directories (build output, vcs metadata, vendored
/// deps). Honors the Lazurite convention (`features/<name>/<name>.lzi`)
/// without requiring callers to enumerate features.
pub(crate) fn collect_package_lzi_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    const SKIP: &[&str] = &[
        ".git",
        ".hg",
        ".svn",
        ".lazuli",
        "dist",
        "node_modules",
        "target",
    ];
    let entries =
        fs::read_dir(dir).with_context(|| format!("reading directory {}", dir.display()))?;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if SKIP.iter().any(|s| *s == name) {
                continue;
            }
            collect_package_lzi_files(&path, out)?;
        } else if path.extension().and_then(|s| s.to_str()) == Some("lzi") {
            out.push(path);
        }
    }
    Ok(())
}

pub(crate) fn collect_package_lzx_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    const SKIP: &[&str] = &[
        ".git",
        ".hg",
        ".svn",
        ".lazuli",
        "dist",
        "node_modules",
        "target",
    ];
    let entries =
        fs::read_dir(dir).with_context(|| format!("reading directory {}", dir.display()))?;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if SKIP.iter().any(|s| *s == name) {
                continue;
            }
            collect_package_lzx_files(&path, out)?;
        } else if path.extension().and_then(|s| s.to_str()) == Some("lzx") {
            out.push(path);
        }
    }
    Ok(())
}

pub(crate) fn collect_lzx_experience_module(input: &Path) -> lazuli_ir::ExperienceModule {
    let mut module = lazuli_ir::ExperienceModule {
        app: None,
        routes: Vec::new(),
        experiences: Vec::new(),
        surfaces: Vec::new(),
    };
    let mut files = Vec::new();
    let result = if input.is_dir() {
        collect_package_lzx_files(input, &mut files)
    } else if input.extension().and_then(|s| s.to_str()) == Some("lzx") {
        files.push(input.to_path_buf());
        Ok(())
    } else {
        Ok(())
    };
    if let Err(err) = result {
        eprintln!("lazuli: skipping .lzx route lift: {err:#}");
        return module;
    }
    files.sort();
    for path in files {
        let source = match fs::read_to_string(&path) {
            Ok(source) => source,
            Err(err) => {
                eprintln!("lazuli: skipping {}: {err}", path.display());
                continue;
            }
        };
        let parsed = match lazuli_syntax::parse_lzx_document(&source) {
            Ok(parsed) => parsed,
            Err(err) => {
                eprintln!(
                    "lazuli: skipping {}: lzx parse failed: {:?}",
                    path.display(),
                    err
                );
                continue;
            }
        };
        let lowered = lazuli_analyzer::lower_lzx_document(&parsed);
        if module.app.is_none() {
            module.app = lowered.app;
        }
        module.routes.extend(lowered.routes);
        module.experiences.extend(lowered.experiences);
        module.surfaces.extend(lowered.surfaces);
    }
    module
}

pub(crate) fn read_package_lzi_source(dir: &Path) -> Result<String> {
    let mut files = Vec::new();
    collect_package_lzi_files(dir, &mut files)?;
    files.sort();
    if files.is_empty() {
        bail!("{} contains no `.lzi` files to inspect", dir.display());
    }

    let mut source = String::new();
    for path in files {
        if !source.is_empty() {
            source.push_str("\n\n");
        }
        source.push_str(
            &fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?,
        );
    }
    Ok(source)
}

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
        // Features via canonical-indent slice
        match lazuli_syntax::parse_feature_skeletons(&source) {
            Ok(skeletons) => {
                for ast in skeletons {
                    match lazuli_analyzer::lower_feature_skeleton(&ast) {
                        Ok(feature) => module.features.push(feature),
                        Err(err) => eprintln!(
                            "lazuli: skipping feature in {}: lower failed: {:?}",
                            path.display(),
                            err
                        ),
                    }
                }
            }
            Err(err) => eprintln!(
                "lazuli: skipping {}: feature parse failed: {:?}",
                path.display(),
                err
            ),
        }
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
        if let Ok(manifest) = lazurite_manifest::load(&project_root) {
            if let Ok(alias_map) =
                plugin_manifest::build_alias_map(manifest.as_ref(), &project_root)
            {
                plugin_semantic_resolver::apply_plugin_semantic_resolution(&mut module, &alias_map);
            }
        }
    }

    Ok(module)
}

/// L0 #3 — look for `features/<feature>/<feature>.web.lzx` and
/// `features/<feature>/<feature>.mobile.lzx` next to each parsed
/// `Feature` and attach the lowered `Surface` records. Missing files
/// are silently skipped; parse / lower errors are reported but do not
/// fail the build.
fn attach_lzx_surfaces(input: &Path, module: &mut lazuli_ir::Module) {
    let features_root = input.join("features");
    if !features_root.is_dir() {
        return;
    }
    for feature in module.features.iter_mut() {
        let feat_dir = features_root.join(&feature.name);
        if !feat_dir.is_dir() {
            continue;
        }
        for (target_label, parsed_target) in [
            ("web", lazuli_syntax::SurfaceTargetAst::Web),
            ("mobile", lazuli_syntax::SurfaceTargetAst::Mobile),
        ] {
            let path = feat_dir.join(format!("{}.{}.lzx", feature.name, target_label));
            if !path.is_file() {
                continue;
            }
            let source = match fs::read_to_string(&path) {
                Ok(s) => s,
                Err(err) => {
                    eprintln!(
                        "lazuli: skipping {}: read failed: {:?}",
                        path.display(),
                        err
                    );
                    continue;
                }
            };
            let ast = match lazuli_syntax::parse_surface_document(&source) {
                Ok(ast) => ast,
                Err(err) => {
                    eprintln!(
                        "lazuli: skipping {}: surface parse failed: {:?}",
                        path.display(),
                        err
                    );
                    continue;
                }
            };
            if ast.target != parsed_target {
                eprintln!(
                    "lazuli: skipping {}: surface target `{:?}` does not match filename target `{}`",
                    path.display(),
                    ast.target,
                    target_label,
                );
                continue;
            }
            match lazuli_analyzer::lower_surface(&ast) {
                Ok(surface) => feature.surfaces.push(surface),
                Err(err) => eprintln!(
                    "lazuli: skipping {}: surface lower failed: {:?}",
                    path.display(),
                    err
                ),
            }
        }
    }
}

#[derive(Default)]
pub(crate) struct LzxBundle {
    pub(crate) app: Option<lazuli_ir::AppManifest>,
    pub(crate) routes: Vec<lazuli_ir::AppRoute>,
    pub(crate) experiences: Vec<lazuli_ir::Experience>,
    pub(crate) surfaces: Vec<lazuli_ir::PlatformSurface>,
}

pub(crate) fn collect_lzx_bundle(input: &Path) -> LzxBundle {
    let mut files = Vec::new();
    if input.is_dir() {
        let _ = collect_package_lzx_files(input, &mut files);
    } else if input.extension().and_then(|s| s.to_str()) == Some("lzx") {
        files.push(input.to_path_buf());
    }
    files.sort();

    let mut bundle = LzxBundle::default();
    for path in files {
        let source = match fs::read_to_string(&path) {
            Ok(source) => source,
            Err(err) => {
                eprintln!(
                    "lazuli: skipping {}: read failed: {:?}",
                    path.display(),
                    err
                );
                continue;
            }
        };
        let document = match lazuli_syntax::parse_lzx_document(&source) {
            Ok(document) => document,
            Err(err) => {
                eprintln!(
                    "lazuli: skipping {}: lzx parse failed: {:?}",
                    path.display(),
                    err
                );
                continue;
            }
        };
        let lowered = lazuli_analyzer::lower_lzx_document(&document);
        if bundle.app.is_none() {
            bundle.app = lowered.app;
        }
        bundle.routes.extend(lowered.routes);
        bundle.experiences.extend(lowered.experiences);
        bundle.surfaces.extend(lowered.surfaces);
    }
    bundle
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
            if let Ok(ast) = lazuli_syntax::parse_design_document(&source) {
                if let Ok(design) = lazuli_analyzer::lower_design(&ast) {
                    module.design = Some(design);
                }
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
        if let Ok(skeletons) = lazuli_syntax::parse_feature_skeletons(&source) {
            for ast in skeletons {
                if let Ok(feature) = lazuli_analyzer::lower_feature_skeleton(&ast) {
                    feature_file_ids.insert(feature.name.clone(), file_id);
                    module.features.push(feature);
                }
            }
        }
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
        if anchor.is_none() {
            if let Some(a) = lazuli_analyzer::parse_subscription_anchor(&source) {
                anchor = Some(a);
            }
        }
        if let Ok(blocks) = lazuli_syntax::parse_plan_blocks(&source) {
            plan_blocks.extend(blocks);
        }
        if let Ok(fg) = lazuli_syntax::parse_feature_gates(&source) {
            if !fg.callables.is_empty() {
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
