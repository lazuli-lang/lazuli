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
mod lazurite_codegen;
mod lazurite_manifest;
mod migrate;
mod module_loader;
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
// `commands::generate::go` reaches for these by short name; canonical
// home is `lazurite_codegen`.
pub(crate) use lazurite_codegen::{codegen_lazurite_manifest, default_go_module_name};
// `commands::generate::go`, `commands::generate::ts`, `dev::regen`,
// `commands::translate`, `commands::check`, `commands::parse`,
// `commands::inspect::symbol`, `doctor::schema_rich_001` reach for
// these by short name; canonical home is `module_loader`.
pub(crate) use module_loader::{
    LzxBundle, build_module_from_path, build_module_with_source_from_path, collect_lzx_bundle,
    collect_lzx_experience_module, collect_package_lzi_files, collect_package_lzx_files,
    collect_plan_gate_facts_for_generate, project_root_for_input, read_package_lzi_source,
};


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


/// OpenAPI bucket cycle — emit a changelog markdown from two inspect
/// JSON payloads.
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
