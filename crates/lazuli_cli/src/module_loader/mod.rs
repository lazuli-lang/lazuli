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

mod collectors;
mod lzx_bundle;
mod plugin_resolution;

pub use plugin_resolution::{find_project_root, resolve_module_plugins};

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
pub fn build_module_from_path(input: &Path) -> Result<lazuli_ir::Module> {
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
                        Ok(mut feature) => {
                            // Embed each `query.sql` / `query.view` body so
                            // codegen can emit `SQLText` and the generated
                            // app has no runtime dependency on the `.sql`
                            // file. The feature dir is the directory holding
                            // the `.lzi`, the same anchor the doctor's
                            // escape-hatch rules resolve `sql_path` against.
                            embed_sql_query_bodies(&mut feature, path.parent());
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

    // Spec 0014 GAP-1 — re-resolve `restrict on_delete` guard scopes against
    // EVERY feature's resources, so a guard referencing a relation owned by
    // another feature still emits `AND tenant_id` + `AND deleted_at IS NULL`
    // (the per-feature pass only saw same-feature resources).
    lazuli_analyzer::resolve_restrict_on_delete_scopes_module(&mut module);

    // L0 #3 — walk `features/<feat>/<feat>.{web,mobile}.lzx` and attach
    // the lowered `Surface` to the matching `Feature`. Skipped in
    // single-file input mode (no surrounding `features/` tree to walk).
    if input.is_dir() {
        attach_lzx_surfaces(input, &mut module);
    }

    // 0019 — the SINGLE plugin-semantic resolution stage. Both loaders
    // funnel through `resolve_module_plugins`: it walks UP to the
    // nearest `Lazurite.toml`, builds the alias map, rewrites
    // `TypeRef::UserDefined("@semantic.<Name>")` references to the typed
    // `SemanticPluginType`, and — when a `[plugins]` block is declared —
    // fails LOUD on any residual unresolved alias. The single-file
    // `lazuli check` path (no project root) stays a silent no-op so the
    // doctor can anchor `SEMANTIC-PLUGIN-001` at the field site.
    plugin_resolution::resolve_module_plugins(&mut module, input)?;

    Ok(module)
}

pub fn build_module_with_source_from_path(
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
                        Ok(mut feature) => {
                            // Embed each `query.sql` / `query.view` body so
                            // codegen emits `SQLText` (see the sibling call in
                            // `build_module_from_path`). This is the path
                            // `lazuli generate go` actually walks.
                            embed_sql_query_bodies(&mut feature, path.parent());
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

    // Spec 0014 GAP-1 — re-resolve `restrict on_delete` guard scopes against
    // EVERY feature's resources, so a guard referencing a relation owned by
    // another feature still emits `AND tenant_id` + `AND deleted_at IS NULL`
    // (the per-feature pass only saw same-feature resources).
    lazuli_analyzer::resolve_restrict_on_delete_scopes_module(&mut module);

    // L0 #3 — attach lowered `.lzx` surfaces alongside the source-map
    // build path (mirrors `build_module_from_path`).
    if input.is_dir() {
        attach_lzx_surfaces(input, &mut module);
    }

    // 0019 — same SINGLE resolution stage as `build_module_from_path`.
    // This is the path `lazuli generate go` uses (`with_source=true`);
    // before 0019 it lacked the resolver entirely, which is why
    // hostpoint's plugin `@semantic.Brazilian*` refs reached Go codegen
    // as `UserDefined` and tripped the closed semantic table. Placed
    // after feature lowering + `.lzx` attach so resolution sees the
    // final TypeRef sites (mirrors the other loader's call point).
    resolve_module_plugins(&mut module, input)?;

    Ok((module, source_map, feature_file_ids))
}

/// Read each `query.sql` / `query.view` `.sql` body into
/// `SqlQuery.sql_text` so codegen can embed it as `lazuli.Query.SQLText`
/// (the runtime prefers the embedded body over reading the file path at
/// run time, removing the generated app's dependency on the `.sql` file
/// shipping alongside the binary, and the flat-`dist/go/queries/`
/// namespace-collision risk).
///
/// `feature_dir` is the directory holding the feature's `.lzi` — the same
/// anchor the doctor's escape-hatch rules resolve `sql_path` against. The
/// two authored `sql_path` shapes both resolve here:
///   - `"./queries/<name>.sql"` (verbatim, feature-dir-relative), and
///   - `@file.<name>.sql` lowered to `app/features/<feature>/queries/<name>.sql`
///     (project-root-relative).
/// We try the feature-dir join first (covers the `./...` form); if that
/// file is absent, we walk up to the project root and try the path verbatim
/// (covers the lowered `app/features/...` form). A read failure leaves
/// `sql_text` as `None` — codegen then emits only the `SQL:` path and the
/// doctor's `query.sql`/view file-existence rule reports the missing file.
fn embed_sql_query_bodies(feature: &mut lazuli_ir::Feature, feature_dir: Option<&Path>) {
    let Some(feature_dir) = feature_dir else {
        return;
    };
    for query in &mut feature.queries {
        let lazuli_ir::Query::Sql(sql) = query else {
            continue;
        };
        if sql.sql_text.is_some() {
            continue;
        }
        sql.sql_text = read_sql_body(feature_dir, &sql.sql_path);
    }
}

/// Resolve + read a `query.sql` body. See [`embed_sql_query_bodies`] for the
/// two `sql_path` shapes this must cover.
fn read_sql_body(feature_dir: &Path, sql_path: &str) -> Option<String> {
    // Form 1: feature-dir-relative (`./queries/<name>.sql`) — and the
    // absolute-path passthrough.
    if let Ok(body) = fs::read_to_string(feature_dir.join(sql_path)) {
        return Some(body);
    }
    // Form 2: project-root-relative (`app/features/<feature>/queries/<name>.sql`,
    // emitted by `@file.` lowering). `feature_dir` is `.../app/features/<feature>`,
    // so the project root is two levels up.
    let project_root = feature_dir.parent().and_then(Path::parent);
    if let Some(root) = project_root
        && let Ok(body) = fs::read_to_string(root.join(sql_path))
    {
        return Some(body);
    }
    None
}

pub(crate) fn project_root_for_input(input: &Path) -> PathBuf {
    // C1 (SEAM 3): walk UP from `input` to the nearest ancestor holding
    // `Lazurite.toml` — the same cargo/git/npm root rule the plugin
    // resolver already uses (`lazurite_manifest::find_project_root`).
    // Without this, `lazuli generate go app` (features under `app/`,
    // manifest at the repo root) loads NO manifest and falls back to the
    // synthetic `lazuli/<app>` module name, producing a non-integrating
    // tree. Walking up makes the manifest win regardless of input depth.
    if let Some(root) = lazurite_manifest::find_project_root(input) {
        return root;
    }

    // No manifest anywhere up the tree: preserve the historical
    // default-shaped behaviour (input dir itself, or the file's parent).
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

    const MIN_MANIFEST: &str =
        "[project]\nname = \"acme\"\nschema = 1\nversion = \"0.1.0\"\n\n[lazuli]\nruntime = \"0.1.0\"\n";

    /// C1: `lazuli generate go app` (features under `app/`, manifest at the
    /// repo root) must discover the repo-root manifest by walking UP from the
    /// input dir — like cargo/git/npm find their root. Before the fix
    /// `project_root_for_input` returned the input dir itself, so the manifest
    /// was never loaded and the module name fell back to synthetic
    /// `lazuli/<app>`, producing a non-integrating tree.
    #[test]
    fn project_root_for_input_walks_up_to_manifest() {
        let root = tempfile::tempdir().expect("create temp project root");
        std::fs::write(root.path().join("Lazurite.toml"), MIN_MANIFEST)
            .expect("write Lazurite.toml at root");
        let app_dir = root.path().join("app");
        std::fs::create_dir_all(app_dir.join("features")).expect("create app/features");

        // From the `app/` subdir: must walk up to the root holding the manifest.
        assert_eq!(
            project_root_for_input(&app_dir),
            root.path(),
            "should walk up from app/ to the manifest-bearing root"
        );
        // From a deeper subdir too.
        assert_eq!(
            project_root_for_input(&app_dir.join("features")),
            root.path(),
            "should walk up from app/features/ to the manifest-bearing root"
        );
        // From the root itself: returns the root.
        assert_eq!(
            project_root_for_input(root.path()),
            root.path(),
            "root input should resolve to itself"
        );
    }

    /// C1 back-compat: with NO manifest anywhere up the tree, the historical
    /// default-shaped behaviour is preserved — a directory input resolves to
    /// itself (pilots without a `Lazurite.toml` still work).
    #[test]
    fn project_root_for_input_no_manifest_returns_input_dir() {
        let dir = tempfile::tempdir().expect("create temp dir without manifest");
        let sub = dir.path().join("nested");
        std::fs::create_dir_all(&sub).expect("create nested dir");
        // No Lazurite.toml exists; tempdirs live under the OS temp root which
        // has none up the chain. Resolve to the input dir itself.
        assert_eq!(
            project_root_for_input(&sub),
            sub,
            "no manifest up-tree should fall back to the input dir"
        );
    }
}
