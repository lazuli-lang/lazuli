use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::Parser;

mod app_manifest;
mod casing;
mod cli_args;
mod cmd_design;
mod cmd_fix;
mod cmd_generate_command;
mod cmd_generate_feature;
mod cmd_generate_handler;
mod cmd_generate_playwright;
mod cmd_generate_rule;
mod cmd_generate_transition;
mod cmd_generate_view;
mod cmd_mcp;
mod cmd_new_frontends;
mod cmd_test;
mod cmd_test_fail_fast;
mod cmd_test_ndjson;
mod cmd_test_output;
mod cmd_test_types;
mod cmd_test_watch;
mod commands;
mod coverage_aggregator;
mod debug;
mod dev;
mod doctor;
mod doctor_report;
mod doctor_watch;
mod examples_bundle;
mod inspect {
    pub mod expand_auth;
    pub mod expand_http;
    pub mod features_summary;
}
mod lazurite_manifest;
mod migrate;
mod path_utils;
mod plugin_catalog;
mod plugin_manifest;
mod plugin_semantic_resolver;
mod profile;
mod runners;
mod seed;
mod signature_aware_stub;
mod templates;
mod upgrade;
mod version;

// `doctor::schema_rich_001` reached for `crate::pascal_case`; preserve
// that surface plus expose the other casers crate-wide so the carved-out
// `commands::generate::ts::*` and `commands::new::*` modules can call
// them without per-module re-imports.
pub(crate) use casing::{lower_camel, pascal_case, to_kebab_case, to_snake_case};
pub(crate) use path_utils::{absolutize_for_codegen, absolutize_project_root, relative_path};
// `crate::tests` and a few legacy paths reach for these by short name;
// the canonical home is `commands::new`.
pub(crate) use commands::new::{
    new_command,
    scaffold::{
        app_template, default_module_name, pascal_case_project_name, scaffold_bare,
        scaffold_from_template,
    },
};
// `crate::tests` references these TS-emitter functions by short name;
// the canonical home is `commands::generate::ts`. Also re-exports the
// helpers reached for by `doctor::schema_rich_001` (`command_schema_ident`,
// `command_zod_slots`, `find_enum_decl`) so the diagnostic can stay
// where it is without per-call-site path imports.
#[allow(unused_imports)]
pub(crate) use commands::generate::ts::{
    command_schema_ident, command_zod_slots, emit_feature_barrel_ts, emit_feature_react_hooks_ts,
    emit_feature_sdk_ts, emit_feature_zod_ts, find_enum_decl, generate_ts, zod_base_for_type_ref,
};
// `cmd_mcp` reaches for `crate::ExpandSet`, `crate::parse_expand_set`,
// `crate::inspect_json_value`; `tests.rs` reaches for these plus
// `crate::inspect_canonical_source`, `crate::expand_canonical_source`
// (test-only), `crate::render_inspect_symbol_lazuli`. Canonical home is
// `commands::inspect`.
#[cfg(test)]
#[allow(unused_imports)]
pub(crate) use commands::inspect::expand_canonical_source;
#[allow(unused_imports)]
pub(crate) use commands::inspect::{
    ExpandSet, inspect_canonical_source, inspect_json_value, inspect_source_path, parse_expand_set,
    render_inspect_symbol_lazuli,
};

const DEFAULT_TEMPLATE: &str = include_str!("../../../examples/crm.lzi");
const REGISTRY_TEMPLATE: &str =
    "registry\n  # capabilities: name typed\n  # integrations: provider-neutral declarations\n";
// Closes WAR-SCAFFOLD-GITIGNORE-01. The previous template's blanket
// `dist/` rule ignored user-authored handler files at
// `dist/go/<bc>/<name>.go`, violating Lazuli's regen contract (gen
// files are overwritable, non-gen files are sacred). The granular
// pattern below ignores ONLY regen-overwritable artifacts:
//   - `*.gen.go` / `*.gen.ts` / `*.zod.ts` (codegen outputs)
//   - `dist/ts-{web,mobile}/**/index.ts` (generated feature barrels)
//   - `dist/go/{main.go,go.mod,go.sum,migrations/}` (full-rewrite slots)
//   - `dist/{ts-web,ts-mobile}/design/` (design-token snapshots)
// User-authored files (`dist/go/<bc>/<name>.go` handlers) stay tracked.
const GITIGNORE_TEMPLATE: &str = r#"# Rust
/target/
**/*.rs.bk

# Go
/bin/
*.exe
*.test
*.out
coverage.out

# Node
node_modules/
npm-debug.log*
yarn-debug.log*
yarn-error.log*
pnpm-debug.log*
build/

# Lazuli generated artifacts (regen-overwritable).
# User-authored handler files at dist/go/<bc>/<name>.go stay tracked.
dist/**/*.gen.go
dist/**/*.gen.ts
dist/**/*.zod.ts
dist/ts-web/**/index.ts
dist/ts-mobile/**/index.ts
dist/go/main.go
dist/go/go.mod
dist/go/go.sum
dist/go/migrations/
dist/ts-web/design/
dist/ts-mobile/design/

