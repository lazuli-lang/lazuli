//! `lazuli_cli` — the command logic for the published `lazuli` binary AND the
//! framework-dev `lazuli-dev` binary.
//!
//! SPEC-20 2/n hoisted the whole command tree out of `main.rs` (a bin crate
//! root that a second binary cannot reuse) into this library crate, so both
//! `src/main.rs` (→ [`run`]) and `src/bin/lazuli-dev.rs` (→ [`run_dev`]) are
//! thin one-line shells over shared handlers + the `build_module_from_path`
//! compile pipeline. The published surface ([`cli_run`]) carries zero
//! framework-dev commands; those live in [`cli_dev`].

// Internal-tooling workspace: rustdoc cross-refs routinely point to
// `#[cfg(test)]` proof-tests and `pub(crate)` helpers (valid navigation under
// `--document-private-items`, but unresolvable to a public-API resolver). CI
// keeps `-D broken_intra_doc_links` on; this is the deliberate posture for these
// internal crates (genuine wrong refs are still fixed inline).
#![allow(rustdoc::broken_intra_doc_links, rustdoc::private_intra_doc_links)]
// Shim-first (Wave D3a): the manifest trio (`app_manifest`, `lazurite_manifest`,
// `plugin_manifest`) lives in the shared `lazuli_manifest` leaf crate. Re-export
// under the original crate-root paths so every `crate::app_manifest::Y` /
// `crate::lazurite_manifest::X` / `crate::plugin_manifest::Z` call site keeps
// resolving unchanged.
pub use lazuli_manifest::{app_manifest, lazurite_manifest, plugin_manifest};

mod casing;
mod cli_args;
mod cli_dev;
mod cli_run;
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
mod go_work_io;
pub mod inspect {
    pub mod expand_auth;
    pub mod expand_http;
    pub mod features_summary;
}
mod lazurite_codegen;
mod migrate;
pub mod module_loader;
mod path_utils;
mod playwright_fixture;
mod plugin_catalog;
pub mod plugin_semantic_resolver;
mod profile;
mod runners;
mod seed;
mod signature_aware_stub;
mod templates;
mod upgrade;
pub mod version;

// `doctor::schema_rich_001` reached for `crate::pascal_case`; preserve
// that surface plus expose the other casers crate-wide so the carved-out
// `commands::generate::ts::*` and `commands::new::*` modules can call
// them without per-module re-imports.
pub(crate) use casing::{lower_camel, pascal_case, to_kebab_case, to_snake_case};
// Sibling modules reach for these clap enums by short name
// (`crate::PlaywrightTarget`, `crate::InspectFormat`, …); re-export so
// the carve-out of `cli_args` is invisible from outside the dispatchers.
// `crate::tests::{dispatch,migrate}` parse `Cli` and match `Commands`
// directly, so those types live at the crate root too.
pub(crate) use cli_args::{
    Cli, Commands, DesignCommand, GenerateKind, InspectFormat, InspectInclude, MigrateCommand,
    PlaywrightTarget, TranslateCommand,
};
pub use cli_dev::run_dev;
// `crate::generate_go` back-compat shim (`dev::regen` + integration tests).
pub(crate) use cli_run::generate_go;
pub use cli_run::run;
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
// `crate::tests` and a few legacy paths reach for these by short name;
// the canonical home is `commands::new`.
pub(crate) use commands::new::{
    new_command,
    scaffold::{
        app_template, default_module_name, pascal_case_project_name, scaffold_bare,
        scaffold_from_template,
    },
};
// `commands::generate::ts`, `commands::generate::go`, `crate::tests`
// reach for these by short name; canonical home is `go_work_io`.
pub(crate) use go_work_io::{write_generated_file, write_go_work_preserving_entries};
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
pub(crate) use path_utils::{absolutize_for_codegen, absolutize_project_root, relative_path};
// `commands::generate::ts` reaches for `crate::playwright_fixture_config`;
// canonical home is `playwright_fixture`.
pub(crate) use playwright_fixture::playwright_fixture_config;

// Scaffold templates referenced crate-wide via `crate::{GITIGNORE_TEMPLATE,
// REGISTRY_TEMPLATE}` (commands::new::scaffold). Private root consts are
// visible to all descendant modules, so they stay unexported.
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

#[cfg(test)]
mod tests;