# Lazuli internal cache.
.lazuli/
"#;

use cli_args::{
    Cli, Commands, DesignCommand, ExamplesCommand, MigrateCommand, TranslateCommand,
};
// Sibling modules reach for these clap enums by short name
// (`crate::PlaywrightTarget`, `crate::InspectFormat`, …); re-export so
// the carve-out of `cli_args` is invisible from outside `main.rs`.
pub(crate) use cli_args::{GenerateKind, InspectFormat, InspectInclude, PlaywrightTarget};


fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Parse { input } => commands::parse::parse_command(&input),
        Commands::Check {
            input,
            security_profile,
        } => commands::check::check_command(
            &input,
            security_profile.into(),
            cli.allow_version_mismatch,
        ),
        Commands::Doctor {
            input,
            security_profile,
            check_release,
            format,
            coverage,
            fail_on,
            self_audit,
        } => doctor::doctor_command_with_options(
            &input,
            security_profile.into(),
            check_release,
            cli.allow_version_mismatch,
            doctor::DoctorRuntimeOptions {
                format: Some(format),
                coverage,
                fail_on,
                self_audit,
            },
        ),
        Commands::Inspect {
            input,
            expand,
            include,
            format,
        } => commands::inspect::inspect_command(&input, &expand, format, &include),
        Commands::Debug {
            error,
            capsule,
            project,
            format,
        } => commands::debug::debug_command(&project, error.as_deref(), capsule, &format),
        Commands::Profile {
            profile,
            top,
            by,
            format,
        } => commands::profile::profile_command(&profile, top, &by, &format),
        Commands::Examples { sub } => match sub {
            ExamplesCommand::Bundle { out } => commands::examples::bundle_command(out.as_deref()),
            ExamplesCommand::Validate { check_decay } => {
                commands::examples::validate_command(check_decay)
            }
        },
        Commands::Init { path } => commands::init::init_command(&path, DEFAULT_TEMPLATE),
        Commands::New {
            project_name,
            template,
            bare,
            no_git,
            module,
            frontends,
            in_place,
        } => commands::new::new_command(
            project_name.as_deref(),
            &template,
            bare,
            no_git,
            module,
            frontends,
            in_place,
        ),
        Commands::Lsp { stdio: _ } => commands::lsp::lsp_command(),
        Commands::SpikeGenerate { root, spec } => {
            commands::spike_generate::spike_generate_command(&root, spec.as_deref())
        }
        Commands::Plan { input, check } => commands::plan::plan_command(&input, check.as_deref()),
        Commands::Generate {
            kind,
            input,
            output,
            api_version,
            module,
            lazuli_go_version,
            check,
            with_source,
            allow_drops,
            playwright_target,
        } => commands::generate::generate_command(
            kind,
            &input,
            output.as_deref(),
            api_version.as_deref(),
            module.as_deref(),
            lazuli_go_version.as_deref(),
            check,
            with_source,
            allow_drops,
            cli.allow_version_mismatch,
            playwright_target,
        ),
        Commands::Dev {
            path,
            out,
            no_run,
            debounce,
        } => commands::dev::dev_command(path, out, no_run, debounce),
        Commands::Migrate { sub } => match sub {
            MigrateCommand::Up { target, yes } => commands::migrate::up_command(target, yes),
            MigrateCommand::Down { steps, yes } => commands::migrate::down_command(steps, yes),
            MigrateCommand::Status => commands::migrate::status_command(),
            MigrateCommand::Dsl {
                from,
                to,
                dry_run,
                path,
            } => commands::migrate::dsl_command(&from, &to, dry_run, path),
        },
        Commands::Design { sub } => match sub {
            DesignCommand::Import {
                from,
                format,
                overwrite,
            } => commands::design::import_command(&from, format.into(), overwrite),
            DesignCommand::Export { target, out } => {
                commands::design::export_command(target.into(), &out)
            }
            DesignCommand::Diff { against } => commands::design::diff_command(&against),
        },
        Commands::Upgrade {
            from,
            to,
            target,
            dry_run,
        } => commands::upgrade::upgrade_command(&from, &to, &target, dry_run),
        Commands::Seed { only, force } => commands::seed::seed_command(only.as_deref(), force),
        Commands::Changelog { from, to, output } => {
            commands::changelog::changelog_command(&from, &to, output.as_deref())
        }
        Commands::Translate { sub } => match sub {
            TranslateCommand::Extract {
                input,
                out,
                locale,
                check,
            } => commands::translate::translate_extract_command(
                &input,
                &out,
                locale.as_deref(),
                check,
            ),
        },
        Commands::Mcp => commands::mcp::mcp_command(),
        Commands::Test {
            input,
            layer,
            format,
            coverage,
            fail_on,
            watch,
            fail_fast,
            aggregate_method,
            extra_args,
        } => commands::test::test_command(
            input,
            layer.into_iter().map(Into::into).collect(),
            format,
            coverage,
            fail_on,
            watch,
            fail_fast,
            aggregate_method,
            extra_args,
        ),
    }
}

fn codegen_lazurite_manifest(
    manifest: &lazurite_manifest::Manifest,
    project_root: &Path,
    out_dir: Option<&Path>,
) -> lazuli_codegen_go::LazuriteManifest {
    use std::collections::BTreeMap;

    let plugins = manifest
        .plugins
        .iter()
        .map(|(plugin_ref, plugin)| {
            let (module, version, path) = match plugin {
                lazurite_manifest::Plugin::Remote { module, version } => {
                    (Some(module.clone()), Some(version.clone()), None)
                }
                lazurite_manifest::Plugin::Local { path } => (None, None, Some(path.clone())),
            };
            // Resolve the plugin's Go module path so codegen can emit a
            // side-effect import in main.go. For Remote plugins the
            // Lazurite.toml `module` IS the Go module path; for Local
            // plugins we read the first-line `module ...` from
            // `<path>/go.mod`. This closes the init-order panic class
            // by guaranteeing the plugin's package init() lands in the
            // binary's transitive import graph — see
            // `runtime/go/lazuli/app_integration.go` for the deferred
            // resolution that lets Local plugins register their adapter
            // before the first facade call.
            let go_module = match plugin {
                lazurite_manifest::Plugin::Remote { module, .. } => Some(module.clone()),
                lazurite_manifest::Plugin::Local { path } => {
                    read_plugin_go_module(project_root, path)
                }
            };
            (
                plugin_ref.clone(),
                lazuli_codegen_go::LazuritePlugin {
                    module,
                    version,
                    path,
                    go_module,
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    // Frente 1 — `[generate.go]` defaults apply transparently. Pilots
    // can omit the block entirely; the canonical
    // `emit_main = true / submodule = true / out = "dist/go"` shape lifts
    // from `GenerateGo::default()`.
    let generate_go = {
        let go = manifest.generate_go_or_default();
        // Resolve the Lazuli runtime/go path. Lazurite.toml's
        // `[lazuli] path` is authoritative (the user explicitly
        // points at a local checkout); fall back to the ancestor
        // heuristic for legacy projects that haven't set it.
        let detected =
            out_dir.and_then(|out_dir| detect_runtime_dev_replace(project_root, out_dir));
        let manifest_runtime = manifest.lazuli.path.as_ref().map(|p| {
            // `path` is the lazuli source ROOT (e.g. `../lazuli`);
            // the runtime/go module lives at `<root>/runtime/go`.
            let runtime_rel = format!("{}/runtime/go", p.trim_end_matches('/'));
            // dist/go/go.mod sits TWO levels deeper than the project
            // root (project → dist → dist/go), so prepend `../../`
            // for the go.mod replace.
            RuntimeDevReplace {
                go_mod: format!("../../{}", runtime_rel),
                go_work: runtime_rel,
            }
        });
        let resolved = manifest_runtime.or(detected);
        Some(lazuli_codegen_go::LazuriteGenerateGo {
            emit_main: go.emit_main,
            submodule: go.submodule,
            dev_replace: go
                .dev_replace
                .clone()
                .or_else(|| resolved.as_ref().map(|paths| paths.go_mod.clone())),
            dev_work_replace: go
                .dev_replace
                .clone()
                .or_else(|| resolved.map(|paths| paths.go_work)),
        })
    };
    let dev = manifest
        .dev
        .as_ref()
        .map(|dev| lazuli_codegen_go::LazuriteDev {
            plugin_paths: dev.plugin_paths.clone(),
        });

    lazuli_codegen_go::LazuriteManifest {
        project_module: manifest.project.module.clone(),
        plugins,
        generate_go,
        dev,
    }
}

/// Read the first-line `module <path>` directive from a local plugin's
/// `go.mod`. Used by `codegen_lazurite_manifest` to discover the Go
/// module path the codegen needs to emit a `_ "<module>"` side-effect
/// import in main.go (so the plugin's package init() runs and its
/// `lazuli.RegisterAdapter(...)` populates the registry).
///
/// Returns `None` when:
/// - the path does not resolve to a directory containing `go.mod`
/// - the file is unreadable
/// - no `module` directive is found in the first ~20 lines
///
/// `None` is a soft failure: the emitter skips that plugin's import,
/// which surfaces as the existing `ErrAdapterMissing` at facade resolve
/// time rather than as a codegen panic. This matches the proposal's
/// "additive, never break the build" discipline.
fn read_plugin_go_module(project_root: &Path, plugin_path: &str) -> Option<String> {
    let plugin_root = if Path::new(plugin_path).is_absolute() {
        std::path::PathBuf::from(plugin_path)
    } else {
        project_root.join(plugin_path)
    };
    let go_mod = plugin_root.join("go.mod");
    let contents = std::fs::read_to_string(&go_mod).ok()?;
    for line in contents.lines().take(40) {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("module ") {
            // Trim trailing comments (`// indirect` style) and whitespace.
            let module = rest.split("//").next()?.trim();
            if !module.is_empty() {
                return Some(module.to_owned());
            }
        }
    }
    None
}

struct RuntimeDevReplace {
    go_mod: String,
    go_work: String,
}

fn detect_runtime_dev_replace(project_root: &Path, out_dir: &Path) -> Option<RuntimeDevReplace> {
    let project_abs = absolutize_project_root(project_root);
    let out_abs = absolutize_for_codegen(project_root, out_dir);
    for parent in out_abs.ancestors() {
        let runtime_dir = parent.join("runtime").join("go");
        let go_mod = runtime_dir.join("go.mod");
        let Ok(contents) = std::fs::read_to_string(&go_mod) else {
            continue;
        };
        if !contents
            .lines()
            .any(|line| line.trim() == "module lazuli.dev/runtime")
        {
            continue;
        }
        return Some(RuntimeDevReplace {
            go_mod: relative_path(&out_abs, &runtime_dir),
            go_work: relative_path(&project_abs, &runtime_dir),
        });
    }
    None
}

/// Derive the Go module name from the IR's `app.name` (kebab-cased,
/// per proposal §1.1). Falls back to `lazuli/app` when no manifest
/// surfaces a name.
fn default_go_module_name(module: &lazuli_ir::Module) -> String {
    let name = module
        .app
        .as_ref()
        .map(|app| app.name.as_str())
        .unwrap_or("app");
    format!("lazuli/{}", to_kebab_case(name))
}

/// OpenAPI bucket cycle — emit a changelog markdown from two inspect
/// JSON payloads.
/// Recursively collect every `.lzi` file under a package root, skipping
/// well-known noise directories (build output, vcs metadata, vendored
/// deps). Honors the Lazurite convention (`features/<name>/<name>.lzi`)
/// without requiring callers to enumerate features.
fn collect_package_lzi_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
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

fn collect_package_lzx_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
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

fn collect_lzx_experience_module(input: &Path) -> lazuli_ir::ExperienceModule {
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
struct LzxBundle {
    app: Option<lazuli_ir::AppManifest>,
    routes: Vec<lazuli_ir::AppRoute>,
    experiences: Vec<lazuli_ir::Experience>,
    surfaces: Vec<lazuli_ir::PlatformSurface>,
}

fn collect_lzx_bundle(input: &Path) -> LzxBundle {
    let mut files = Vec::new();
    if input.is_dir() {
        collect_package_lzx_files(input, &mut files);
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

// (router-w1's duplicate collect_package_lzx_files removed; original
// Result<()>-returning version at line ~3965 is canonical.)

fn playwright_fixture_config(
    project_root: &Path,
    manifest: Option<&lazurite_manifest::Manifest>,
) -> lazuli_codegen_ts::playwright::PlaywrightFixtureConfig {
    let Some(frontend) = manifest
        .and_then(|manifest| {
            manifest.frontends.values().find(|frontend| {
                matches!(
                    frontend.target,
                    lazurite_manifest::FrontendTarget::TanstackVite
                )
            })
        })
        .and_then(|frontend| frontend.source.as_deref())
    else {
        return lazuli_codegen_ts::playwright::PlaywrightFixtureConfig::without_helpers();
    };

    let helper_dir = project_root.join(frontend).join("e2e").join("helpers");
    let api = helper_dir.join("api.ts");
    let session = helper_dir.join("session.ts");
    if !api.is_file() || !session.is_file() {
        return lazuli_codegen_ts::playwright::PlaywrightFixtureConfig::without_helpers();
    }

    let from_dir = project_root.join("dist").join("ts-web").join("tests");
    let Some(api_import) = relative_ts_import(&from_dir, &api) else {
        return lazuli_codegen_ts::playwright::PlaywrightFixtureConfig::without_helpers();
    };
    let Some(session_import) = relative_ts_import(&from_dir, &session) else {
        return lazuli_codegen_ts::playwright::PlaywrightFixtureConfig::without_helpers();
    };

    let onboarding = helper_dir.join("onboarding-progress.ts");
    let (lifecycle_import, lifecycle_seeders) = if onboarding.is_file() {
        let contents = fs::read_to_string(&onboarding).unwrap_or_default();
        let import = relative_ts_import(&from_dir, &onboarding);
        let seeders = ["host", "traveler", "operator"]
            .into_iter()
            .filter_map(|role| {
                let function_name = format!("progress{}To", playwright_fixture_pascal_case(role));
                if contents.contains(&format!("function {function_name}"))
                    || contents.contains(&format!("function* {function_name}"))
                {
                    Some(lazuli_codegen_ts::playwright::LifecycleSeeder {
                        role: role.to_owned(),
                        function_name,
                    })
                } else {
                    None
                }
            })
            .collect();
        (import, seeders)
    } else {
        (None, Vec::new())
    };

    lazuli_codegen_ts::playwright::PlaywrightFixtureConfig {
        helpers: Some(
            lazuli_codegen_ts::playwright::PlaywrightFixtureHelperImports {
                api_import,
                session_import,
                lifecycle_import,
                lifecycle_seeders,
            },
        ),
    }
}

fn relative_ts_import(from_dir: &Path, target_file: &Path) -> Option<String> {
    let from = normalized_components(from_dir);
    let target = normalized_components(target_file);
    let mut common = 0usize;
    while common < from.len() && common < target.len() && from[common] == target[common] {
        common += 1;
    }
    if common == 0 {
        return None;
    }

    let mut parts = Vec::new();
    for _ in common..from.len() {
        parts.push("..".to_owned());
    }
    parts.extend(target[common..].iter().cloned());
    let mut import = parts.join("/");
    if let Some(stripped) = import.strip_suffix(".ts") {
        import = stripped.to_owned();
    } else if let Some(stripped) = import.strip_suffix(".tsx") {
        import = stripped.to_owned();
    }
    if !import.starts_with('.') {
        import = format!("./{import}");
    }
    Some(import)
}

fn normalized_components(path: &Path) -> Vec<String> {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy().replace('\\', "/"))
        .collect()
}

fn playwright_fixture_pascal_case(value: &str) -> String {
    let mut out = String::new();
    for word in value.split(|ch: char| ch == '_' || ch == '-' || ch == ' ') {
        if word.is_empty() {
            continue;
        }
        let mut chars = word.chars();
        if let Some(first) = chars.next() {
            out.extend(first.to_uppercase());
            out.push_str(&chars.as_str().to_ascii_lowercase());
        }
    }
    out
}

fn build_module_with_source_from_path(
    input: &Path,
) -> Result<(
    lazuli_ir::Module,
    lazuli_ir::SourceMap,
    BTreeMap<String, lazuli_ir::FileId>,
)> {
    use lazuli_analyzer::source_map::SourceMapResolver;

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

/// Back-compat shim for callers that pre-date the W4.5 R2 split of
/// `lazuli generate go` into `commands/generate/go.rs`. `dev::regen`
/// and a handful of integration tests still spell the call as
/// `crate::generate_go(...)`; rather than touch those modules
/// (whose ownership lives outside W4.5 R2's scope), we keep this
/// single-line forward shim. Internal callers should use
/// `crate::commands::generate::go::generate_go` directly.
#[allow(clippy::too_many_arguments)]
pub(crate) fn generate_go(
    input: &Path,
    output: Option<&Path>,
    module: Option<&str>,
    lazuli_go_version: Option<&str>,
    check: bool,
    with_source: bool,
    allow_drops: bool,
) -> Result<()> {
    commands::generate::go::generate_go(
        input,
        output,
        module,
        lazuli_go_version,
        check,
        with_source,
        allow_drops,
    )
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
fn collect_plan_gate_facts_for_generate(
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

pub(crate) fn write_generated_file(root: &Path, relative: &str, contents: &str) -> Result<()> {
    let path = root.join(relative);

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory {}", parent.display()))?;
    }

    fs::write(&path, contents).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

pub(crate) fn write_go_work_preserving_entries(
    project_root: &Path,
    generated_contents: &str,
) -> Result<()> {
    let path = project_root.join("go.work");
    let required_entries = extract_go_work_use_entries(generated_contents);

    if !path.exists() {
        write_generated_file(project_root, "go.work", generated_contents)?;
        return Ok(());
    }

    let original =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    let updated = add_missing_go_work_use_entries(&original, &required_entries);
    fs::write(&path, updated).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

fn add_missing_go_work_use_entries(original: &str, required_entries: &[String]) -> String {
    let existing_entries = extract_go_work_use_entries(original);
    let missing_entries: Vec<&str> = required_entries
        .iter()
        .map(String::as_str)
        .filter(|entry| !existing_entries.iter().any(|existing| existing == entry))
        .collect();

    if missing_entries.is_empty() {
        return original.to_owned();
    }

    if let Some((close_idx, entry_indent)) = find_go_work_use_block_close(original) {
        let inserted = missing_entries
            .iter()
            .map(|entry| format!("{entry_indent}{entry}\n"))
            .collect::<String>();
        let (head, tail) = original.split_at(close_idx);
        return format!("{head}{inserted}{tail}");
    }

    let mut updated = original.trim_end().to_owned();
    updated.push_str("\n\nuse (\n");
    for entry in missing_entries {
        updated.push('\t');
        updated.push_str(entry);
        updated.push('\n');
    }
    updated.push_str(")\n");
    updated
}

fn extract_go_work_use_entries(contents: &str) -> Vec<String> {
    let mut entries = Vec::new();
    let mut in_use_block = false;

    for line in contents.lines() {
        let trimmed = line.trim();
        if in_use_block {
            if trimmed == ")" {
                in_use_block = false;
                continue;
            }
            if let Some(entry) = go_work_entry_from_line(trimmed) {
                entries.push(entry);
            }
            continue;
        }

        if trimmed == "use (" {
            in_use_block = true;
            continue;
        }

        if let Some(entry) = trimmed.strip_prefix("use ") {
            if entry.trim() != "(" {
                if let Some(entry) = go_work_entry_from_line(entry.trim()) {
                    entries.push(entry);
                }
            }
        }
    }

    entries
}

fn find_go_work_use_block_close(contents: &str) -> Option<(usize, String)> {
    let mut in_use_block = false;
    let mut entry_indent: Option<String> = None;
    let mut offset = 0;

    for line in contents.split_inclusive('\n') {
        let raw = line.trim_end_matches(['\r', '\n']);
        let trimmed = raw.trim();

        if in_use_block {
            if trimmed == ")" {
                return Some((offset, entry_indent.unwrap_or_else(|| "\t".to_owned())));
            }
            if entry_indent.is_none() && go_work_entry_from_line(trimmed).is_some() {
                entry_indent = Some(raw.chars().take_while(|c| c.is_whitespace()).collect());
            }
        } else if trimmed == "use (" {
            in_use_block = true;
        }

        offset += line.len();
    }

    None
}

fn go_work_entry_from_line(line: &str) -> Option<String> {
    let entry = line
        .split_once("//")
        .map_or(line, |(entry, _)| entry)
        .trim();
    if entry.is_empty() || entry.starts_with("//") {
        None
    } else {
        Some(entry.to_owned())
    }
}

#[cfg(test)]
mod tests;
