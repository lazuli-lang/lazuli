use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use lazuli_lsp::SecurityProfile;
use serde::Serialize;
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

mod app_manifest;
mod casing;
mod cmd_design;
mod commands;
mod cmd_fix;
mod cmd_generate_command;
mod cmd_generate_feature;
mod cmd_generate_handler;
mod cmd_generate_playwright;
mod cmd_generate_rule;
mod cmd_generate_transition;
mod cmd_generate_view;
mod doctor_report;
mod doctor_watch;
mod cmd_mcp;
mod cmd_new_frontends;
mod cmd_test;
mod cmd_test_fail_fast;
mod cmd_test_ndjson;
mod cmd_test_output;
mod cmd_test_types;
mod cmd_test_watch;
mod coverage_aggregator;
mod debug;
mod dev;
mod doctor;
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
    command_schema_ident, command_zod_slots, emit_feature_barrel_ts,
    emit_feature_react_hooks_ts, emit_feature_sdk_ts, emit_feature_zod_ts, find_enum_decl,
    generate_ts, zod_base_for_type_ref,
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

#[derive(Debug, Parser)]
#[command(name = "lazuli", version = env!("CARGO_PKG_VERSION"))]
#[command(about = "Lazuli application metalinguage compiler")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
    /// Skip the Lazurite.toml [lazuli] runtime version pin check.
    ///
    /// Use when intentionally bumping mid-project; ship a follow-up commit
    /// updating Lazurite.toml to match the new pin.
    #[arg(long, global = true)]
    allow_version_mismatch: bool,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Parse {
        input: PathBuf,
    },
    Check {
        input: PathBuf,
        #[arg(long, value_enum, default_value_t = CheckSecurityProfile::Strict)]
        security_profile: CheckSecurityProfile,
    },
    Doctor {
        input: PathBuf,
        #[arg(long, value_enum, default_value_t = CheckSecurityProfile::Strict)]
        security_profile: CheckSecurityProfile,
        /// Run release-gate checks only.
        #[arg(long)]
        check_release: bool,
        /// Output format. `text` is the default human-readable
        /// rendering; `json` emits the canonical agent-first
        /// `DoctorReport` (Wave 2.0 schema, extended with Wave 6
        /// `coverage`).
        #[arg(long, default_value = "text")]
        format: String,
        /// Wave 6.4 — emit per-layer coverage report alongside
        /// diagnostics. Pure-IR layers
        /// (spec_predicate/spec_actor_matrix/spec_transition_state/view_extensibility)
        /// are always computed; handler_go and view_e2e_pair
        /// degrade gracefully when external inputs are absent.
        #[arg(long)]
        coverage: bool,
        /// Wave 2.2 + Wave 6.4 — composable threshold gate.
        /// Accepted forms:
        ///
        /// - `error` / `warning` / `info` — severity gate
        /// - `category:<C>` — rule-category gate (post-Wave 0.5)
        /// - `rule:<R>` — single-rule gate
        /// - `coverage:<layer>=<N>` — coverage threshold gate
        #[arg(long, action = clap::ArgAction::Append)]
        fail_on: Vec<String>,
        /// W3 (rails-style-refactor) — audit the framework's own Rust
        /// source instead of (or in addition to) `.lzi`/`.lzx` IR.
        /// Walks `crates/lazuli_*/src/` and emits `INTERNAL-*` findings
        /// (file size, missing rustdoc, absent `## Examples`, unpaired
        /// tests). Pairs with workspace-root
        /// `[doctor.internal_hygiene].preset = "tdd-iron-hand"` for
        /// the framework's CI editorial veto.
        #[arg(long = "self")]
        self_audit: bool,
    },
    Inspect {
        input: PathBuf,
        #[arg(long, default_value = "none")]
        expand: String,
        #[arg(long, value_enum, value_delimiter = ',')]
        include: Vec<InspectInclude>,
        #[arg(long, value_enum, default_value_t = InspectFormat::Json)]
        format: InspectFormat,
    },
    /// Reads a typed error envelope and emits a compact AI debug bundle.
    Debug {
        /// Path to error envelope JSON. If absent, reads from stdin.
        #[arg(long)]
        error: Option<PathBuf>,
        /// Capsule name; overrides envelope.capsule when present.
        #[arg(long)]
        capsule: Option<String>,
        /// Project root. Defaults to the current directory.
        #[arg(long, default_value = ".")]
        project: PathBuf,
        /// Output format: json or markdown.
        #[arg(long, default_value = "json")]
        format: String,
    },
    /// Read a pprof profile and report top-N ops by .lzi semantics.
    Profile {
        profile: PathBuf,
        #[arg(long, default_value = "10")]
        top: usize,
        #[arg(long, default_value = "cpu")]
        by: String,
        #[arg(long, default_value = "text")]
        format: String,
    },
    /// Curated examples for AI authoring and CI validation.
    Examples {
        #[command(subcommand)]
        sub: ExamplesCommand,
    },
    Init {
        path: PathBuf,
    },
    /// Create a new Lazuli project scaffold.
    New {
        /// Project directory to create.
        project_name: Option<PathBuf>,
        /// Template to scaffold. Supports `default` and `bare`.
        #[arg(long, default_value = "default")]
        template: String,
        /// Use the minimal bare template.
        #[arg(long)]
        bare: bool,
        /// Skip git initialization and initial commit.
        #[arg(long)]
        no_git: bool,
        /// Go module path to write into template files.
        #[arg(long)]
        module: Option<String>,
        /// Frontend skeletons to add: `web`, `mobile`, or `web,mobile`.
        #[arg(long)]
        frontends: Option<String>,
        /// Add frontend scaffold to an existing Lazurite project.
        #[arg(long)]
        in_place: bool,
    },
    Lsp {
        /// Accepted (and ignored) for compatibility with
        /// `vscode-languageclient`, which appends `--stdio` to the
        /// server command when `TransportKind.stdio` is used. The
        /// Lazuli LSP only supports stdio, so the flag is a no-op.
        #[arg(long, hide = true)]
        stdio: bool,
    },
    /// Regenerate the runtime-form `customer.gen.go` and `customer.gen.ts`
    /// files from a runtime spec. Without `--spec`, uses the in-process
    /// `customer_spike()` fixture; with `--spec <path>`, loads a JSON
    /// `RuntimeFeature` manifest (see `examples/runtime-spec/customer.json`).
    SpikeGenerate {
        /// Workspace root (defaults to the current directory). The
        /// command writes to `<root>/dist/go/customer/customer.gen.go`
        /// and `<root>/dist/web/customer/src/customer.gen.ts`.
        #[arg(long, short, default_value = ".")]
        root: PathBuf,
        /// Optional JSON path to a serialised `RuntimeFeature`. The CLI
        /// reads it via serde and runs the same emitter as the in-process
        /// fixture, decoupling codegen from the hardcoded spec.
        #[arg(long)]
        spec: Option<PathBuf>,
    },
    /// Migrations bucket cycle Route C — schema-migration planning
    /// surface. The current implementation validates checkpoint
    /// integrity (`--check <name>`); typed field-level diff lands
    /// in the Tier 4 follow-up cycle.
    Plan {
        /// Path to `app.lzi` (or a directory containing it).
        input: PathBuf,
        /// `--check <name>` validates the named `deploy.checkpoint`'s
        /// path exists and snapshot version matches the analyzer.
        #[arg(long = "check")]
        check: Option<String>,
    },
    /// OpenAPI / Lazuli Go / Lazurite feature cycle — emit artifacts
    /// derived from the typed IR slice. Today supports `openapi`
    /// (OpenAPI 3.1 spec YAML), `go` (Lazuli Go user-code that imports
    /// `lazuli.dev/runtime/lazuli`).
    Generate {
        /// Which artifact to emit. Closed catalog: `openapi`, `go`, `feature`.
        #[arg(value_enum)]
        kind: GenerateKind,
        /// Path to a `.lzi` file or directory; for `feature`, the feature name.
        input: PathBuf,
        /// Output file path (for `openapi`) or directory (for `go`).
        /// When omitted, `openapi` writes to stdout; `go` requires
        /// `--out` because it produces multiple files.
        #[arg(long, short, alias = "out")]
        output: Option<PathBuf>,
        /// API version string emitted as `info.version` (openapi only).
        /// Defaults to "0.0.0" when not provided.
        #[arg(long)]
        api_version: Option<String>,
        /// Go module path emitted in root `go.mod` (go only). Defaults
        /// to `lazuli/<app-name-kebab>` derived from `app.name`, or
        /// `lazuli/app` when no manifest is present.
        #[arg(long)]
        module: Option<String>,
        /// Version constraint emitted in `require lazuli.dev/runtime/lazuli`
        /// (go only). Defaults to the crate-pinned
        /// `lazuli_codegen_go::LAZULI_GO_VERSION` constant.
        #[arg(long)]
        lazuli_go_version: Option<String>,
        /// Smoke-run the emitter without writing any file. Surfaces
        /// unresolved references and exits non-zero on any error.
        /// Mirrors `translate extract --check`.
        #[arg(long)]
        check: bool,
        /// Emit source-map sidecar data and Go //line directives.
        #[arg(long)]
        with_source: bool,
        /// Allow the ALTER migration emitter (cell A11) to emit live
        /// `DROP COLUMN` SQL when the diff drops a column. Without this
        /// flag, `lazuli generate go` emits the DROPs as commented-out
        /// lines under a WARNING header so authors review them before
        /// pushing — adding a destructive ALTER to production by accident
        /// is otherwise too easy. `--allow-drops` is a NO-OP for the
        /// initial `CREATE TABLE` emission; it gates the per-resource
        /// `<NNN+1>_<feature>_<resource>_alter.sql` follow-ups only.
        /// (go only)
        #[arg(long)]
        allow_drops: bool,
        /// Playwright emit target (only used when kind == Playwright).
        /// Closed catalog: api-policy, lifecycle-gate, scalar-fixtures-barrel, all.
        #[arg(long, value_enum)]
        playwright_target: Option<PlaywrightTarget>,
    },
    /// Watch Lazuli source files, regenerate Go output, and optionally run it.
    Dev {
        /// Path to a `.lzi` file or a directory containing one.
        path: PathBuf,
        /// Output directory for generated Go files. Relative paths are
        /// resolved under `<path>` when `<path>` is a directory, or its
        /// parent when `<path>` is a file.
        #[arg(long, default_value = "dist/go")]
        out: PathBuf,
        /// Watch and regenerate only; do not run the generated Go server.
        #[arg(long)]
        no_run: bool,
        /// Debounce window in milliseconds.
        #[arg(long, default_value_t = 300)]
        debounce: u64,
    },
    /// Apply, roll back, or inspect SQL migrations from Lazurite.toml.
    Migrate {
        #[command(subcommand)]
        sub: MigrateCommand,
    },
    /// Import, export, or diff Lazuli design-token catalogs.
    Design {
        #[command(subcommand)]
        sub: DesignCommand,
    },
    /// Apply Lazuli authoring migration recipes.
    Upgrade {
        #[arg(long)]
        from: String,
        #[arg(long)]
        to: String,
        /// Project root with .lzi files to upgrade.
        target: PathBuf,
        /// Show what would happen without applying.
        #[arg(long)]
        dry_run: bool,
    },
    /// Run seed scripts from Lazurite.toml [seeds].dir.
    Seed {
        /// Run only one seed file by filename.
        #[arg(long)]
        only: Option<String>,
        /// Allow seeding when LAZULI_ENV=production.
        #[arg(long)]
        force: bool,
    },
    /// OpenAPI bucket cycle — diff two `lazuli inspect --format=json`
    /// payloads and emit a markdown changelog covering added / removed /
    /// deprecated / breaking / non-breaking operations.
    Changelog {
        /// Path to the baseline inspect JSON (typically `--from <rev>`).
        #[arg(long)]
        from: PathBuf,
        /// Path to the new inspect JSON (typically `--to <rev>`).
        #[arg(long)]
        to: PathBuf,
        /// Output file path. When omitted, the report is written to stdout.
        #[arg(long, short)]
        output: Option<PathBuf>,
    },
    /// i18n bucket cycle — translation toolbox. Today supports
    /// `extract`: walks the package for translatable surfaces (rule
    /// `message @translation.<key>` references, notification templates
    /// with `<locale>` placeholder, authored `translation` keys) and
    /// writes per-locale catalog stubs.
    Translate {
        #[command(subcommand)]
        sub: TranslateCommand,
    },
    /// Run an MCP (Model Context Protocol) server over stdio,
    /// exposing Lazuli's introspection surface to AI agents.
    ///
    /// Closed catalog of 8 read-only tools + 4 resource prefixes.
    /// See `docs/proposals/lazuli-mcp-subcommand-2026-05-17.md`.
    Mcp,
    /// Unified test runner across all layers (spec, view, handler,
    /// ts, e2e). Wire-thin: shells out to native runners (`go test`,
    /// Playwright, Vitest/Jest); does not reimplement execution.
    ///
    /// See `docs/proposals/lazuli-test-runner-2026-05-24.md`.
    Test {
        /// Path to a `.lzi` file or a directory containing one.
        /// Defaults to the current directory.
        #[arg(default_value = ".")]
        input: PathBuf,
        /// Layer(s) to run. Repeat for multiple (`--layer spec
        /// --layer handler`). Omit to run conventional discovery.
        #[arg(long = "layer", value_enum)]
        layer: Vec<TestLayerFlag>,
        /// Output format: `text` (default for TTY), `json`, `ndjson`.
        #[arg(long, default_value = "text")]
        format: String,
        /// Emit per-layer coverage report.
        #[arg(long)]
        coverage: bool,
        /// `--fail-on <spec>`. Repeatable. Supports
        /// `category:<Name>` and `coverage:<metric>=<pct>`.
        #[arg(long = "fail-on")]
        fail_on: Vec<String>,
        /// Watch source files and re-run affected layers on change.
        #[arg(long)]
        watch: bool,
        /// Stop after the first failing layer (skip downstream).
        #[arg(long = "fail-fast")]
        fail_fast: bool,
        /// Aggregate method (`weighted-by-construct-count`,
        /// `arithmetic-mean`, `min-of-layers`). Required when
        /// `--fail-on coverage:aggregate=...` is set.
        #[arg(long = "aggregate-method")]
        aggregate_method: Option<String>,
        /// Pass-through args after `--`. Forwarded to the layer's
        /// native runner (today: handler only).
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        extra_args: Vec<String>,
    },
}

/// CLI ValueEnum mirror of `cmd_test_types::Layer` — kept distinct so
/// the test-runner types stay free of `clap` dependency.
#[derive(Debug, Clone, Copy, ValueEnum)]
enum TestLayerFlag {
    Spec,
    View,
    Handler,
    Ts,
    E2e,
}

impl From<TestLayerFlag> for cmd_test_types::Layer {
    fn from(flag: TestLayerFlag) -> Self {
        match flag {
            TestLayerFlag::Spec => cmd_test_types::Layer::Spec,
            TestLayerFlag::View => cmd_test_types::Layer::View,
            TestLayerFlag::Handler => cmd_test_types::Layer::Handler,
            TestLayerFlag::Ts => cmd_test_types::Layer::Ts,
            TestLayerFlag::E2e => cmd_test_types::Layer::E2e,
        }
    }
}

#[derive(Debug, clap::Subcommand)]
enum TranslateCommand {
    /// Extract translatable keys to catalog stub files.
    Extract {
        /// Path to a `.lzi` file or a directory containing one.
        input: PathBuf,
        /// Output directory for per-locale catalog files (default `./i18n`).
        #[arg(long, default_value = "./i18n")]
        out: PathBuf,
        /// Only extract one locale's catalog. Defaults to every
        /// `app.locale.supported` tag.
        #[arg(long)]
        locale: Option<String>,
        /// Fail the CLI if any referenced `@translation.<key>` does
        /// not resolve, or if any declared key is missing a variant
        /// for a supported locale. CI gate.
        #[arg(long)]
        check: bool,
    },
}

#[derive(Debug, clap::Subcommand)]
enum ExamplesCommand {
    /// Emit deterministic JSONL bundle of curated examples for AI file-load.
    Bundle {
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Validate that every curated example still compiles and matches frozen IR.
    Validate {
        /// Also check provenance.last_validated freshness (warn if > 90 days).
        #[arg(long)]
        check_decay: bool,
    },
}

#[derive(Debug, clap::Subcommand)]
enum MigrateCommand {
    /// Apply pending migrations.
    Up {
        /// Apply migrations up to and including this version.
        #[arg(long)]
        target: Option<String>,
        /// Skip confirmation prompt.
        #[arg(long)]
        yes: bool,
    },
    /// Roll back applied migrations.
    Down {
        /// Number of migrations to roll back.
        #[arg(long, default_value_t = 1)]
        steps: u32,
        /// Skip confirmation prompt.
        #[arg(long)]
        yes: bool,
    },
    /// Show current version and pending migrations.
    Status,
    /// Apply DSL recipes that rewrite `.lzi`/`.lzx` source between
    /// two Lazuli versions. Recipes live under
    /// `migrations/recipes/<from>-to-<to>/`.
    Dsl {
        /// Source version tag (e.g. `v0.11`).
        #[arg(long)]
        from: String,
        /// Target version tag (e.g. `v0.12`).
        #[arg(long)]
        to: String,
        /// Print the diff per file without writing.
        #[arg(long)]
        dry_run: bool,
        /// Project root containing `.lzi`/`.lzx` files. Defaults to
        /// the current directory.
        #[arg(long)]
        path: Option<PathBuf>,
    },
}

#[derive(Debug, clap::Subcommand)]
enum DesignCommand {
    /// Import an external design-token catalog into `design.lzi`.
    Import {
        #[arg(long)]
        from: PathBuf,
        #[arg(long, value_enum, default_value_t = DesignImportFormat::Figma)]
        format: DesignImportFormat,
        #[arg(long)]
        overwrite: bool,
    },
    /// Export `design.lzi` into an external design-token catalog.
    Export {
        #[arg(long, value_enum)]
        target: DesignExportTarget,
        #[arg(long)]
        out: PathBuf,
    },
    /// Diff `design.lzi` against an external design-token catalog.
    Diff {
        #[arg(long)]
        against: PathBuf,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum PlaywrightTarget {
    ApiPolicy,
    LifecycleGate,
    ScalarFixturesBarrel,
    All,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum GenerateKind {
    Openapi,
    Go,
    Feature,
    Handler,
    Ts,
    Playwright,
    // Wave 3 — scaffold authoring kinds. Each appends a new construct
    // to an existing feature `.lzi` (or `.lzx` for View) with a pre-
    // populated `tests` block + `@TODO authored:` markers so the
    // scaffold ships RED (per docs/proposals/tdd-bdd-first-2026-05-23.md
    // Wave 3 + TEST-STUB-001 sentinel).
    Command,
    View,
    Rule,
    Transition,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum DesignImportFormat {
    Figma,
    StyleDictionary,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum DesignExportTarget {
    Figma,
    StyleDictionary,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum InspectFormat {
    Json,
    Lazuli,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum InspectInclude {
    Manifest,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CheckSecurityProfile {
    Prototype,
    Strict,
    Production,
}

impl From<CheckSecurityProfile> for SecurityProfile {
    fn from(profile: CheckSecurityProfile) -> Self {
        match profile {
            CheckSecurityProfile::Prototype => SecurityProfile::Prototype,
            CheckSecurityProfile::Strict => SecurityProfile::Strict,
            CheckSecurityProfile::Production => SecurityProfile::Production,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct ExpandSet {
    refs: bool,
    summary: bool,
    locators: bool,
    dependencies: bool,
    security: bool,
    events: bool,
    targets: bool,
    policies: bool,
    tests: bool,
    defaults: bool,
    /// Cut A — `--expand=tools` produces the per-agent dispatch graph
    /// keyed by tool reference. Per the proposal's "Pass model" note
    /// (plan §7.2), this is the first expansion that explicitly opts
    /// into cross-feature resolution.
    tools: bool,
    /// Cut A.7 — `--expand=expose` produces a unified HTTP route table
    /// across every `api` block and every agent declaring
    /// `expose http`. Cross-feature path collisions surface via doctor;
    /// this projection is the inspect-side observable.
    expose: bool,
    /// Phase L — `--expand=auth` projects the per-feature `auth` block
    /// (identity / password / sessions / mfa / oauth) from the canonical
    /// IR. Without the flag the projection is omitted entirely; this
    /// keeps the default inspect output stable.
    auth: bool,
    /// Phase L Tier 2 — `--expand=storage` projects every typed
    /// `@cap.File(...)` site (resource fields + api outputs) with the
    /// parsed `max_size`/`accept`/`visibility`/`signed_ttl`. Cross-
    /// feature symmetry checks live in doctor (storage bucket cycle);
    /// this projection is the per-feature observable.
    storage: bool,
    /// Observability bucket cycle row 36 — `--expand=tracing` /
    /// `--expand=logging`. The `AppManifest.logging` /
    /// `AppManifest.tracing` blocks always serialize when populated;
    /// these labels mark the report as having intentionally surfaced
    /// the observability axis so consumers (LLM, CLI users, docs)
    /// know the projection is current.
    tracing: bool,
    logging: bool,
    /// Roadmap §1.2 — `--expand=http` surfaces a unified `http`
    /// projection covering the three app-level HTTP hygiene blocks
    /// (`cookie` / `proxy` / `limits`). Each present block is
    /// included with `origin` metadata. Without the flag the typed
    /// blocks still serialize on `app` (because `AppManifest` carries
    /// them), but the unified projection at the report root is
    /// omitted.
    http: bool,
    /// Phase L Tier 3 — `--expand=jobs` projects every lifted
    /// `ir::Job` (handler-backed + declarative) on the feature.
    /// Without the flag the projection is omitted; with the flag the
    /// feature carries a `jobs` array mirroring `InspectAgent`.
    jobs: bool,
    /// Phase L Tier 3 — `--expand=webhooks` projects every lifted
    /// `ir::Webhook` on the feature.
    webhooks: bool,
    /// Phase L Tier 3 — `--expand=event_groups` projects every
    /// `ir::EventGroup` on the feature (pattern + inheritance).
    event_groups: bool,
    /// Migrations bucket cycle Route C — `--expand=migrations` projects
    /// every lifted `ir::TenantMigration` per feature and the
    /// app-level `deploy.checkpoint` + expansion fields.
    migrations: bool,
    /// Notifications expanded bucket cycle — `--expand=notifications`
    /// opts into the typed `digest` / `throttle` projection from the
    /// lifted `ir::Notification` slice. The scalar notification
    /// fields surface in default inspect regardless; this flag adds
    /// the structured sub-blocks so they appear without `--expand=all`.
    notifications: bool,
    /// Cache bucket cycle (CL.C.3) — `--expand=caches` projects every
    /// feature-level `cache <name>` profile (lifted `ir::CacheProfile`)
    /// per feature. Sibling of `--expand=jobs`/`webhooks`/`notifications`.
    /// Inline (per-query) cache blocks remain visible on each query's
    /// `cache` slot regardless of this flag; this flag controls the
    /// dedicated profile array.
    caches: bool,
    /// Roadmap §1.11 — `--expand=webhook_events` projects the package
    /// registry's canonical outbound webhook event schemas.
    webhook_events: bool,
    /// CL.C.4 — `--expand=aggregates` projects every lifted
    /// `ir::Aggregate` declaration on the feature, including the
    /// `root` resource, the `contains` cluster, and any invariants
    /// (with predicate text + message). Roadmap §1.7.
    aggregates: bool,
    /// Phase L Tier 4b — `--expand=commands` projects every lifted
    /// `ir::Command` on the feature (route + input + policy + audit +
    /// approval + invalidates + external_calls + rate_limit +
    /// timeout/retry/idempotency). Retires the legacy text-pattern
    /// command surface; mirrors `jobs`/`webhooks` shape.
    commands: bool,
    /// Phase L Tier 4b — `--expand=apis` projects every lifted
    /// `ir::Api` on the feature (method + path + output + policy +
    /// handler + locale_negotiate). Accepts `api` or `apis` token.
    apis: bool,
    /// Phase L Tier 4c — `--expand=resources` projects every lifted
    /// `ir::Resource` on the feature (fields + retention + has_many
    /// + constraints + validate + previous_names). Mirrors `commands`.
    resources: bool,
    /// Phase L Tier 4d — `--expand=queries` projects every lifted
    /// `ir::Query` on the feature (`List`/`Lookup`/`Sql` variants).
    queries: bool,
    /// Phase L Tier 4d — `--expand=records` projects every lifted
    /// `ir::Record` on the feature (fields + discriminator_field).
    records: bool,
    /// IR Error-Vocab (Cell PARSE-1) — `--expand=errors` projects the
    /// lifted `ir::FeatureErrors` block on the feature (exposure
    /// defaults + 4xx/5xx field allowlists + per-code message
    /// overrides). Mirrors `commands`/`apis` shape. See
    /// `docs/proposals/ir-error-messages-vocab.md` §3.6.
    errors: bool,
}

impl ExpandSet {
    fn all() -> Self {
        Self {
            refs: true,
            summary: true,
            locators: true,
            dependencies: true,
            security: true,
            events: true,
            targets: true,
            policies: true,
            tests: true,
            defaults: true,
            tools: true,
            expose: true,
            auth: true,
            storage: true,
            tracing: true,
            logging: true,
            http: true,
            jobs: true,
            webhooks: true,
            event_groups: true,
            migrations: true,
            notifications: true,
            caches: true,
            webhook_events: true,
            aggregates: true,
            commands: true,
            apis: true,
            resources: true,
            queries: true,
            records: true,
            errors: true,
        }
    }

    fn any(self) -> bool {
        self.refs
            || self.summary
            || self.locators
            || self.dependencies
            || self.security
            || self.events
            || self.targets
            || self.policies
            || self.tests
            || self.defaults
            || self.tools
            || self.expose
            || self.auth
            || self.storage
            || self.tracing
            || self.logging
            || self.http
            || self.jobs
            || self.webhooks
            || self.event_groups
            || self.migrations
            || self.webhook_events
            || self.notifications
            || self.caches
            || self.aggregates
            || self.commands
            || self.apis
            || self.resources
            || self.queries
            || self.records
            || self.errors
    }

    fn labels(self) -> Vec<&'static str> {
        let mut labels = Vec::new();
        if self.refs {
            labels.push("refs");
        }
        if self.summary {
            labels.push("summary");
        }
        if self.locators {
            labels.push("locators");
        }
        if self.dependencies {
            labels.push("dependencies");
        }
        if self.security {
            labels.push("security");
        }
        if self.events {
            labels.push("events");
        }
        if self.targets {
            labels.push("targets");
        }
        if self.policies {
            labels.push("policies");
        }
        if self.tests {
            labels.push("tests");
        }
        if self.defaults {
            labels.push("defaults");
        }
        if self.tools {
            labels.push("tools");
        }
        if self.expose {
            labels.push("expose");
        }
        if self.auth {
            labels.push("auth");
        }
        if self.storage {
            labels.push("storage");
        }
        if self.tracing {
            labels.push("tracing");
        }
        if self.logging {
            labels.push("logging");
        }
        if self.http {
            labels.push("http");
        }
        if self.jobs {
            labels.push("jobs");
        }
        if self.webhooks {
            labels.push("webhooks");
        }
        if self.event_groups {
            labels.push("event_groups");
        }
        if self.migrations {
            labels.push("migrations");
        }
        if self.notifications {
            labels.push("notifications");
        }
        if self.caches {
            labels.push("caches");
        }
        if self.webhook_events {
            labels.push("webhook_events");
        }
        if self.aggregates {
            labels.push("aggregates");
        }
        if self.commands {
            labels.push("commands");
        }
        if self.apis {
            labels.push("apis");
        }
        if self.resources {
            labels.push("resources");
        }
        if self.queries {
            labels.push("queries");
        }
        if self.records {
            labels.push("records");
        }
        if self.errors {
            labels.push("errors");
        }
        labels
    }
}

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
        } => inspect_command(&input, &expand, format, &include),
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
        Commands::Plan { input, check } => {
            commands::plan::plan_command(&input, check.as_deref())
        }
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


impl From<DesignImportFormat> for cmd_design::ImportFormat {
    fn from(format: DesignImportFormat) -> Self {
        match format {
            DesignImportFormat::Figma => cmd_design::ImportFormat::Figma,
            DesignImportFormat::StyleDictionary => cmd_design::ImportFormat::StyleDictionary,
        }
    }
}

impl From<DesignExportTarget> for cmd_design::ExportTarget {
    fn from(target: DesignExportTarget) -> Self {
        match target {
            DesignExportTarget::Figma => cmd_design::ExportTarget::Figma,
            DesignExportTarget::StyleDictionary => cmd_design::ExportTarget::StyleDictionary,
        }
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
                eprintln!("lazuli: skipping {}: lzx parse failed: {:?}", path.display(), err);
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

fn read_package_lzi_source(dir: &Path) -> Result<String> {
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
            if let Ok(alias_map) = plugin_manifest::build_alias_map(manifest.as_ref(), &project_root) {
                plugin_semantic_resolver::apply_plugin_semantic_resolution(
                    &mut module,
                    &alias_map,
                );
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
                eprintln!("lazuli: skipping {}: read failed: {:?}", path.display(), err);
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
        helpers: Some(lazuli_codegen_ts::playwright::PlaywrightFixtureHelperImports {
            api_import,
            session_import,
            lifecycle_import,
            lifecycle_seeders,
        }),
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

/// Migrations bucket cycle Route C — `lazuli plan --check <name>`
/// validates the named `deploy.checkpoint`. Reads the snapshot path
/// relative to `app.lzi`, verifies the file is parseable JSON, and
/// reports a `lazuli_version` mismatch with the analyzer.
///
/// Exit codes:
///   0 — checkpoint resolves + parses + version matches.
///   non-zero — checkpoint name unknown / path missing / parse error.
///
/// Typed field-level diff (`Rename Customer.status -> Customer.lifecycle_status`)
/// is out of scope for Route C; lands in the Tier-4 follow-up cycle.
fn inspect_command(
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
    inspect::features_summary::render_features_summary(&features)
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
fn inspect_symbol_arg(input: &Path) -> Option<&str> {
    let s = input.to_str()?;
    if s.is_empty() || s == "." || s == ".." {
        return None;
    }
    if s.contains('/') || s.contains('\\') {
        return None;
    }
    if s.ends_with(".lzi") {
        return None;
    }
    if input.exists() {
        return None;
    }
    Some(s)
}

/// Symbol-mode dispatch: build the SymbolOriginIndex from the project root
/// and emit JSON for the requested symbol per §5.2 / §5.4.
fn inspect_symbol_command(symbol: &str, format: InspectFormat) -> Result<()> {
    let cwd = std::env::current_dir().context("failed to determine current directory")?;
    let project_root = inspect_symbol_project_root(&cwd);
    let module = build_module_from_path(&project_root)?;
    let source_map = lazuli_ir::SourceMap { files: Vec::new() };
    let index = lazuli_analyzer::build_symbol_origin_index(&module, &source_map);

    let output = inspect_symbol_lookup(symbol, &module, &index);
    match format {
        InspectFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&output)?);
        }
        InspectFormat::Lazuli => {
            println!("{}", render_inspect_symbol_lazuli(symbol, &output));
        }
    }
    Ok(())
}

/// Render a `lazuli inspect <symbol>` JSON result as compact
/// human-readable lines for terminal viewers (closes the
/// `--format=lazuli for symbol-mode` next-checklist item). The JSON
/// shape stays normative; this is a one-screen view that surfaces
/// the four facts a reader usually wants: kind + feature + path:line
/// + previous names (when present).
fn render_inspect_symbol_lazuli(symbol: &str, output: &serde_json::Value) -> String {
    if let Some(error) = output.get("error") {
        let code = error
            .get("code")
            .and_then(|v| v.as_str())
            .unwrap_or("ERROR");
        let message = error
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("(no message)");
        let mut lines = vec![format!("{code}: {message}")];
        if let Some(candidates) = error.get("candidates").and_then(|v| v.as_array()) {
            for c in candidates {
                if let Some(s) = c.as_str() {
                    lines.push(format!("  - {s}"));
                }
            }
        }
        return lines.join("\n");
    }

    let name = output
        .get("symbol")
        .and_then(|v| v.as_str())
        .unwrap_or(symbol);
    let feature = output
        .get("feature")
        .and_then(|v| v.as_str())
        .unwrap_or("?");
    let kind = output
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("symbol");
    let defined_in = output
        .get("defined_in")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let location = match (
        defined_in.get("file").and_then(|v| v.as_str()),
        defined_in.get("line").and_then(|v| v.as_u64()),
    ) {
        (Some(file), Some(line)) => format!("{file}:{line}"),
        (Some(file), None) => file.to_owned(),
        _ => match defined_in.get("source").and_then(|v| v.as_str()) {
            Some("builtin") => "builtin".to_owned(),
            _ => "?".to_owned(),
        },
    };

    let mut lines = vec![format!(
        "{name} ({kind}) — feature `{feature}`, defined in: {location}"
    )];

    if let Some(prev) = output.get("previous_names").and_then(|v| v.as_array()) {
        if !prev.is_empty() {
            let names: Vec<&str> = prev.iter().filter_map(|v| v.as_str()).collect();
            if !names.is_empty() {
                lines.push(format!("  previously: {}", names.join(", ")));
            }
        }
    }

    if let Some(imported) = output.get("imported_via").and_then(|v| v.as_object()) {
        if let Some(feat) = imported.get("feature").and_then(|v| v.as_str()) {
            // Optional line/file anchor for the `uses <feat>` clause.
            let uses_anchor = imported
                .get("uses_at")
                .and_then(|v| v.as_object())
                .and_then(|obj| {
                    let file = obj.get("file").and_then(|v| v.as_str());
                    let line = obj.get("line").and_then(|v| v.as_u64());
                    match (file, line) {
                        (Some(f), Some(l)) => Some(format!(" at {f}:{l}")),
                        (Some(f), None) => Some(format!(" at {f}")),
                        _ => None,
                    }
                })
                .unwrap_or_default();
            lines.push(format!("  imported via: uses {feat}{uses_anchor}"));
        }
    }

    lines.join("\n")
}

/// Find the project root by walking up from `start` for a directory that
/// contains `Lazurite.toml`. Falls back to `start` itself when no manifest
/// is found — `build_module_from_path` will still produce something useful
/// from a single-feature dir.
fn inspect_symbol_project_root(start: &Path) -> PathBuf {
    let mut cursor: Option<&Path> = Some(start);
    while let Some(dir) = cursor {
        if dir.join("Lazurite.toml").is_file() || dir.join("lazurite.toml").is_file() {
            return dir.to_path_buf();
        }
        cursor = dir.parent();
    }
    start.to_path_buf()
}

/// Resolve a symbol query against the index. Returns a `serde_json::Value`
/// matching the JSON shapes in `docs/proposals/lsp-symbol-origin.md` §5.2,
/// §5.4 (error shapes).
fn inspect_symbol_lookup(
    symbol: &str,
    module: &lazuli_ir::Module,
    index: &lazuli_ir::SymbolOriginIndex,
) -> serde_json::Value {
    // Step 1: parse the symbol into (qualifier, name).
    let (qualifier, name) = match symbol.split_once('.') {
        Some((q, n)) => (Some(q.to_owned()), n.to_owned()),
        None => (None, symbol.to_owned()),
    };

    // Step 2: find candidate keys in the index.
    let candidates: Vec<&str> = match &qualifier {
        Some(feature_or_alias) => {
            // Qualified: look up `<qualifier>.<name>` directly. The qualifier
            // is the FEATURE that contains the symbol, regardless of which
            // feature triggered the inspect (uses-clause resolution would
            // need an analyzer pass; out of scope for the bare lookup).
            let key = format!("{}.{}", feature_or_alias, name);
            if index.symbols.contains_key(&key) {
                vec![index
                    .symbols
                    .get_key_value(&key)
                    .map(|(k, _)| k.as_str())
                    .unwrap()]
            } else {
                Vec::new()
            }
        }
        None => {
            // Bare name: walk all symbols matching `*.<name>`.
            index
                .symbols
                .iter()
                .filter(|(_, origin)| origin.name == name)
                .map(|(k, _)| k.as_str())
                .collect()
        }
    };

    // Step 3: when the qualifier is provided AND the symbol is NOT
    // defined in the qualified feature itself, check whether the
    // qualified feature imports a feature that defines it. This is
    // the `imported_via: uses account` case from
    // `docs/proposals/lsp-symbol-origin.md` §5.2 — a feature can
    // re-export a type by `uses`-ing the feature that owns it. The
    // qualified key lookup at step 2 already returns the direct
    // match (e.g. `account.Gender`); here we additionally consider
    // the cross-feature `host.Gender → uses account` resolution.
    let imported_via = qualifier
        .as_ref()
        .and_then(|consumer| resolve_imported_via(consumer, &name, index));

    // Step 4: re-resolve candidates against the imported edge when
    // the direct qualified lookup yielded nothing.
    let candidates = if candidates.is_empty() {
        if let Some((owning_feature, _)) = imported_via.as_ref() {
            let key = format!("{}.{}", owning_feature, name);
            if index.symbols.contains_key(&key) {
                vec![key]
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        }
    } else {
        candidates.into_iter().map(|s| s.to_owned()).collect()
    };

    // Step 5: branch on candidate count.
    match candidates.len() {
        0 => inspect_symbol_not_found(&qualifier, &name, module, index),
        1 => inspect_symbol_found(&candidates[0], &qualifier, &name, index, imported_via.as_ref()),
        _ => inspect_symbol_ambiguous(
            &name,
            &candidates.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
        ),
    }
}

/// When the consumer feature `<consumer>` `uses <other>` and `<other>`
/// defines `<name>`, return `(other_feature, ImportEdge)` so the
/// caller can populate `imported_via` in the inspect output. Returns
/// `None` when the consumer doesn't import the owning feature.
fn resolve_imported_via(
    consumer: &str,
    name: &str,
    index: &lazuli_ir::SymbolOriginIndex,
) -> Option<(String, lazuli_ir::ImportEdge)> {
    let edges = index.imports.get(consumer)?;
    for edge in edges {
        let candidate_key = format!("{}.{}", edge.imported, name);
        if index.symbols.contains_key(&candidate_key) {
            return Some((edge.imported.clone(), edge.clone()));
        }
    }
    None
}

fn inspect_symbol_found(
    key: &str,
    qualifier: &Option<String>,
    name: &str,
    index: &lazuli_ir::SymbolOriginIndex,
    imported_via: Option<&(String, lazuli_ir::ImportEdge)>,
) -> serde_json::Value {
    let origin = index.symbols.get(key).expect("key exists by construction");
    let imported_via_json = match imported_via {
        Some((owning, edge)) => serde_json::json!({
            "feature": owning,
            "uses_at": match &edge.uses_at {
                lazuli_ir::SourceLocation::File { file, line, column } => serde_json::json!({
                    "source": "file",
                    "file": file,
                    "line": line,
                    "column": column,
                }),
                lazuli_ir::SourceLocation::Builtin => serde_json::json!({"source": "builtin"}),
            },
        }),
        None => serde_json::Value::Null,
    };
    serde_json::json!({
        "symbol": name,
        "feature": qualifier.clone().unwrap_or_else(|| origin.feature.clone()),
        "defined_in": {
            "source": match &origin.defined_at {
                lazuli_ir::SourceLocation::File { .. } => "file",
                lazuli_ir::SourceLocation::Builtin => "builtin",
            },
            "file": match &origin.defined_at {
                lazuli_ir::SourceLocation::File { file, .. } => Some(file.clone()),
                lazuli_ir::SourceLocation::Builtin => None,
            },
            "line": match &origin.defined_at {
                lazuli_ir::SourceLocation::File { line, .. } => Some(*line),
                lazuli_ir::SourceLocation::Builtin => None,
            },
            "column": match &origin.defined_at {
                lazuli_ir::SourceLocation::File { column, .. } => Some(*column),
                lazuli_ir::SourceLocation::Builtin => None,
            },
            "kind": symbol_kind_str(&origin.kind),
        },
        "imported_via": imported_via_json,
        "type": symbol_kind_str(&origin.kind),
        "previous_names": origin.previous_names,
    })
}

fn inspect_symbol_not_found(
    qualifier: &Option<String>,
    name: &str,
    _module: &lazuli_ir::Module,
    _index: &lazuli_ir::SymbolOriginIndex,
) -> serde_json::Value {
    let message = match qualifier {
        Some(q) => format!(
            "no declaration named `{}` in feature `{}` or any imported feature",
            name, q
        ),
        None => format!("no declaration named `{}` in any feature of this project", name),
    };
    serde_json::json!({
        "error": {
            "code": "SYMBOL_NOT_FOUND",
            "message": message,
        }
    })
}

fn inspect_symbol_ambiguous(name: &str, candidates: &[&str]) -> serde_json::Value {
    serde_json::json!({
        "error": {
            "code": "AMBIGUOUS_SYMBOL",
            "message": format!("`{}` is declared in multiple features; qualify the lookup as `<feature>.{}`", name, name),
            "candidates": candidates,
        }
    })
}

fn symbol_kind_str(kind: &lazuli_ir::SymbolKind) -> &'static str {
    match kind {
        lazuli_ir::SymbolKind::Enum => "enum",
        lazuli_ir::SymbolKind::Resource => "resource",
        lazuli_ir::SymbolKind::Record => "record",
        lazuli_ir::SymbolKind::Scalar => "scalar",
        lazuli_ir::SymbolKind::Semantic => "semantic",
        lazuli_ir::SymbolKind::Command => "command",
        lazuli_ir::SymbolKind::Query => "query",
        lazuli_ir::SymbolKind::Event => "event",
        lazuli_ir::SymbolKind::Aggregate => "aggregate",
    }
}

fn inspect_source_path(input: &Path) -> PathBuf {
    if input.is_dir() {
        return lazurite_manifest::resolve_in_app_dir(input, "app.lzi");
    }

    input.to_path_buf()
}

fn inspect_json_value(
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
    let entry = line.split_once("//").map_or(line, |(entry, _)| entry).trim();
    if entry.is_empty() || entry.starts_with("//") {
        None
    } else {
        Some(entry.to_owned())
    }
}

fn parse_expand_set(value: &str) -> Result<ExpandSet> {
    let mut set = ExpandSet::default();

    for raw_item in value.split(',') {
        let item = raw_item.trim();
        if item.is_empty() || item == "none" {
            continue;
        }

        if item == "all" {
            return Ok(ExpandSet::all());
        }

        match item {
            "refs" => set.refs = true,
            "summary" => set.summary = true,
            "locators" => set.locators = true,
            "dependencies" => set.dependencies = true,
            "security" => set.security = true,
            "events" => set.events = true,
            "targets" => set.targets = true,
            "policies" => set.policies = true,
            "tests" => set.tests = true,
            "defaults" => set.defaults = true,
            "tools" => set.tools = true,
            "expose" => set.expose = true,
            "auth" => set.auth = true,
            "storage" => set.storage = true,
            "tracing" => set.tracing = true,
            "logging" => set.logging = true,
            "http" => set.http = true,
            "jobs" => set.jobs = true,
            "webhooks" => set.webhooks = true,
            "event_groups" => set.event_groups = true,
            "webhook_events" => set.webhook_events = true,
            // Migrations bucket cycle Route C — projects every lifted
            // `ir::TenantMigration` on the feature + the app deploy
            // block's checkpoint/strategy/lock_timeout/hook fields.
            "migrations" | "tenant_migrations" => set.migrations = true,
            // Notifications expanded bucket cycle — projects every
            // lifted `ir::Notification` with typed `digest` /
            // `throttle` sub-blocks. The scalar fields surface in
            // default inspect; this flag adds the structured shapes.
            "notifications" => set.notifications = true,
            // Cache bucket cycle (CL.C.3) — projects every lifted
            // `ir::CacheProfile` on the feature. Inline (per-query)
            // cache slots remain visible regardless of this flag;
            // this flag controls the dedicated profile array.
            "caches" => set.caches = true,
            // CL.C.4 — projects every lifted `ir::Aggregate` on the
            // feature (root + contains + invariants). Roadmap §1.7.
            "aggregates" => set.aggregates = true,
            // Phase L Tier 4b — projects every lifted `ir::Command` on
            // the feature (route + input + policy + audit + approval
            // + invalidates + external_calls + rate_limit +
            // timeout/retry/idempotency). Mirrors `jobs`/`webhooks`.
            "commands" => set.commands = true,
            // Phase L Tier 4b — projects every lifted `ir::Api` on the
            // feature (method + path + output + policy + handler +
            // locale_negotiate). Accepts both singular and plural to
            // mirror `migrations`/`tenant_migrations`.
            "api" | "apis" => set.apis = true,
            // Phase L Tier 4c — projects every lifted `ir::Resource`
            // on the feature (fields + retention + has_many +
            // constraints + validate + previous_names).
            "resources" => set.resources = true,
            // Phase L Tier 4d — projects every lifted `ir::Query`
            // on the feature (List / Lookup / Sql variants).
            "queries" => set.queries = true,
            // Phase L Tier 4d — projects every lifted `ir::Record`
            // on the feature (fields + discriminator_field).
            "records" => set.records = true,
            // IR Error-Vocab (Cell PARSE-1) — projects the lifted
            // `ir::FeatureErrors` block (exposure defaults + 4xx/5xx
            // field allowlists + per-code message overrides).
            "errors" => set.errors = true,
            _ => bail!(
                "unknown inspect expansion `{item}`; use none, all, refs, summary, locators, dependencies, security, events, targets, policies, tests, defaults, tools, expose, auth, storage, tracing, logging, jobs, webhooks, event_groups, webhook_events, migrations, tenant_migrations, notifications, caches, aggregates, commands, api, apis, resources, queries, records, or errors"
            ),
        }
    }

    Ok(set)
}

#[derive(Debug, Serialize)]
struct InspectReport {
    schema: &'static str,
    source: String,
    expand: Vec<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    workspace: Option<lazuli_ir::AppWorkspace>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    contracts: Vec<lazuli_ir::AppContract>,
    #[serde(skip_serializing_if = "Option::is_none")]
    app: Option<lazuli_ir::AppManifest>,
    #[serde(skip_serializing_if = "Option::is_none")]
    registry: Option<lazuli_ir::AppRegistry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    webhook_events: Option<Vec<lazuli_ir::WebhookEventRegistry>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    profiles: Vec<lazuli_ir::AppProfile>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    routes: Vec<lazuli_ir::AppRoute>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    experiences: Vec<lazuli_ir::Experience>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    surfaces: Vec<lazuli_ir::PlatformSurface>,
    /// Roadmap §1.2 — populated only when `--expand=http` is set. The
    /// unified HTTP hygiene projection covers the three app-level
    /// blocks (`cookie` / `proxy` / `limits`) with `origin` metadata.
    /// `None` when the flag is off or when no block is populated.
    #[serde(skip_serializing_if = "Option::is_none")]
    http: Option<serde_json::Value>,
    features: Vec<InspectFeature>,
}

#[derive(Debug, Serialize)]
struct InspectFeature {
    name: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    requirements: Vec<InspectRequirement>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    external_calls: Vec<InspectExternalCall>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    agents: Vec<InspectAgent>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    notifications: Vec<InspectNotification>,
    #[serde(skip_serializing_if = "Option::is_none")]
    refs: Option<InspectRefs>,
    #[serde(skip_serializing_if = "Option::is_none")]
    summary: Option<InspectSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    locators: Option<Vec<InspectLocators>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    dependencies: Option<Vec<InspectDependency>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    security: Option<InspectSecurity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    defaults: Option<Vec<InspectDefault>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    events: Option<Vec<InspectEvent>>,
    /// Cut A.8 — built-in trace events surfaced alongside the authored
    /// `events` when `--expand=events` is set. Today only `agent_run`;
    /// the slot exists so a future cut adding `job_run`/`webhook_run`
    /// surfaces them without an additional flag.
    #[serde(skip_serializing_if = "Option::is_none")]
    built_in_trace_events: Option<Vec<InspectBuiltInTraceEvent>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    targets: Option<Vec<InspectTarget>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    policies: Option<Vec<InspectPolicy>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tests: Option<Vec<InspectTests>>,
    /// Cut A — populated only when `--expand=tools` is set. The
    /// dispatch graph keyed by agent + tool reference; doctor-level
    /// resolution of cross-feature targets is referenced via
    /// `resolution`, while structural facts come from the file alone
    /// (preserves the single-pass-base guarantee).
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<InspectAgentToolsEntry>>,
    /// Cut A.7 — populated only when `--expand=expose` is set. Unified
    /// HTTP route table for the feature: every `api` block plus every
    /// agent declaring `expose http`. Cross-feature collisions surface
    /// via doctor; this projection is the per-feature observable.
    #[serde(skip_serializing_if = "Option::is_none")]
    expose: Option<Vec<InspectExposeEntry>>,
    /// Phase L — populated only when `--expand=auth` is set. Lowered
    /// `auth` block from the canonical-indent slice. `None` when the
    /// feature declares no `auth`; cross-feature checks (e.g. unique
    /// identity per workspace) live in doctor.
    #[serde(skip_serializing_if = "Option::is_none")]
    auth: Option<InspectAuth>,
    /// Phase L Tier 2 — populated only when `--expand=storage` is set.
    /// Every typed `@cap.File(...)` site in the feature: resource fields
    /// and api outputs. Omitted entirely when no `@cap.File` is authored.
    #[serde(skip_serializing_if = "Option::is_none")]
    storage: Option<InspectStorage>,
    /// Phase L Tier 3 — populated only when `--expand=jobs` is set.
    /// Every lifted `ir::Job` on the feature. Mirrors `InspectAgent`'s
    /// shape (one struct per job) so an LLM can read it cold without
    /// joining tables.
    #[serde(skip_serializing_if = "Option::is_none")]
    jobs: Option<Vec<InspectJob>>,
    /// Phase L Tier 3 — populated only when `--expand=webhooks` is
    /// set. Every lifted `ir::Webhook` on the feature.
    #[serde(skip_serializing_if = "Option::is_none")]
    webhooks: Option<Vec<InspectWebhook>>,
    /// Phase L Tier 3 — populated only when `--expand=event_groups`
    /// is set. Every lifted `ir::EventGroup` on the feature.
    #[serde(skip_serializing_if = "Option::is_none")]
    event_groups: Option<Vec<InspectEventGroup>>,
    /// Migrations bucket cycle Route C — populated only when
    /// `--expand=migrations` is set. Every lifted
    /// `ir::TenantMigration` on the feature.
    #[serde(skip_serializing_if = "Option::is_none")]
    tenant_migrations: Option<Vec<lazuli_ir::TenantMigration>>,
    /// Cache bucket cycle (CL.C.3) — populated only when
    /// `--expand=caches` is set. Every lifted feature-level
    /// `cache <name>` profile (`ir::CacheProfile`) on the feature.
    /// Inline (per-query) cache slots are projected on each query's
    /// `cache` field regardless of this flag.
    #[serde(skip_serializing_if = "Option::is_none")]
    caches: Option<Vec<lazuli_ir::CacheProfile>>,
    /// CL.C.4 — populated only when `--expand=aggregates` is set.
    /// Every lifted `ir::Aggregate` on the feature. Roadmap §1.7.
    #[serde(skip_serializing_if = "Option::is_none")]
    aggregates: Option<Vec<InspectAggregate>>,
    /// Phase L Tier 4b — populated only when `--expand=commands` is set.
    /// Every lifted `ir::Command` on the feature, serialized verbatim
    /// from IR so the projection stays in lockstep with the lowered
    /// shape. Cross-feature checks (audit emit_to, policy resolution,
    /// rate-limit shape) surface via doctor; this projection is the
    /// per-feature observable.
    #[serde(skip_serializing_if = "Option::is_none")]
    commands: Option<Vec<lazuli_ir::Command>>,
    /// Phase L Tier 4b — populated only when `--expand=apis` (or
    /// `--expand=api`) is set. Every lifted `ir::Api` on the feature.
    /// Cross-feature path collision lives in doctor
    /// (`agent_expose_path_conflict_cross_feature_diagnostics`); this
    /// projection is the per-feature observable.
    #[serde(skip_serializing_if = "Option::is_none")]
    apis: Option<Vec<lazuli_ir::Api>>,
    /// Phase L Tier 4c — populated only when `--expand=resources` is
    /// set. Every lifted `ir::Resource` on the feature, serialized
    /// verbatim (fields with typed capability + semantic + pii
    /// decorators, `retention`, `has_many`, `constraints`,
    /// `validate`, `previous_names`).
    #[serde(skip_serializing_if = "Option::is_none")]
    resources: Option<Vec<lazuli_ir::Resource>>,
    /// Phase L Tier 4d — populated only when `--expand=queries` is
    /// set. Every lifted `ir::Query` on the feature (`List`/`Lookup`/
    /// `Sql` variants, each with their full v0 child coverage).
    #[serde(skip_serializing_if = "Option::is_none")]
    queries: Option<Vec<lazuli_ir::Query>>,
    /// Phase L Tier 4d — populated only when `--expand=records` is
    /// set. Every lifted `ir::Record` on the feature (fields +
    /// optional discriminator_field marker).
    #[serde(skip_serializing_if = "Option::is_none")]
    records: Option<Vec<lazuli_ir::Record>>,
    /// IR Error-Vocab (Cell PARSE-1) — populated only when
    /// `--expand=errors` is set. The lifted feature-level `errors`
    /// block (`ir::FeatureErrors`): exposure defaults, 4xx/5xx field
    /// allowlists, and per-code message overrides. `None` when the
    /// feature declares no `errors` block; `Some(default)` (with all
    /// vectors empty) when the block exists but has no overrides.
    #[serde(skip_serializing_if = "Option::is_none")]
    errors: Option<lazuli_ir::FeatureErrors>,
}

#[derive(Debug, Serialize)]
struct InspectStorage {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    fields: Vec<InspectStorageField>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    api_outputs: Vec<InspectStorageApiOutput>,
}

#[derive(Debug, Serialize)]
struct InspectStorageField {
    resource: String,
    field: String,
    file_capability: InspectFileCapability,
}

#[derive(Debug, Serialize)]
struct InspectStorageApiOutput {
    api: String,
    file_capability: InspectFileCapability,
}

#[derive(Debug, Serialize)]
struct InspectFileCapability {
    max_size: InspectFileSize,
    accept: Vec<InspectMimeType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    visibility: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    signed_ttl: Option<String>,
}

#[derive(Debug, Serialize)]
struct InspectFileSize {
    bytes: u64,
    literal: String,
}

#[derive(Debug, Serialize)]
struct InspectMimeType {
    family: String,
    subtype: String,
}

#[derive(Debug, Serialize)]
struct InspectAuth {
    origin: InspectOrigin,
    identity: InspectAuthIdentity,
    #[serde(skip_serializing_if = "Option::is_none")]
    password: Option<InspectAuthPassword>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sessions: Option<InspectAuthSessions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mfa: Option<InspectAuthMfa>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    oauth: Vec<InspectAuthOAuthProvider>,
}

#[derive(Debug, Serialize)]
struct InspectAuthIdentity {
    /// `<Resource>.<field>` joined back together so downstream consumers
    /// don't need to reassemble it.
    field: String,
    resource: String,
    origin: InspectOrigin,
}

#[derive(Debug, Serialize)]
struct InspectAuthPassword {
    algorithm: String,
    hash: String,
    verify: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    rate_limit: Option<String>,
    origin: InspectOrigin,
}

#[derive(Debug, Serialize)]
struct InspectAuthSessions {
    resource: String,
    ttl: String,
    refresh: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    access_ttl: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    rotation: Option<lazuli_ir::RotationConfig>,
    origin: InspectOrigin,
}

#[derive(Debug, Serialize)]
struct InspectAuthMfa {
    method: String,
    enroll: String,
    verify: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    adapter: Option<String>,
    origin: InspectOrigin,
}

#[derive(Debug, Serialize)]
struct InspectAuthOAuthProvider {
    provider: String,
    adapter: String,
    origin: InspectOrigin,
}

#[derive(Debug, Serialize, Clone)]
struct InspectOrigin {
    feature: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    line: Option<usize>,
}

#[derive(Debug, Serialize)]
struct InspectBuiltInTraceEvent {
    name: String,
    fires_per: String,
    payload: Vec<InspectBuiltInTraceField>,
}

#[derive(Debug, Serialize)]
struct InspectBuiltInTraceField {
    name: String,
    #[serde(rename = "type")]
    type_text: String,
    optional: bool,
}

#[derive(Debug, Serialize)]
struct InspectExposeEntry {
    /// `agent` or `api` — the kind of declaration that produced the route.
    kind: &'static str,
    /// `<feature>.<kind>.<name>` for stable cross-references.
    origin: String,
    method: String,
    path: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    route_slots: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    audience: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    rate_limit_override: Option<String>,
}

/// One per agent in the file. Carries every tool reference the agent
/// dispatches plus the local categorisation (kind, scope). Cross-feature
/// resolution lives in doctor; the projection records the symbol shape
/// so consumers can compose either path.
#[derive(Debug, Serialize)]
struct InspectAgentToolsEntry {
    agent: String,
    tools: Vec<InspectAgentToolBinding>,
}

#[derive(Debug, Serialize)]
struct InspectAgentToolBinding {
    /// Canonical reference exactly as the author wrote it.
    reference: String,
    /// Local-categorisation of the reference: `query.list`, `query.lookup`,
    /// `query.sql`, `query`, `command`, `api`, `adapter`. Cross-feature
    /// resolution narrows `query` to one of the three subkinds.
    kind: &'static str,
    /// `local`, `cross_feature`, or `adapter` — the resolution scope.
    scope: &'static str,
    /// `read` / `write` / `unknown`. Adapter references rely on the
    /// registry; local kinds map directly (`command` is always `write`,
    /// queries default to `read`).
    derived_effect: &'static str,
}

#[derive(Debug, Serialize)]
struct InspectRequirement {
    kind: String,
    name: String,
    contract: String,
    origin: &'static str,
}

#[derive(Debug, Serialize)]
struct InspectExternalCall {
    subject: String,
    slot: String,
    operation: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    args: Vec<InspectCallArg>,
    #[serde(skip_serializing_if = "Option::is_none")]
    timeout: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    retry: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    idempotency: Option<String>,
    audit: bool,
    origin: &'static str,
}

#[derive(Debug, Serialize)]
struct InspectCallArg {
    name: String,
    value: String,
}

#[derive(Debug, Serialize)]
struct InspectRefs {
    declared: Vec<InspectRefGroup>,
    used: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    missing: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    unused: Vec<String>,
}

#[derive(Debug, Serialize)]
struct InspectRefGroup {
    group: String,
    namespaces: Vec<String>,
    origin: &'static str,
}

#[derive(Debug, Serialize)]
struct InspectSummary {
    provides: InspectProvides,
    resources: Vec<String>,
    records: Vec<String>,
    queries: Vec<String>,
    commands: Vec<String>,
    workflows: Vec<InspectWorkflowSummary>,
    jobs: Vec<String>,
    webhooks: Vec<String>,
    events: Vec<String>,
    surfaces: Vec<String>,
    anchors: Vec<String>,
    extends: Vec<String>,
    extended_by: Vec<String>,
}

#[derive(Debug, Serialize)]
struct InspectProvides {
    types: Vec<String>,
    queries: Vec<String>,
    events: Vec<String>,
    anchors: Vec<String>,
}

#[derive(Debug, Serialize)]
struct InspectWorkflowSummary {
    name: String,
    transitions: Vec<String>,
}

#[derive(Debug, Serialize)]
struct InspectLocators {
    subject: String,
    kind: String,
    bindings: Vec<InspectBinding>,
}

#[derive(Debug, Serialize)]
struct InspectBinding {
    name: String,
    origin: String,
    meaning: String,
}

#[derive(Debug, Serialize)]
struct InspectDependency {
    kind: String,
    from: String,
    to: String,
    origin: String,
}

#[derive(Debug, Serialize)]
struct InspectSecurity {
    fields: Vec<InspectSecurityField>,
    event_payloads: Vec<InspectSecurityEventPayload>,
    operations: Vec<InspectSecurityOperation>,
    webhooks: Vec<InspectSecurityWebhook>,
}

#[derive(Debug, Serialize)]
struct InspectSecurityField {
    resource: String,
    field: String,
    markers: Vec<String>,
    origin: &'static str,
}

#[derive(Debug, Serialize)]
struct InspectSecurityEventPayload {
    event: String,
    field: String,
    markers: Vec<String>,
    origin: &'static str,
}

#[derive(Debug, Serialize)]
struct InspectSecurityOperation {
    subject: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    policy: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tenant_from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    scope_reason: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    rate_limits: Vec<String>,
    scope_override: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    audit: Option<InspectAudit>,
    origin: &'static str,
}

#[derive(Debug, Serialize)]
struct InspectAudit {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    fields: Vec<String>,
    /// Observability bucket cycle row 37 — `audit ... emit_to <X>`
    /// destination. `None` means "runtime falls back to the reserved
    /// `audit_log` stream".
    #[serde(skip_serializing_if = "Option::is_none")]
    emit_to: Option<String>,
    origin: &'static str,
}

#[derive(Debug, Serialize)]
struct InspectNotification {
    name: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    channels: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    recipient: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    trigger: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    template: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    policy: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tenant_from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    idempotency: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    retry: Option<String>,
    /// Scalar `rate_limit "N per <window>"` captured verbatim. Kept
    /// for forward-compat: the language reserves `rate_limit` as the
    /// per-call scalar slot across `agent`/`auth password`/`command`/
    /// `expose http` and may surface it on `notification` once pilot
    /// pressure requires it. Distinct from the structured `throttle`
    /// sub-block below.
    #[serde(skip_serializing_if = "Option::is_none")]
    rate_limit: Option<String>,
    /// Notifications expanded bucket cycle — typed projection of the
    /// `digest` sub-block (`every`/`group_by`/`max_size`/
    /// `template_strategy`). `None` when the notification does not
    /// declare digesting.
    #[serde(skip_serializing_if = "Option::is_none")]
    digest: Option<InspectNotificationDigest>,
    /// Notifications expanded bucket cycle — typed projection of the
    /// `throttle` sub-block (`max_per`/`per_recipient`/`per_channel`/
    /// `burst`). `None` when the notification does not declare a
    /// throttle bucket. Distinct from scalar `rate_limit`.
    #[serde(skip_serializing_if = "Option::is_none")]
    throttle: Option<InspectNotificationThrottle>,
    origin: &'static str,
}

/// Notifications expanded bucket cycle — `--expand=notifications`
/// projection of `ir::NotificationDigest`. Mirrors the IR shape one-
/// to-one so consumers can read the digest contract cold.
#[derive(Debug, Serialize)]
struct InspectNotificationDigest {
    every: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    group_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_size: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    template_strategy: Option<String>,
}

/// Notifications expanded bucket cycle — `--expand=notifications`
/// projection of `ir::NotificationThrottle`. Distinct shape from
/// scalar `rate_limit` so the structured per-recipient/per-channel
/// contract surfaces in JSON without being conflated with the scalar
/// slot above.
#[derive(Debug, Serialize)]
struct InspectNotificationThrottle {
    max_per: String,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    per_recipient: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    per_channel: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    burst: Option<u32>,
}

#[derive(Debug, Serialize)]
struct InspectAgent {
    name: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    inputs: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    context: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    policy: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    rate_limit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    output: Option<String>,
    /// Cut A — `text` / `stream` / `discriminated_enum` /
    /// `discriminated_record`. Derived from the `output` declaration.
    #[serde(skip_serializing_if = "Option::is_none")]
    output_kind: Option<&'static str>,
    /// Cut A — the enum or record name the discriminator points at,
    /// when `output_kind` resolves to a discriminated form.
    #[serde(skip_serializing_if = "Option::is_none")]
    output_discriminator: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    seed: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    prompt: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<String>,
    /// Cut A — eval `case <name>` headers under this agent.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    evals: Vec<String>,
    /// Cut A — `pinned` when both `temperature 0` and `seed <int>` are
    /// declared (cases gate CI); `nondeterministic` otherwise (cases
    /// run as informational).
    #[serde(skip_serializing_if = "Option::is_none")]
    eval_determinism: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    safety: Option<String>,
    /// Cut A.7 — `expose http` block summary. Always-on field
    /// (file-local; no cross-feature resolution).
    #[serde(skip_serializing_if = "Option::is_none")]
    expose_http: Option<InspectAgentExpose>,
    origin: &'static str,
}

#[derive(Debug, Serialize)]
struct InspectAgentExpose {
    method: String,
    path: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    route_slots: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    audience: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    rate_limit_override: Option<String>,
}

// -----------------------------------------------------------------------------
// Phase L Tier 3 — inspect projections for jobs / webhooks / event_groups.
//
// `--expand=jobs`, `--expand=webhooks`, and `--expand=event_groups` produce
// these per-feature arrays. The shape mirrors `InspectAgent` /
// `InspectNotification` so a consumer (LLM or human) can read the full
// `notification`/`job`/`webhook` triple cold without joining tables.
// Row 32 of `docs/next-checklist.md`.
// -----------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct InspectJob {
    name: String,
    /// Derived operational kind: `scheduled` / `reactor` / `queued_worker`.
    operational_kind: &'static str,
    trigger: InspectJobTrigger,
    #[serde(skip_serializing_if = "Option::is_none")]
    queue: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    idempotency_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    retry: Option<InspectJobRetry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    policy: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tenant_from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    fanout: Option<InspectJobFanout>,
    #[serde(skip_serializing_if = "Option::is_none")]
    timeout: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    external_calls: Vec<InspectJobExternalCall>,
    body: InspectJobBody,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    emits: Vec<String>,
    origin: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", content = "value")]
enum InspectJobTrigger {
    /// `trigger event <feature>.<event>`.
    Event(String),
    /// `trigger schedule "<cron>"`.
    Schedule(String),
}

#[derive(Debug, Serialize)]
struct InspectJobRetry {
    count: u32,
    backoff: &'static str,
}

#[derive(Debug, Serialize)]
struct InspectJobFanout {
    scope: &'static str,
    axis: String,
}

#[derive(Debug, Serialize)]
struct InspectJobExternalCall {
    slot: String,
    op: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    args: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", content = "value")]
enum InspectJobBody {
    /// `handler "./..."` — declarative path with optional return type.
    Handler(InspectJobHandler),
    /// Declarative body with the typed declarative spine (Phase L Tier
    /// 4b). Replaces the previous raw-string carve-out.
    Declarative(InspectJobDeclarative),
    /// Job declares no body — emits-only reactor.
    None,
}

#[derive(Debug, Serialize)]
struct InspectJobHandler {
    path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    returns: Option<String>,
}

#[derive(Debug, Serialize)]
struct InspectJobDeclarative {
    #[serde(skip_serializing_if = "Option::is_none")]
    target: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    lets: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    effect: Option<String>,
}

#[derive(Debug, Serialize)]
struct InspectWebhook {
    name: String,
    route: String,
    verify: InspectWebhookVerify,
    #[serde(skip_serializing_if = "Option::is_none")]
    tenant_from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    idempotency_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    policy: Option<String>,
    handler: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    returns: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    emits: Vec<String>,
    // Webhooks expanded cycle — typed envelope reference. Atrito #2:
    // structured ref, not opaque string.
    #[serde(skip_serializing_if = "Option::is_none")]
    payload_from: Option<InspectWebhookPayloadFrom>,
    #[serde(skip_serializing_if = "Option::is_none")]
    replay: Option<InspectWebhookReplay>,
    #[serde(skip_serializing_if = "Option::is_none")]
    dlq: Option<InspectWebhookDlq>,
    // Webhooks expanded cycle — Atrito #5: retry shares the jobs IR
    // `RetryPolicy` shape.
    #[serde(skip_serializing_if = "Option::is_none")]
    retry: Option<InspectWebhookRetry>,
    origin: &'static str,
}

#[derive(Debug, Serialize)]
struct InspectWebhookVerify {
    scheme: &'static str,
    algorithm: String,
    secret_env: String,
    header: String,
}

/// Webhooks expanded cycle — typed payload-from projection. The
/// `path` field is the canonical surface form (`webhook_events.<name>`)
/// so JSON consumers do not have to reconstruct the catalog prefix.
#[derive(Debug, Serialize)]
struct InspectWebhookPayloadFrom {
    name: String,
    path: String,
}

#[derive(Debug, Serialize)]
struct InspectWebhookReplay {
    mode: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    within: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    dedupe_by: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum InspectWebhookDlq {
    Emit { event: String },
    Handler { path: String },
    Drop { reason: String },
}

#[derive(Debug, Serialize)]
struct InspectWebhookRetry {
    count: u32,
    backoff: &'static str,
}

#[derive(Debug, Serialize)]
struct InspectEventGroup {
    pattern: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    on_resource: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    payload: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    audit: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    events: Vec<String>,
    origin: &'static str,
}

// CL.C.4 — `--expand=aggregates` projections (roadmap §1.7).
#[derive(Debug, Serialize)]
struct InspectAggregate {
    name: String,
    root: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    contains: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    invariants: Vec<InspectInvariant>,
    origin: &'static str,
}

#[derive(Debug, Serialize)]
struct InspectInvariant {
    name: String,
    /// Closed-catalog predicate text. The IR carries an
    /// `EvalPredicate`; we stringify it back so the projection is
    /// stable across `Closed` / `Unparsed` / `Contains` shapes.
    when: String,
    /// Predicate kind as projected. Aids LLM/cold-reader inspection;
    /// stable closed catalog: `closed | contains | tools_calls | unparsed`.
    when_kind: &'static str,
    #[serde(skip_serializing_if = "String::is_empty")]
    message: String,
}

#[derive(Debug, Serialize)]
struct InspectSecurityWebhook {
    webhook: String,
    verify: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    secrets: Vec<String>,
    origin: &'static str,
}

#[derive(Debug, Serialize)]
struct InspectDefault {
    name: String,
    value: String,
    origin: &'static str,
    applies_to: Vec<String>,
}

#[derive(Debug, Serialize)]
struct InspectEvent {
    name: String,
    payload: Vec<InspectPayloadField>,
}

#[derive(Debug, Serialize)]
struct InspectPayloadField {
    name: String,
    ty: String,
    origin: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    expression: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    condition: Option<String>,
}

#[derive(Debug, Serialize)]
struct InspectTarget {
    command: String,
    target: String,
    origin: String,
}

#[derive(Debug, Serialize)]
struct InspectPolicy {
    subject: String,
    policy: String,
    atoms: Vec<String>,
    origin: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    requires: Vec<InspectPolicyRequirement>,
    /// IR Error-Vocab (Cell PARSE-1) — per-policy or per-command
    /// `when_denied @translation.<key>` override surfaced from the
    /// lifted IR. `None` when neither the `policies.<category>` nor
    /// `command.policy` declared an override. Resolution-chain steps 1
    /// and 2 (proposal §2.E).
    #[serde(skip_serializing_if = "Option::is_none")]
    when_denied: Option<String>,
}

#[derive(Debug, Serialize)]
struct InspectPolicyRequirement {
    policy: String,
    atoms: Vec<String>,
    origin: String,
}

#[derive(Debug, Serialize)]
struct InspectTests {
    subject: String,
    groups: BTreeMap<String, Vec<InspectTestAssertion>>,
}

#[derive(Debug, Serialize)]
struct InspectTestAssertion {
    assertion: String,
    origin: String,
}

fn inspect_canonical_source(source: &str, input: &Path, expansions: ExpandSet) -> InspectReport {
    inspect_canonical_source_with_aliases(source, input, expansions, &std::collections::BTreeMap::new())
}

/// B3 — variant of [`inspect_canonical_source`] that applies a plugin
/// alias map to lifted features so `--expand=resources` projections
/// surface `SemanticPluginType` carriers rather than the unresolved
/// `UserDefined` placeholders authored in `.lzi`.
fn inspect_canonical_source_with_aliases(
    source: &str,
    input: &Path,
    expansions: ExpandSet,
    alias_map: &std::collections::BTreeMap<String, plugin_manifest::ResolvedPluginSemantic>,
) -> InspectReport {
    let lines: Vec<String> = source.lines().map(str::to_owned).collect();

    let is_lzx = input
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("lzx"))
        .unwrap_or(false);

    let lzx_module = if is_lzx {
        lazuli_syntax::parse_lzx_document(source)
            .ok()
            .map(|document| lazuli_analyzer::lower_lzx_document(&document))
    } else {
        None
    };

    let (lzx_app, routes, experiences, surfaces) = match lzx_module {
        Some(module) => (
            module.app,
            module.routes,
            module.experiences,
            module.surfaces,
        ),
        None => (None, Vec::new(), Vec::new(), Vec::new()),
    };

    // Phase L — lower the canonical-indent slice once per inspect call
    // and build a per-feature lookup. The slice is permissive about
    // unknown constructs, so a failed parse degrades gracefully into
    // an empty lookup; the text-pattern inspect path still runs.
    let auth_by_feature = if expansions.auth && !is_lzx {
        collect_auth_by_feature(source)
    } else {
        std::collections::BTreeMap::new()
    };

    // Phase L Tier 3 — collect the lifted `Job`/`Webhook`/
    // `EventGroup` shapes for every feature in one pass. Reuses the
    // same parse-and-lower the auth lookup runs; degradation rules
    // match (empty map on parse failure). Tier 4 follow-up also
    // surfaces typed `policies` here so `inspect_policies`/
    // `inspect_tests` consume the IR instead of a text walker.
    let tier3_by_feature = if (expansions.jobs
        || expansions.webhooks
        || expansions.event_groups
        || expansions.notifications
        || expansions.policies
        || expansions.tests
        || expansions.migrations
        || expansions.caches
        || expansions.aggregates
        || expansions.defaults
        || expansions.commands
        || expansions.apis
        || expansions.resources
        || expansions.queries
        || expansions.records
        || expansions.errors)
        && !is_lzx
    {
        collect_tier3_by_feature_with_aliases(source, alias_map)
    } else {
        std::collections::BTreeMap::new()
    };

    let registry = app_manifest::parse_app_registry(source);
    let webhook_events = expansions.webhook_events.then(|| {
        registry
            .as_ref()
            .map(|registry| registry.webhook_events.clone())
            .unwrap_or_default()
    });

    let app = app_manifest::parse_app_manifest(source).or(lzx_app);
    // Roadmap §1.2 — unified HTTP hygiene projection. Only populated
    // when the flag is set; the typed blocks still surface via `app`
    // either way.
    let http = if expansions.http {
        inspect::expand_http::expand_http(app.as_ref())
    } else {
        None
    };

    InspectReport {
        schema: "lazuli.inspect.v0",
        source: input.display().to_string(),
        expand: expansions.labels(),
        workspace: app_manifest::parse_app_workspace(source),
        contracts: app_manifest::parse_app_contracts(source),
        app,
        registry,
        webhook_events,
        profiles: app_manifest::parse_app_profiles(source),
        routes,
        experiences,
        surfaces,
        http,
        features: inspect_features(&lines, expansions, &auth_by_feature, &tier3_by_feature),
    }
}

/// Phase L Tier 3 — lower the canonical-indent slice once per inspect
/// call and build a per-feature lookup of `(jobs, webhooks,
/// event_groups)`. Same degradation rules as `collect_auth_by_feature`:
/// failures fall through to an empty map so `--expand=jobs` etc. are
/// projections, not checks.
fn collect_tier3_by_feature(source: &str) -> std::collections::BTreeMap<String, Tier3FeatureSlice> {
    collect_tier3_by_feature_with_aliases(source, &std::collections::BTreeMap::new())
}

/// B3 — variant that applies the plugin alias map to lifted features
/// so inspect's `--expand=resources` projection surfaces
/// `SemanticPluginType` carriers rather than `UserDefined` placeholders.
fn collect_tier3_by_feature_with_aliases(
    source: &str,
    alias_map: &std::collections::BTreeMap<String, plugin_manifest::ResolvedPluginSemantic>,
) -> std::collections::BTreeMap<String, Tier3FeatureSlice> {
    let mut map = std::collections::BTreeMap::new();
    let Ok(features) = lazuli_syntax::parse_feature_skeletons(source) else {
        return map;
    };
    for feature_ast in features {
        let Ok(mut feature_ir) = lazuli_analyzer::lower_feature_skeleton(&feature_ast) else {
            continue;
        };
        if !alias_map.is_empty() {
            // Reuse the package-level resolver pass on this single
            // feature. Wrap in a transient module so the walker
            // signature is stable across both callers.
            let mut transient = lazuli_ir::Module {
                workspace: None,
                contracts: Vec::new(),
                app: None,
                registry: None,
                profiles: Vec::new(),
                design: None,
                rbac: None,
                features: vec![feature_ir],
            };
            plugin_semantic_resolver::apply_plugin_semantic_resolution(
                &mut transient,
                alias_map,
            );
            feature_ir = transient.features.pop().unwrap();
        }
        map.insert(
            feature_ir.name.clone(),
            Tier3FeatureSlice {
                jobs: feature_ir.jobs,
                webhooks: feature_ir.webhooks,
                event_groups: feature_ir.event_groups,
                tenant_migrations: feature_ir.tenant_migrations,
                notifications: feature_ir.notifications,
                policies: feature_ir.policies,
                caches: feature_ir.caches,
                aggregates: feature_ir.aggregates,
                defaults: feature_ir.defaults,
                resource_names: feature_ir
                    .resources
                    .iter()
                    .map(|r| r.name.clone())
                    .collect(),
                commands: feature_ir.commands,
                apis: feature_ir.apis,
                resources: feature_ir.resources,
                queries: feature_ir.queries,
                records: feature_ir.records,
                errors: feature_ir.errors,
            },
        );
    }
    map
}

struct Tier3FeatureSlice {
    jobs: Vec<lazuli_ir::Job>,
    webhooks: Vec<lazuli_ir::Webhook>,
    event_groups: Vec<lazuli_ir::EventGroup>,
    /// Migrations bucket cycle Route C — lifted `tenant_migration`
    /// declarations for `--expand=migrations`.
    tenant_migrations: Vec<lazuli_ir::TenantMigration>,
    /// Notifications expanded bucket cycle — lifted `notification`
    /// declarations. Powers the typed `digest`/`throttle` projection
    /// in `inspect_notifications`; the text-walker keeps owning the
    /// scalar fields so the projection stays additive.
    notifications: Vec<lazuli_ir::Notification>,
    /// Tier 4 follow-up — lifted `policies` block. Powers the typed
    /// `category -> atoms` lookup that `inspect_policies` and
    /// `inspect_tests` consume; retires the `collect_policy_atoms`
    /// text walker.
    policies: lazuli_ir::Policies,
    /// Cache bucket cycle (CL.C.3) — lifted feature-level
    /// `cache <name>` profile declarations. Powers `--expand=caches`.
    caches: Vec<lazuli_ir::CacheProfile>,
    /// CL.C.4 — lifted `aggregate <Name>` declarations. Powers
    /// `--expand=aggregates`.
    aggregates: Vec<lazuli_ir::Aggregate>,
    /// Phase L Tier 4a — lifted feature-level `defaults` block.
    /// Powers `--expand=defaults` IR-driven projection; replaces the
    /// text-pattern walker for the canonical-indent code path.
    defaults: lazuli_ir::Defaults,
    /// Phase L Tier 4a — resource names lifted from
    /// `Feature.resources`. Used by `--expand=defaults` to compute
    /// `applies_to` for `tenancy`/`timestamps` defaults without
    /// re-walking the source text.
    resource_names: Vec<String>,
    /// Phase L Tier 4b — lifted `command <name>` declarations on the
    /// feature. Powers `--expand=commands`; emitted verbatim from IR
    /// so downstream consumers see the typed Command shape (with
    /// audit, approval, invalidates, etc.) without re-deriving from
    /// text.
    commands: Vec<lazuli_ir::Command>,
    /// Phase L Tier 4b — lifted `api <name>` declarations on the
    /// feature. Powers `--expand=apis` (accepting `api` or `apis`).
    apis: Vec<lazuli_ir::Api>,
    /// Phase L Tier 4c — lifted `resource <Name>` declarations on the
    /// feature. Powers `--expand=resources`.
    resources: Vec<lazuli_ir::Resource>,
    /// Phase L Tier 4d — lifted `query.{list,lookup,sql}` declarations
    /// on the feature. Powers `--expand=queries`.
    queries: Vec<lazuli_ir::Query>,
    /// Phase L Tier 4d — lifted `record <Name>` declarations on the
    /// feature. Powers `--expand=records`.
    records: Vec<lazuli_ir::Record>,
    /// IR Error-Vocab (Cell PARSE-1) — lifted `errors` block. `None`
    /// when the feature declared no `errors` block. Powers
    /// `--expand=errors` projection.
    errors: Option<lazuli_ir::FeatureErrors>,
}

/// Phase L — run the canonical-indent slice and build a `feature_name ->
/// IR Auth` lookup. Failures in either parse or lower silently degrade
/// to an empty lookup: `--expand=auth` is a projection, not a check,
/// so it must not flip inspect into an error path.
fn collect_auth_by_feature(source: &str) -> std::collections::BTreeMap<String, lazuli_ir::Auth> {
    let mut map = std::collections::BTreeMap::new();
    let Ok(features) = lazuli_syntax::parse_feature_skeletons(source) else {
        return map;
    };
    for feature in features {
        if let Some(auth_ast) = feature.auth.as_ref() {
            if let Ok(auth_ir) = lazuli_analyzer::lower_auth(auth_ast) {
                map.insert(feature.name.clone(), auth_ir);
            }
        }
    }
    map
}

fn inspect_features(
    lines: &[String],
    expansions: ExpandSet,
    auth_by_feature: &std::collections::BTreeMap<String, lazuli_ir::Auth>,
    tier3_by_feature: &std::collections::BTreeMap<String, Tier3FeatureSlice>,
) -> Vec<InspectFeature> {
    let mut features = Vec::new();
    let mut index = 0;

    while index < lines.len() {
        if leading_spaces(&lines[index]) == 0 && lines[index].trim_start().starts_with("feature ") {
            let start = index;
            index += 1;

            while index < lines.len() {
                if leading_spaces(&lines[index]) == 0
                    && lines[index].trim_start().starts_with("feature ")
                {
                    break;
                }
                index += 1;
            }

            features.push(inspect_feature(
                &lines[start..index],
                expansions,
                auth_by_feature,
                tier3_by_feature,
            ));
        } else {
            index += 1;
        }
    }

    features
}

fn inspect_feature(
    lines: &[String],
    expansions: ExpandSet,
    auth_by_feature: &std::collections::BTreeMap<String, lazuli_ir::Auth>,
    tier3_by_feature: &std::collections::BTreeMap<String, Tier3FeatureSlice>,
) -> InspectFeature {
    let name = lines
        .first()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("unknown")
        .to_owned();
    let external_calls = inspect_external_calls(&name, lines);
    let agents = inspect_agents(lines);
    let tier3 = tier3_by_feature.get(&name);
    // Tier 4 follow-up — `policies` category lookup now reads typed
    // `Feature.policies.categories` from the Tier 3 slice. Falls back
    // to an empty map when the slice is absent (either because the
    // feature has no `policies` block, or because no expand flag
    // gated the slice collection).
    let policies: BTreeMap<String, Vec<String>> = tier3
        .map(|t| {
            t.policies
                .categories
                .iter()
                .map(|c| (c.name.clone(), c.atoms.clone()))
                .collect()
        })
        .unwrap_or_default();
    let notifications = inspect_notifications(lines, tier3);

    let tools = expansions
        .tools
        .then(|| inspect_agent_tools_projection(&agents));

    let expose = expansions
        .expose
        .then(|| inspect_expose_projection(&name, &agents, lines));

    // Phase L — auth projection is only present when `--expand=auth`
    // is set AND the feature declared an `auth` block. Features
    // without auth omit the field entirely so consumers can distinguish
    // "no auth declared" from "auth declared but empty".
    let auth = expansions
        .auth
        .then(|| {
            auth_by_feature
                .get(&name)
                .map(|auth| project_auth(&name, auth))
        })
        .flatten();

    // Phase L Tier 2 — storage projection harvests every `@cap.File(...)`
    // site from the source text and runs each through the typed
    // analyzer pass. The projection is omitted when the feature
    // declares zero file capabilities; that distinguishes "no storage"
    // from "storage declared but empty" for downstream consumers.
    let storage = expansions
        .storage
        .then(|| inspect_storage_projection(lines))
        .filter(|s| !s.fields.is_empty() || !s.api_outputs.is_empty());

    // Phase L Tier 3 — jobs/webhooks/event_groups projections. Each is
    // present only when the matching expand flag is set AND the feature
    // actually declares the construct. Empty arrays still surface so
    // consumers can distinguish "flag not set" from "no constructs
    // declared". `tier3` is bound earlier in this function so the
    // notification projection can read typed `digest`/`throttle`.
    let jobs_projection = expansions.jobs.then(|| {
        tier3
            .map(|t| t.jobs.iter().map(project_job).collect::<Vec<_>>())
            .unwrap_or_default()
    });
    let webhooks_projection = expansions.webhooks.then(|| {
        tier3
            .map(|t| t.webhooks.iter().map(project_webhook).collect::<Vec<_>>())
            .unwrap_or_default()
    });
    let event_groups_projection = expansions.event_groups.then(|| {
        tier3
            .map(|t| {
                t.event_groups
                    .iter()
                    .map(project_event_group)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    });
    // Migrations bucket cycle Route C — `--expand=migrations`. Surfaces
    // every lifted `ir::TenantMigration` on the feature.
    let tenant_migrations_projection = expansions.migrations.then(|| {
        tier3
            .map(|t| t.tenant_migrations.clone())
            .unwrap_or_default()
    });
    // CL.C.4 — `--expand=aggregates` projection.
    let aggregates_projection = expansions.aggregates.then(|| {
        tier3
            .map(|t| {
                t.aggregates
                    .iter()
                    .map(project_aggregate)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    });
    // Cache bucket cycle (CL.C.3) — `--expand=caches`. Surfaces every
    // lifted feature-level `cache <name>` profile on the feature.
    // Empty arrays still surface so consumers can distinguish "flag
    // not set" from "no profiles declared".
    let caches_projection = expansions
        .caches
        .then(|| tier3.map(|t| t.caches.clone()).unwrap_or_default());
    // Phase L Tier 4b — `--expand=commands` projects every lifted
    // `ir::Command` on the feature. Empty arrays surface so consumers
    // distinguish "flag not set" from "no commands declared".
    let commands_projection = expansions
        .commands
        .then(|| tier3.map(|t| t.commands.clone()).unwrap_or_default());
    // Phase L Tier 4b — `--expand=apis` projects every lifted
    // `ir::Api` on the feature.
    let apis_projection = expansions
        .apis
        .then(|| tier3.map(|t| t.apis.clone()).unwrap_or_default());
    // Phase L Tier 4c — `--expand=resources` projects every lifted
    // `ir::Resource` on the feature.
    let resources_projection = expansions
        .resources
        .then(|| tier3.map(|t| t.resources.clone()).unwrap_or_default());
    // Phase L Tier 4d — `--expand=queries` projects every lifted
    // `ir::Query` on the feature.
    let queries_projection = expansions
        .queries
        .then(|| tier3.map(|t| t.queries.clone()).unwrap_or_default());
    // Phase L Tier 4d — `--expand=records` projects every lifted
    // `ir::Record` on the feature.
    let records_projection = expansions
        .records
        .then(|| tier3.map(|t| t.records.clone()).unwrap_or_default());
    // IR Error-Vocab (Cell PARSE-1) — `--expand=errors` projects the
    // lifted `ir::FeatureErrors` block (None when the feature has no
    // `errors` block authored). The outer `Option` is gated by the
    // expansion flag; the inner `Option` is gated by authoring.
    let errors_projection = expansions
        .errors
        .then(|| tier3.and_then(|t| t.errors.clone()))
        .flatten();

    InspectFeature {
        name,
        requirements: inspect_requirements(lines),
        external_calls,
        agents,
        notifications,
        refs: expansions.refs.then(|| inspect_refs(lines)),
        summary: expansions.summary.then(|| inspect_summary(lines)),
        locators: expansions.locators.then(|| inspect_locators(lines)),
        dependencies: expansions.dependencies.then(|| inspect_dependencies(lines)),
        security: expansions.security.then(|| inspect_security(lines)),
        defaults: expansions.defaults.then(|| inspect_defaults(lines, tier3)),
        events: expansions.events.then(|| inspect_events(lines)),
        built_in_trace_events: expansions.events.then(inspect_built_in_trace_events),
        targets: expansions.targets.then(|| inspect_targets(lines)),
        policies: expansions
            .policies
            .then(|| inspect_policies(lines, &policies, tier3)),
        tests: expansions.tests.then(|| inspect_tests(lines, &policies)),
        tools,
        expose,
        auth,
        storage,
        jobs: jobs_projection,
        webhooks: webhooks_projection,
        event_groups: event_groups_projection,
        tenant_migrations: tenant_migrations_projection,
        caches: caches_projection,
        aggregates: aggregates_projection,
        commands: commands_projection,
        apis: apis_projection,
        resources: resources_projection,
        queries: queries_projection,
        records: records_projection,
        errors: errors_projection,
    }
}

// -----------------------------------------------------------------------------
// Phase L Tier 3 — IR -> Inspect projections.
// -----------------------------------------------------------------------------

fn project_job(job: &lazuli_ir::Job) -> InspectJob {
    let operational_kind: &'static str = match (&job.trigger, &job.queue) {
        (lazuli_ir::JobTrigger::Schedule { .. }, _) => "scheduled",
        (lazuli_ir::JobTrigger::Event { .. }, Some(_)) => "queued_worker",
        (lazuli_ir::JobTrigger::Event { .. }, None) => "reactor",
    };
    let trigger = match &job.trigger {
        lazuli_ir::JobTrigger::Event { event } => InspectJobTrigger::Event(format_qname(event)),
        lazuli_ir::JobTrigger::Schedule { cron } => InspectJobTrigger::Schedule(cron.clone()),
    };
    InspectJob {
        name: job.name.clone(),
        operational_kind,
        trigger,
        queue: job.queue.clone(),
        idempotency_by: job.idempotency.as_ref().map(|i| path_to_string(&i.by)),
        retry: job.retry.as_ref().map(|r| InspectJobRetry {
            count: r.count,
            backoff: match r.backoff {
                lazuli_ir::BackoffStrategy::Exponential => "exponential",
                lazuli_ir::BackoffStrategy::Fixed => "fixed",
            },
        }),
        policy: job.policy.as_ref().map(policy_ref_to_string),
        tenant_from: job.tenant_from.as_ref().map(|t| path_to_string(&t.path)),
        fanout: job.fanout.as_ref().map(|f| InspectJobFanout {
            scope: match f.scope {
                lazuli_ir::FanoutScope::Tenants => "tenants",
            },
            axis: f.axis.clone(),
        }),
        timeout: job.timeout.clone(),
        external_calls: job
            .external_calls
            .iter()
            .map(|c| InspectJobExternalCall {
                slot: c.slot.clone(),
                op: c.op.clone(),
                args: c.args.iter().map(|a| a.name.clone()).collect(),
            })
            .collect(),
        body: match &job.body {
            lazuli_ir::JobBody::Handler(h) => InspectJobBody::Handler(InspectJobHandler {
                path: h.path.path.clone(),
                returns: h.returns.as_ref().map(type_ref_to_string),
            }),
            lazuli_ir::JobBody::Declarative(d) => {
                let target_text = d.target.as_ref().map(inspect_target_expr_to_string);
                let lets_text: Vec<String> =
                    d.lets.iter().map(inspect_let_binding_to_string).collect();
                let effect_text = match &d.effect {
                    lazuli_ir::CommandEffect::None => None,
                    other => Some(inspect_command_effect_to_string(other)),
                };
                if target_text.is_none() && lets_text.is_empty() && effect_text.is_none() {
                    InspectJobBody::None
                } else {
                    InspectJobBody::Declarative(InspectJobDeclarative {
                        target: target_text,
                        lets: lets_text,
                        effect: effect_text,
                    })
                }
            }
        },
        emits: job.emits.clone(),
        origin: "job",
    }
}

fn project_webhook(webhook: &lazuli_ir::Webhook) -> InspectWebhook {
    let verify = match &webhook.structured_verify {
        Some(v) => InspectWebhookVerify {
            scheme: match v.scheme {
                lazuli_ir::VerifyScheme::Hmac => "hmac",
            },
            algorithm: v.algorithm.clone(),
            secret_env: v.secret_env.clone(),
            header: v.header.clone(),
        },
        None => InspectWebhookVerify {
            scheme: "hmac",
            algorithm: String::new(),
            secret_env: String::new(),
            header: String::new(),
        },
    };
    // Webhooks expanded cycle — typed projections for the four new
    // children. Each Option<…> is skipped when absent so consumers
    // that lived through Tier 3 see no churn.
    let payload_from = webhook
        .payload_from
        .as_ref()
        .map(|r| InspectWebhookPayloadFrom {
            name: r.name.clone(),
            path: format!("webhook_events.{}", r.name),
        });
    let replay = webhook.replay.as_ref().map(|r| InspectWebhookReplay {
        mode: match r.mode {
            lazuli_ir::ReplayMode::Allow => "allow",
            lazuli_ir::ReplayMode::Deny => "deny",
        },
        within: r.within.clone(),
        dedupe_by: r.dedupe_by.as_ref().map(path_to_string),
    });
    let dlq = webhook.dlq.as_ref().map(|d| match d {
        lazuli_ir::DlqSpec::Emit { event } => InspectWebhookDlq::Emit {
            event: event.clone(),
        },
        lazuli_ir::DlqSpec::Handler { path } => InspectWebhookDlq::Handler {
            path: path.path.clone(),
        },
        lazuli_ir::DlqSpec::Drop { reason } => InspectWebhookDlq::Drop {
            reason: reason.clone(),
        },
    });
    let retry = webhook.retry.as_ref().map(|r| InspectWebhookRetry {
        count: r.count,
        backoff: match r.backoff {
            lazuli_ir::BackoffStrategy::Fixed => "fixed",
            lazuli_ir::BackoffStrategy::Exponential => "exponential",
        },
    });
    InspectWebhook {
        name: webhook.name.clone(),
        route: webhook.route.clone(),
        verify,
        tenant_from: webhook
            .tenant_from
            .as_ref()
            .map(|t| path_to_string(&t.path)),
        idempotency_by: webhook.idempotency.as_ref().map(|i| path_to_string(&i.by)),
        policy: webhook.policy.as_ref().map(policy_ref_to_string),
        handler: webhook.handler.path.clone(),
        returns: webhook.returns.as_ref().map(type_ref_to_string),
        emits: webhook.emits.clone(),
        payload_from,
        replay,
        dlq,
        retry,
        origin: "webhook",
    }
}

fn project_event_group(group: &lazuli_ir::EventGroup) -> InspectEventGroup {
    InspectEventGroup {
        pattern: group.pattern.clone(),
        on_resource: group.on_resource.clone(),
        payload: group.raw_payload.clone(),
        audit: group.raw_audit.clone(),
        events: group.events.clone(),
        origin: "event_group",
    }
}

// CL.C.4 — project an `ir::Aggregate` into the inspect view.
fn project_aggregate(agg: &lazuli_ir::Aggregate) -> InspectAggregate {
    InspectAggregate {
        name: agg.name.clone(),
        root: format_qname(&agg.root),
        contains: agg.contains.iter().map(format_qname).collect(),
        invariants: agg.invariants.iter().map(project_invariant).collect(),
        origin: "aggregate",
    }
}

fn project_invariant(inv: &lazuli_ir::Invariant) -> InspectInvariant {
    let (when, when_kind): (String, &'static str) = match &inv.when {
        lazuli_ir::EvalPredicate::Closed(pred) => {
            (predicate_to_string(pred), "closed")
        }
        lazuli_ir::EvalPredicate::Contains { lhs, rhs } => {
            let rhs_str = match rhs {
                lazuli_ir::EvalContainsRhs::Literal(t) => format!("\"{t}\""),
                lazuli_ir::EvalContainsRhs::SemanticType(q) => format_qname(q),
            };
            (
                format!("{} contains {}", path_to_string(lhs), rhs_str),
                "contains",
            )
        }
        lazuli_ir::EvalPredicate::ToolsCalls { op, target } => (
            format!("tools.calls {} {}", op_as_str(op), tool_ref_to_string(target)),
            "tools_calls",
        ),
        lazuli_ir::EvalPredicate::Unparsed(text) => (text.clone(), "unparsed"),
    };
    InspectInvariant {
        name: inv.name.clone(),
        when,
        when_kind,
        message: inv.message.clone(),
    }
}

fn predicate_to_string(pred: &lazuli_ir::Predicate) -> String {
    match pred {
        lazuli_ir::Predicate::Comparison { left, op, right } => format!(
            "{} {} {}",
            inspect_expr_to_string(left),
            compare_op_to_string(*op),
            inspect_expr_to_string(right),
        ),
        lazuli_ir::Predicate::Has {
            collection,
            element,
        } => format!(
            "{} has {}",
            inspect_expr_to_string(collection),
            inspect_expr_to_string(element),
        ),
        lazuli_ir::Predicate::And(parts) => parts
            .iter()
            .map(predicate_to_string)
            .collect::<Vec<_>>()
            .join(" and "),
        lazuli_ir::Predicate::Or(parts) => parts
            .iter()
            .map(predicate_to_string)
            .collect::<Vec<_>>()
            .join(" or "),
    }
}

fn compare_op_to_string(op: lazuli_ir::CompareOp) -> &'static str {
    match op {
        lazuli_ir::CompareOp::Eq => "=",
        lazuli_ir::CompareOp::Ne => "!=",
        lazuli_ir::CompareOp::Lt => "<",
        lazuli_ir::CompareOp::Le => "<=",
        lazuli_ir::CompareOp::Gt => ">",
        lazuli_ir::CompareOp::Ge => ">=",
    }
}

fn op_as_str(op: &lazuli_ir::ToolsCallsOp) -> &'static str {
    match op {
        lazuli_ir::ToolsCallsOp::Includes => "includes",
        lazuli_ir::ToolsCallsOp::Excludes => "excludes",
    }
}

fn tool_ref_to_string(t: &lazuli_ir::QualifiedToolRef) -> String {
    match t {
        lazuli_ir::QualifiedToolRef::Local { kind, name } => {
            format!("{}.{}", tool_kind_segment(*kind), name)
        }
        lazuli_ir::QualifiedToolRef::CrossFeature {
            feature,
            kind,
            name,
        } => format!("{feature}.{}.{name}", tool_kind_segment(*kind)),
        lazuli_ir::QualifiedToolRef::Adapter { dotted } => {
            format!("@tool.{}", dotted.join("."))
        }
    }
}

fn tool_kind_segment(kind: lazuli_ir::ToolKind) -> &'static str {
    match kind {
        lazuli_ir::ToolKind::QueryList => "query.list",
        lazuli_ir::ToolKind::QueryLookup => "query.lookup",
        lazuli_ir::ToolKind::QuerySql => "query.sql",
        lazuli_ir::ToolKind::QueryView => "query.view",
        lazuli_ir::ToolKind::Command => "command",
        lazuli_ir::ToolKind::Api => "api",
        lazuli_ir::ToolKind::QueryUnspecified => "query",
    }
}

fn format_qname(q: &lazuli_ir::QualifiedName) -> String {
    match q.feature.as_deref() {
        Some(f) => format!("{f}.{}", q.name),
        None => q.name.clone(),
    }
}

fn path_to_string(p: &lazuli_ir::Path) -> String {
    p.segments.join(".")
}

fn policy_ref_to_string(p: &lazuli_ir::PolicyRef) -> String {
    match p {
        lazuli_ir::PolicyRef::Local(name) => format!("@policy.{name}"),
        lazuli_ir::PolicyRef::Atom(atom) => atom.clone(),
        lazuli_ir::PolicyRef::External { feature, name } => format!("{feature}.{name}"),
        lazuli_ir::PolicyRef::Unresolved(text) => text.clone(),
        lazuli_ir::PolicyRef::None => String::new(),
    }
}

fn type_ref_to_string(t: &lazuli_ir::TypeRef) -> String {
    match t {
        lazuli_ir::TypeRef::Builtin(b) => format!("{b:?}"),
        lazuli_ir::TypeRef::UserDefined(q) => format_qname(q),
        lazuli_ir::TypeRef::EnumRef(q) => format_qname(q),
        lazuli_ir::TypeRef::Many(inner) => format!("Many({})", type_ref_to_string(inner)),
        lazuli_ir::TypeRef::Unresolved(s) => s.clone(),
        lazuli_ir::TypeRef::Capability(_) => "@cap.File(...)".to_owned(),
    }
}

/// Phase L Tier 4b — pretty-print a typed `Expr` back into source-like
/// text for inspect projections. Used by both job declarative bodies
/// and command projections so the inspect output is stable across
/// Tier 3 and Tier 4 lifts.
fn inspect_expr_to_string(e: &lazuli_ir::Expr) -> String {
    match e {
        lazuli_ir::Expr::Path(p) => p.segments.join("."),
        lazuli_ir::Expr::String(s) => format!("\"{s}\""),
        lazuli_ir::Expr::Integer(n) => n.to_string(),
        lazuli_ir::Expr::Boolean(b) => b.to_string(),
        lazuli_ir::Expr::Enum(l) => match &l.type_name {
            Some(q) => format!("{}.{}", format_qname(q), l.variant),
            None => l.variant.clone(),
        },
        lazuli_ir::Expr::Nil => "nil".to_owned(),
        lazuli_ir::Expr::FnCall(call) => {
            let args: Vec<String> = call.args.iter().map(inspect_expr_to_string).collect();
            format!("@fn.{}({})", call.name.name, args.join(", "))
        }
    }
}

fn inspect_target_expr_to_string(t: &lazuli_ir::TargetExpr) -> String {
    let args: Vec<String> = t
        .args
        .iter()
        .map(|a| format!("{}: {}", a.name, inspect_expr_to_string(&a.value)))
        .collect();
    format!("{}({})", format_qname(&t.query), args.join(", "))
}

fn inspect_let_binding_to_string(l: &lazuli_ir::LetBinding) -> String {
    format!("{} = {}", l.name, inspect_expr_to_string(&l.value))
}

fn inspect_command_effect_to_string(e: &lazuli_ir::CommandEffect) -> String {
    match e {
        lazuli_ir::CommandEffect::Creates(c) => {
            let head = if c.from_input {
                format!("creates {} from input", format_qname(&c.resource))
            } else {
                format!("creates {}", format_qname(&c.resource))
            };
            inspect_assignments_to_string(&head, &c.assignments)
        }
        lazuli_ir::CommandEffect::Updates(u) => inspect_assignments_to_string(
            &format!("updates {}", format_qname(&u.resource)),
            &u.assignments,
        ),
        lazuli_ir::CommandEffect::Deletes(d) => format!("deletes {}", format_qname(&d.resource)),
        lazuli_ir::CommandEffect::Returns(r) => {
            format!("returns {}", type_ref_to_string(&r.return_type))
        }
        lazuli_ir::CommandEffect::None => String::new(),
    }
}

fn inspect_assignments_to_string(head: &str, assignments: &[lazuli_ir::Assignment]) -> String {
    if assignments.is_empty() {
        head.to_owned()
    } else {
        let mut out = head.to_owned();
        for a in assignments {
            out.push_str("\n  ");
            out.push_str(&a.field);
            out.push_str(" = ");
            out.push_str(&inspect_expr_to_string(&a.value));
        }
        out
    }
}

/// Phase L Tier 2 — walk a feature's source lines, find every
/// `@cap.File(...)` site, parse it via the analyzer's typed pass, and
/// project the result. Two site shapes are recognised:
///
/// - `<field>: @cap.File(...)` inside a `resource <Name>` block.
/// - `output @cap.File(...)` inside an `api <name>` block.
///
/// Unparseable shapes are skipped silently so the LSP's existing
/// file-local diagnostics remain the canonical source of shape errors.
fn inspect_storage_projection(lines: &[String]) -> InspectStorage {
    let mut fields: Vec<InspectStorageField> = Vec::new();
    let mut api_outputs: Vec<InspectStorageApiOutput> = Vec::new();

    let mut current_resource: Option<String> = None;
    let mut current_api: Option<String> = None;
    let mut resource_indent: usize = 0;
    let mut api_indent: usize = 0;

    for raw in lines.iter() {
        let trimmed = raw.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = leading_spaces(raw);

        // Detect entering / exiting resource and api blocks. The
        // existing canonical fixture uses 4-space resource headers
        // (inside `domain`) and 2-space api headers; we close the
        // block as soon as the indent retreats to the header level
        // or shallower.
        if let Some(rest) = trimmed.strip_prefix("resource ") {
            current_resource = Some(rest.split_whitespace().next().unwrap_or("").to_owned());
            current_api = None;
            resource_indent = indent;
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("api ") {
            current_api = Some(rest.split_whitespace().next().unwrap_or("").to_owned());
            current_resource = None;
            api_indent = indent;
            continue;
        }
        if current_resource.is_some() && indent <= resource_indent && !trimmed.is_empty() {
            current_resource = None;
        }
        if current_api.is_some() && indent <= api_indent && !trimmed.is_empty() {
            current_api = None;
        }

        // Try a resource-field shape: `<field>: @cap.File(...)`.
        if let Some(resource) = current_resource.as_deref() {
            if let Some((field_name, cap_text)) = extract_cap_file_field(trimmed) {
                if let lazuli_ir::TypeRef::Capability(lazuli_ir::CapabilityRef::File(file)) =
                    lazuli_analyzer::type_ref_from_syntax_public(&cap_text)
                {
                    fields.push(InspectStorageField {
                        resource: resource.to_owned(),
                        field: field_name,
                        file_capability: project_file_capability(&file),
                    });
                }
            }
        }

        // Try an api-output shape: `output @cap.File(...)`.
        if let Some(api) = current_api.as_deref() {
            if let Some(cap_text) = trimmed
                .strip_prefix("output ")
                .map(str::trim)
                .filter(|rest| rest.starts_with("@cap.File("))
            {
                if let lazuli_ir::TypeRef::Capability(lazuli_ir::CapabilityRef::File(file)) =
                    lazuli_analyzer::type_ref_from_syntax_public(cap_text)
                {
                    api_outputs.push(InspectStorageApiOutput {
                        api: api.to_owned(),
                        file_capability: project_file_capability(&file),
                    });
                }
            }
        }
    }

    InspectStorage {
        fields,
        api_outputs,
    }
}

/// Extract `(field_name, "@cap.File(...)")` from a `<field>: @cap.File(...) [required]`
/// resource line. Returns `None` if the line is not a `@cap.File` field.
fn extract_cap_file_field(trimmed: &str) -> Option<(String, String)> {
    let (name_part, type_part) = trimmed.split_once(':')?;
    let name = name_part.trim();
    if name.is_empty() || name.contains(char::is_whitespace) {
        return None;
    }
    // Drop trailing `required` / `optional` / annotation keywords so the
    // analyzer parses the bare type expression.
    let type_tokens = type_part.trim();
    let cap_start = type_tokens.find("@cap.File(")?;
    let from_cap = &type_tokens[cap_start..];
    let close = from_cap.find(')')?;
    let cap_text = &from_cap[..=close];
    Some((name.to_owned(), cap_text.to_owned()))
}

fn project_file_capability(file: &lazuli_ir::FileCapability) -> InspectFileCapability {
    InspectFileCapability {
        max_size: InspectFileSize {
            bytes: file.max_size.bytes,
            literal: format_file_size_literal(file.max_size.literal),
        },
        accept: file
            .accept
            .iter()
            .map(|m| InspectMimeType {
                family: m.family.clone(),
                subtype: m.subtype.clone(),
            })
            .collect(),
        visibility: file
            .visibility
            .map(|v| format_file_visibility(v).to_owned()),
        signed_ttl: file.signed_ttl.clone(),
    }
}

/// Phase L — project a lowered `ir::Auth` into the inspect-shaped
/// `InspectAuth`. Mirrors the IR structure 1:1; the only translation is
/// joining `FieldRef` back into a `<Resource>.<field>` string so the
/// json projection reads exactly like the source surface.
fn project_auth(feature_name: &str, auth: &lazuli_ir::Auth) -> InspectAuth {
    let origin = inspect_origin(feature_name, auth.span_ref);
    InspectAuth {
        origin: origin.clone(),
        identity: InspectAuthIdentity {
            field: format!(
                "{}.{}",
                auth.identity.field.resource.name, auth.identity.field.field
            ),
            resource: auth.identity.field.resource.name.clone(),
            origin: origin.clone(),
        },
        password: auth.password.as_ref().map(|p| InspectAuthPassword {
            algorithm: p.algorithm.clone(),
            hash: p.hash.clone(),
            verify: p.verify.clone(),
            // `ir-rate-limit-env-aware` cell 1 — inspect shim: surface
            // the default literal. Cell 3 extends the projection with
            // the env-qualified shape.
            rate_limit: p.rate_limit.as_ref().map(|spec| spec.default.clone()),
            origin: origin.clone(),
        }),
        sessions: auth.sessions.as_ref().map(|s| InspectAuthSessions {
            resource: s.resource.name.clone(),
            ttl: s.ttl.clone(),
            refresh: s.refresh,
            access_ttl: s.access_ttl.clone(),
            rotation: s.rotation.clone(),
            origin: origin.clone(),
        }),
        mfa: auth.mfa.as_ref().map(|m| InspectAuthMfa {
            method: m.method.clone(),
            enroll: m.enroll.clone(),
            verify: m.verify.clone(),
            adapter: m.adapter.clone(),
            origin: origin.clone(),
        }),
        oauth: auth
            .oauth
            .iter()
            .map(|o| InspectAuthOAuthProvider {
                provider: o.provider.clone(),
                adapter: o.adapter.clone(),
                origin: origin.clone(),
            })
            .collect(),
    }
}

fn inspect_origin(feature_name: &str, span_ref: Option<lazuli_ir::SpanRef>) -> InspectOrigin {
    InspectOrigin {
        feature: feature_name.to_owned(),
        line: span_ref.map(|span| span.start),
    }
}

/// Materialise the per-feature unified HTTP route table for
/// `--expand=expose`. Walks every agent's `expose_http` and every
/// `api <name>` block in the feature body, emitting one entry per
/// declaration with stable `<feature>.<kind>.<name>` origins so
/// cross-feature collation downstream (doctor or external tools)
/// composes cleanly.
fn inspect_expose_projection(
    feature_name: &str,
    agents: &[InspectAgent],
    lines: &[String],
) -> Vec<InspectExposeEntry> {
    let mut entries: Vec<InspectExposeEntry> = Vec::new();

    for agent in agents {
        if let Some(expose) = agent.expose_http.as_ref() {
            entries.push(InspectExposeEntry {
                kind: "agent",
                origin: format!("{feature_name}.agent.{}", agent.name),
                method: expose.method.clone(),
                path: expose.path.clone(),
                route_slots: expose.route_slots.clone(),
                audience: expose.audience.clone(),
                rate_limit_override: expose.rate_limit_override.clone(),
            });
        }
    }

    for block in top_level_blocks(lines, "api ") {
        let name = named_top_block_name(block[0].trim_start())
            .unwrap_or("unknown")
            .to_owned();
        let method = direct_child_value(block, "method ").map(|m| m.to_ascii_uppercase());
        let path = direct_child_value(block, "path ")
            .as_deref()
            .map(strip_quotes);
        let audience = direct_child_value(block, "audience ");
        let rate_limit_override = direct_child_value(block, "rate_limit ")
            .as_deref()
            .map(strip_quotes);
        // Walk `route <name>:` children for slots.
        let mut route_slots: Vec<String> = Vec::new();
        let block_indent = block.first().map(|l| leading_spaces(l)).unwrap_or(0);
        let child_indent = block_indent + 2;
        for inner in block.iter().skip(1) {
            let trimmed = inner.trim_start();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            if leading_spaces(inner) != child_indent {
                continue;
            }
            if let Some(rest) = trimmed.strip_prefix("route ") {
                if let Some((slot, _)) = rest.split_once(':') {
                    route_slots.push(slot.trim().to_owned());
                }
            }
        }

        let (Some(method), Some(path)) = (method, path) else {
            continue;
        };
        entries.push(InspectExposeEntry {
            kind: "api",
            origin: format!("{feature_name}.api.{}", name),
            method,
            path,
            route_slots,
            audience,
            rate_limit_override,
        });
    }

    entries
}

/// Materialise the per-agent dispatch graph for `--expand=tools`.
/// Cross-feature resolution of effects / policies / PII lives in doctor;
/// this projection records the structural facts visible from the file
/// alone (kind, scope, derived effect from the local categorisation).
fn inspect_agent_tools_projection(agents: &[InspectAgent]) -> Vec<InspectAgentToolsEntry> {
    agents
        .iter()
        .filter(|agent| !agent.tools.is_empty())
        .map(|agent| InspectAgentToolsEntry {
            agent: agent.name.clone(),
            tools: agent
                .tools
                .iter()
                .map(|reference| tool_binding_for_reference(reference))
                .collect(),
        })
        .collect()
}

fn tool_binding_for_reference(reference: &str) -> InspectAgentToolBinding {
    let trimmed = reference.trim();
    if trimmed.starts_with("@tool.") {
        return InspectAgentToolBinding {
            reference: trimmed.to_owned(),
            kind: "adapter",
            scope: "adapter",
            derived_effect: "unknown",
        };
    }

    let segments: Vec<&str> = trimmed.split('.').collect();
    let (kind, scope) = match segments.as_slice() {
        ["query", "list", _] => ("query.list", "local"),
        ["query", "lookup", _] => ("query.lookup", "local"),
        ["query", "sql", _] => ("query.sql", "local"),
        ["query", "view", _] => ("query.view", "local"),
        ["query", _] => ("query", "local"),
        ["command", _] => ("command", "local"),
        ["api", _] => ("api", "local"),
        [_feature, "query", "list", _] => ("query.list", "cross_feature"),
        [_feature, "query", "lookup", _] => ("query.lookup", "cross_feature"),
        [_feature, "query", "sql", _] => ("query.sql", "cross_feature"),
        [_feature, "query", "view", _] => ("query.view", "cross_feature"),
        [_feature, "query", _] => ("query", "cross_feature"),
        [_feature, "command", _] => ("command", "cross_feature"),
        [_feature, "api", _] => ("api", "cross_feature"),
        _ => ("unknown", "unknown"),
    };

    let derived_effect = match kind {
        "command" => "write",
        "query.list" | "query.lookup" | "query.sql" | "query.view" | "query" => "read",
        _ => "unknown",
    };

    InspectAgentToolBinding {
        reference: trimmed.to_owned(),
        kind,
        scope,
        derived_effect,
    }
}

fn inspect_requirements(lines: &[String]) -> Vec<InspectRequirement> {
    let mut requirements = Vec::new();
    let mut in_requires_block = false;

    for line in lines {
        let trimmed = line.trim_start();
        let leading = leading_spaces(line);

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if leading == 2 {
            in_requires_block = trimmed == "requires";
            if let Some(requirement) = trimmed.strip_prefix("requires ") {
                if let Some(parsed) = parse_inspect_requirement(requirement, "requires inline") {
                    requirements.push(parsed);
                }
            }
            continue;
        }

        if leading <= 2 {
            in_requires_block = false;
        }

        if in_requires_block && leading == 4 {
            if let Some(parsed) = parse_inspect_requirement(trimmed, "requires block") {
                requirements.push(parsed);
            }
        }
    }

    requirements
}

fn inspect_external_calls(feature: &str, lines: &[String]) -> Vec<InspectExternalCall> {
    let mut calls = Vec::new();
    let mut index = 0;

    while index < lines.len() {
        let trimmed = lines[index].trim_start();
        let leading = leading_spaces(&lines[index]);

        if leading == 2 && (trimmed.starts_with("command ") || trimmed.starts_with("job ")) {
            let (kind, name) = if let Some(name) = named_block_name(trimmed, "command") {
                ("command", name)
            } else if let Some(name) = named_block_name(trimmed, "job") {
                ("job", name)
            } else {
                index += 1;
                continue;
            };

            let start = index;
            index += 1;
            while index < lines.len() && leading_spaces(&lines[index]) > 2 {
                index += 1;
            }

            calls.extend(inspect_external_calls_in_block(
                feature,
                kind,
                name,
                &lines[start..index],
            ));
        } else {
            index += 1;
        }
    }

    calls
}

fn inspect_external_calls_in_block(
    feature: &str,
    kind: &str,
    name: &str,
    lines: &[String],
) -> Vec<InspectExternalCall> {
    let timeout = block_scalar_value(lines, "timeout").map(strip_quotes);
    let retry = block_scalar_value(lines, "retry").map(str::to_owned);
    let idempotency = block_prefixed_value(lines, "idempotency by ").map(str::to_owned);
    let audit = block_has_exact_line(lines, "audit required");
    let subject = format!("{feature}.{kind}.{name}");
    let mut calls = Vec::new();
    let mut index = 1;

    while index < lines.len() {
        let trimmed = lines[index].trim_start();

        if leading_spaces(&lines[index]) == 4
            && let Some((slot, operation)) = parse_external_call_header(trimmed)
        {
            let mut args = Vec::new();
            index += 1;

            while index < lines.len() && leading_spaces(&lines[index]) > 4 {
                let child = lines[index].trim_start();
                if leading_spaces(&lines[index]) == 6
                    && let Some((name, value)) = child.split_once('=')
                {
                    args.push(InspectCallArg {
                        name: name.trim().to_owned(),
                        value: value.trim().to_owned(),
                    });
                }
                index += 1;
            }

            calls.push(InspectExternalCall {
                subject: subject.clone(),
                slot: slot.to_owned(),
                operation: operation.to_owned(),
                args,
                timeout: timeout.clone(),
                retry: retry.clone(),
                idempotency: idempotency.clone(),
                audit,
                origin: "calls",
            });
        } else {
            index += 1;
        }
    }

    calls
}

fn parse_external_call_header(trimmed: &str) -> Option<(&str, &str)> {
    let rest = trimmed.strip_prefix("calls ")?;
    let (slot, operation) = rest.trim().split_once('.')?;
    let slot = slot.trim();
    let operation = operation.trim();

    if is_identifier(slot) && is_identifier(operation) {
        Some((slot, operation))
    } else {
        None
    }
}

fn named_block_name<'a>(trimmed: &'a str, keyword: &str) -> Option<&'a str> {
    let rest = trimmed.strip_prefix(keyword)?.trim_start();
    let name = rest.split_whitespace().next()?;
    is_identifier(name).then_some(name)
}

fn block_scalar_value<'a>(lines: &'a [String], keyword: &str) -> Option<&'a str> {
    lines.iter().skip(1).find_map(|line| {
        (leading_spaces(line) == 4)
            .then(|| line.trim_start().strip_prefix(keyword))
            .flatten()
            .map(str::trim)
            .filter(|value| !value.is_empty())
    })
}

fn block_prefixed_value<'a>(lines: &'a [String], prefix: &str) -> Option<&'a str> {
    lines.iter().skip(1).find_map(|line| {
        (leading_spaces(line) == 4)
            .then(|| line.trim_start().strip_prefix(prefix))
            .flatten()
            .map(str::trim)
            .filter(|value| !value.is_empty())
    })
}

fn block_has_exact_line(lines: &[String], expected: &str) -> bool {
    lines
        .iter()
        .skip(1)
        .any(|line| leading_spaces(line) == 4 && line.trim_start() == expected)
}

fn strip_quotes(value: &str) -> String {
    value
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or(value)
        .to_owned()
}

fn parse_inspect_requirement(source: &str, origin: &'static str) -> Option<InspectRequirement> {
    let rest = source.trim().strip_prefix("integration ")?;
    let (name, contract) = rest.split_once(':')?;
    let name = name.trim();
    let contract = contract.trim();

    if !is_identifier(name) || !is_type_name(contract) {
        return None;
    }

    Some(InspectRequirement {
        kind: "integration".to_owned(),
        name: name.to_owned(),
        contract: contract.to_owned(),
        origin,
    })
}

fn inspect_refs(lines: &[String]) -> InspectRefs {
    let declared = collect_declared_ref_groups(lines);
    let declared_namespaces: BTreeSet<String> = declared
        .iter()
        .flat_map(|group| group.namespaces.iter().cloned())
        .collect();
    let used_namespaces = collect_used_namespaces(lines);
    let used: Vec<String> = used_namespaces.iter().cloned().collect();
    let (missing, unused) = if declared_namespaces.is_empty() {
        (Vec::new(), Vec::new())
    } else {
        (
            used_namespaces
                .difference(&declared_namespaces)
                .cloned()
                .collect(),
            declared_namespaces
                .difference(&used_namespaces)
                .cloned()
                .collect(),
        )
    };

    InspectRefs {
        declared,
        used,
        missing,
        unused,
    }
}

fn collect_declared_ref_groups(lines: &[String]) -> Vec<InspectRefGroup> {
    let mut groups = Vec::new();
    let mut in_refs = false;

    for line in lines {
        let trimmed = line.trim_start();

        if leading_spaces(line) == 2 {
            in_refs = trimmed == "refs";
            continue;
        }

        if !in_refs || leading_spaces(line) != 4 || trimmed.is_empty() {
            continue;
        }

        let Some((group, namespaces)) = trimmed.split_once(':') else {
            continue;
        };

        groups.push(InspectRefGroup {
            group: group.trim().to_owned(),
            namespaces: namespaces
                .split(',')
                .map(str::trim)
                .filter(|namespace| namespace.starts_with('@') && !namespace.is_empty())
                .map(str::to_owned)
                .collect(),
            origin: "authored",
        });
    }

    groups
}

fn collect_used_namespaces(lines: &[String]) -> BTreeSet<String> {
    let mut namespaces = BTreeSet::new();
    let mut current_top = None;

    for line in lines {
        let trimmed = line.trim_start();

        if leading_spaces(line) == 2 {
            current_top = trimmed.split_whitespace().next();
        }

        if current_top == Some("refs") || trimmed.starts_with('#') {
            continue;
        }

        for namespace in namespace_references(line) {
            namespaces.insert(format!("@{namespace}"));
        }
    }

    namespaces
}

fn inspect_summary(lines: &[String]) -> InspectSummary {
    let resources = collect_resource_names(lines);
    let records = collect_record_names(lines);
    let queries = collect_query_names(lines);
    let events = collect_event_names(lines);
    let anchors = collect_view_anchors(lines);
    let mut types = resources.clone();
    types.extend(records.clone());

    InspectSummary {
        provides: InspectProvides {
            types,
            queries: queries.clone(),
            events: events.clone(),
            anchors: anchors.clone(),
        },
        resources,
        records,
        queries,
        commands: collect_command_names(lines),
        workflows: collect_workflow_summaries(lines),
        jobs: collect_named_top_blocks(lines, "job"),
        webhooks: collect_named_top_blocks(lines, "webhook"),
        events,
        surfaces: collect_surface_names(lines),
        anchors,
        extends: collect_extends_anchors(lines),
        extended_by: collect_extensible_by_features(lines),
    }
}

fn inspect_locators(lines: &[String]) -> Vec<InspectLocators> {
    let mut locators = Vec::new();
    let has_id_lookup = feature_has_id_lookup(lines);

    for block in query_blocks(lines) {
        let name = query_name(block[0].trim_start()).unwrap_or("unknown");
        let inferred = query_kind(block);
        let mut bindings = vec![inspect_binding(
            "ctx.*",
            "runtime",
            "request and tenant execution context",
        )];

        for param in query_param_names(block) {
            bindings.push(inspect_binding(
                format!("params.{param}"),
                "query.params",
                "read argument declared by this query",
            ));
        }

        locators.push(InspectLocators {
            subject: format!("query.{name}"),
            kind: format!("query.{inferred}"),
            bindings,
        });
    }

    for block in command_blocks(lines) {
        let name = command_name(block[0].trim_start()).unwrap_or("unknown");
        let mut bindings = vec![inspect_binding(
            "ctx.*",
            "runtime",
            "request and tenant execution context",
        )];

        for route in command_route_names(block) {
            bindings.push(inspect_binding(
                format!("route.{route}"),
                "command.route",
                "path or caller-context locator declared by this command",
            ));
        }

        for input in command_input_names(block) {
            bindings.push(inspect_binding(
                format!("input.{input}"),
                "command.input",
                "submitted command body field",
            ));
        }

        if let Some(target) = direct_child_value(block, "target ") {
            bindings.push(inspect_binding(
                "target",
                format!("explicit target {target}"),
                "entity loaded before declarative command effects",
            ));
        } else if has_id_lookup && command_needs_inferred_target(block) {
            bindings.push(inspect_binding(
                "target",
                "inferred local query.by_id(id: route.id)",
                "entity loaded before declarative command effects",
            ));
        }

        locators.push(InspectLocators {
            subject: format!("command.{name}"),
            kind: "command".to_owned(),
            bindings,
        });
    }

    for block in top_level_blocks(lines, "job ") {
        let name = named_top_block_name(block[0].trim_start()).unwrap_or("unknown");
        let trigger = direct_child_value(block, "trigger ");
        let mut bindings = vec![inspect_binding("ctx.*", "runtime", "job execution context")];
        let kind = if trigger
            .as_deref()
            .is_some_and(|trigger| trigger.starts_with("event "))
        {
            bindings.push(inspect_binding(
                "envelope.*",
                "event trigger",
                "event-bus metadata such as envelope.id",
            ));
            bindings.push(inspect_binding(
                "payload.*",
                "event trigger",
                "producer event payload fields",
            ));
            "event_job"
        } else if trigger
            .as_deref()
            .is_some_and(|trigger| trigger.starts_with("schedule "))
        {
            bindings.push(inspect_binding(
                "schedule.*",
                "schedule trigger",
                "scheduler metadata such as run time",
            ));
            "schedule_job"
        } else {
            "job"
        };

        if let Some(target) = direct_child_value(block, "target ") {
            bindings.push(inspect_binding(
                "target",
                format!("explicit target {target}"),
                "entity loaded before declarative job effects",
            ));
        }

        locators.push(InspectLocators {
            subject: format!("job.{name}"),
            kind: kind.to_owned(),
            bindings,
        });
    }

    for block in top_level_blocks(lines, "webhook ") {
        let name = named_top_block_name(block[0].trim_start()).unwrap_or("unknown");
        locators.push(InspectLocators {
            subject: format!("webhook.{name}"),
            kind: "webhook".to_owned(),
            bindings: vec![
                inspect_binding(
                    "payload.*",
                    "webhook payload",
                    "verified inbound request body fields",
                ),
                inspect_binding("ctx.*", "runtime", "webhook execution context"),
            ],
        });
    }

    for block in top_level_blocks(lines, "rule ") {
        let name = block[0]
            .trim_start()
            .trim_start_matches("rule ")
            .trim_matches('"');
        locators.push(InspectLocators {
            subject: format!("rule.{name}"),
            kind: "rule".to_owned(),
            bindings: vec![
                inspect_binding(
                    "self",
                    "rule target snapshot",
                    "resource snapshot evaluated by the rule predicate",
                ),
                inspect_binding("ctx.*", "runtime", "request and tenant execution context"),
            ],
        });
    }

    locators
}

fn inspect_dependencies(lines: &[String]) -> Vec<InspectDependency> {
    let feature = lines
        .first()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("unknown");
    let mut dependencies = Vec::new();

    for line in lines {
        let trimmed = line.trim_start();
        if leading_spaces(line) == 2 && trimmed.starts_with("uses ") {
            for target in parse_ident_list(trimmed.trim_start_matches("uses ")) {
                dependencies.push(inspect_dependency("uses", feature, target, "uses"));
            }
        } else if leading_spaces(line) == 2 && trimmed.starts_with("extends @anchor.") {
            if let Some(anchor) = trimmed.split_whitespace().nth(1) {
                dependencies.push(inspect_dependency(
                    "extends_anchor",
                    feature,
                    anchor,
                    "extends",
                ));
            }
        }
    }

    for block in command_blocks(lines) {
        let name = command_name(block[0].trim_start()).unwrap_or("unknown");
        let subject = format!("{feature}.command.{name}");
        dependencies.extend(emits_dependencies(feature, &subject, block));
        dependencies.extend(query_reference_dependencies(&subject, block));
    }

    for block in top_level_blocks(lines, "workflow ") {
        let name = named_top_block_name(block[0].trim_start()).unwrap_or("unknown");
        let subject = format!("{feature}.workflow.{name}");
        dependencies.extend(emits_dependencies(feature, &subject, block));
    }

    for block in top_level_blocks(lines, "job ") {
        let name = named_top_block_name(block[0].trim_start()).unwrap_or("unknown");
        let subject = format!("{feature}.job.{name}");
        if let Some(trigger) = direct_child_value(block, "trigger ") {
            if let Some(event) = trigger.strip_prefix("event ") {
                dependencies.push(inspect_dependency(
                    "trigger_event",
                    subject.clone(),
                    qualify_event_ref(feature, event.trim()),
                    "job.trigger",
                ));
            }
        }
        dependencies.extend(emits_dependencies(feature, &subject, block));
        dependencies.extend(query_reference_dependencies(&subject, block));
    }

    for block in top_level_blocks(lines, "webhook ") {
        let name = named_top_block_name(block[0].trim_start()).unwrap_or("unknown");
        let subject = format!("{feature}.webhook.{name}");
        dependencies.extend(emits_dependencies(feature, &subject, block));
    }

    dependencies
}

fn inspect_security(lines: &[String]) -> InspectSecurity {
    InspectSecurity {
        fields: inspect_security_fields(lines),
        event_payloads: inspect_security_event_payloads(lines),
        operations: inspect_security_operations(lines),
        webhooks: inspect_security_webhooks(lines),
    }
}

fn inspect_security_fields(lines: &[String]) -> Vec<InspectSecurityField> {
    let mut fields = Vec::new();
    let mut current_resource: Option<String> = None;

    for line in lines {
        let trimmed = line.trim_start();

        if leading_spaces(line) == 4 && trimmed.starts_with("resource ") {
            current_resource = trimmed.split_whitespace().nth(1).map(str::to_owned);
            continue;
        }

        if leading_spaces(line) <= 4 && !trimmed.is_empty() {
            if !trimmed.starts_with("resource ") {
                current_resource = None;
            }
            continue;
        }

        if leading_spaces(line) == 6 {
            let Some(resource) = current_resource.as_deref() else {
                continue;
            };
            let Some(field) = field_name_from_typed_line(trimmed) else {
                continue;
            };
            let markers: Vec<_> = security_markers(line).collect();
            if markers.is_empty() {
                continue;
            }

            fields.push(InspectSecurityField {
                resource: resource.to_owned(),
                field: field.to_owned(),
                markers,
                origin: "field",
            });
        }
    }

    fields
}

fn inspect_security_event_payloads(lines: &[String]) -> Vec<InspectSecurityEventPayload> {
    let mut payloads = Vec::new();

    for event in collect_event_decls(lines) {
        for field_line in event.payload {
            let Some(field) = field_name_from_typed_line(&field_line) else {
                continue;
            };
            let markers: Vec<_> = security_markers(&field_line).collect();
            if markers.is_empty() {
                continue;
            }

            payloads.push(InspectSecurityEventPayload {
                event: event.name.clone(),
                field: field.to_owned(),
                markers,
                origin: "event",
            });
        }
    }

    payloads
}

fn inspect_security_operations(lines: &[String]) -> Vec<InspectSecurityOperation> {
    let mut operations = Vec::new();

    for block in query_blocks(lines) {
        let name = query_name(block[0].trim_start()).unwrap_or("unknown");
        let policy = direct_child_value(block, "policy ");
        let scope_reason = scope_override_reason(block);
        let scope_override = block
            .iter()
            .any(|line| line.trim_start().starts_with("scope override"));
        let rate_limits = direct_child_values(block, "rate_limit ");
        let audit = parse_audit(block, "query");

        if policy.is_some() || scope_override || !rate_limits.is_empty() || audit.is_some() {
            operations.push(InspectSecurityOperation {
                subject: format!("query.{name}"),
                policy,
                tenant_from: None,
                scope_reason,
                rate_limits,
                scope_override,
                audit,
                origin: "query",
            });
        }
    }

    for block in command_blocks(lines) {
        let name = command_name(block[0].trim_start()).unwrap_or("unknown");
        let policy = direct_child_value(block, "policy ");
        let rate_limits = direct_child_values(block, "rate_limit ");
        let audit = parse_audit(block, "command");

        if policy.is_some() || !rate_limits.is_empty() || audit.is_some() {
            operations.push(InspectSecurityOperation {
                subject: format!("command.{name}"),
                policy,
                tenant_from: None,
                scope_reason: None,
                rate_limits,
                scope_override: false,
                audit,
                origin: "command",
            });
        }
    }

    for block in top_level_blocks(lines, "job ") {
        let name = named_top_block_name(block[0].trim_start()).unwrap_or("unknown");
        let policy = direct_child_value(block, "policy ");
        let tenant_from = direct_child_value(block, "tenant_from ");
        let rate_limits = direct_child_values(block, "rate_limit ");
        let audit = parse_audit(block, "job");

        if policy.is_some() || tenant_from.is_some() || !rate_limits.is_empty() || audit.is_some() {
            operations.push(InspectSecurityOperation {
                subject: format!("job.{name}"),
                policy,
                tenant_from,
                scope_reason: None,
                rate_limits,
                scope_override: false,
                audit,
                origin: "job",
            });
        }
    }

    for block in top_level_blocks(lines, "webhook ") {
        let name = named_top_block_name(block[0].trim_start()).unwrap_or("unknown");
        let policy = direct_child_value(block, "policy ");
        let rate_limits = direct_child_values(block, "rate_limit ");
        let audit = parse_audit(block, "webhook");

        if policy.is_some() || !rate_limits.is_empty() || audit.is_some() {
            operations.push(InspectSecurityOperation {
                subject: format!("webhook.{name}"),
                policy,
                tenant_from: None,
                scope_reason: None,
                rate_limits,
                scope_override: false,
                audit,
                origin: "webhook",
            });
        }
    }

    operations
}

fn scope_override_reason(lines: &[String]) -> Option<String> {
    let mut in_scope_override = false;

    for line in lines {
        let trimmed = line.trim_start();
        let indent = leading_spaces(line);

        if indent == 6 && trimmed.starts_with("scope override") {
            in_scope_override = true;
            continue;
        }

        if in_scope_override && indent <= 6 && !trimmed.is_empty() {
            in_scope_override = false;
        }

        if in_scope_override && indent == 8 {
            if let Some(reason) = trimmed.strip_prefix("reason ") {
                return Some(reason.trim().to_owned());
            }
        }
    }

    None
}

fn inspect_security_webhooks(lines: &[String]) -> Vec<InspectSecurityWebhook> {
    let mut webhooks = Vec::new();

    for block in top_level_blocks(lines, "webhook ") {
        let name = named_top_block_name(block[0].trim_start()).unwrap_or("unknown");
        let verify = direct_child_value(block, "verify ").unwrap_or_else(|| "missing".to_owned());
        let secrets = block
            .iter()
            .filter_map(|line| {
                if leading_spaces(line) == 6 {
                    line.trim_start()
                        .strip_prefix("secret ")
                        .map(str::trim)
                        .map(str::to_owned)
                } else {
                    None
                }
            })
            .collect();

        webhooks.push(InspectSecurityWebhook {
            webhook: name.to_owned(),
            verify,
            secrets,
            origin: "webhook.verify",
        });
    }

    webhooks
}

fn inspect_binding(
    name: impl Into<String>,
    origin: impl Into<String>,
    meaning: impl Into<String>,
) -> InspectBinding {
    InspectBinding {
        name: name.into(),
        origin: origin.into(),
        meaning: meaning.into(),
    }
}

fn inspect_dependency(
    kind: impl Into<String>,
    from: impl Into<String>,
    to: impl Into<String>,
    origin: impl Into<String>,
) -> InspectDependency {
    InspectDependency {
        kind: kind.into(),
        from: from.into(),
        to: to.into(),
        origin: origin.into(),
    }
}

/// Phase L Tier 4a — `--expand=defaults` projection. Reads the lifted
/// `Feature.defaults` block from the Tier 3 slice when available
/// (canonical-indent code path), and falls back to the text-pattern
/// walker only for legacy documents that did not lower through
/// `parse_feature_skeletons`. The query-derived language defaults
/// (`query_order`, `query_filter_index`) stay text-derived because
/// those facts originate from CLI heuristics over query bodies, not
/// from feature-state IR.
fn inspect_defaults(
    lines: &[String],
    tier3: Option<&Tier3FeatureSlice>,
) -> Vec<InspectDefault> {
    let mut defaults = match tier3 {
        Some(slice) => project_defaults_from_ir(slice, lines),
        None => inspect_defaults_legacy(lines),
    };

    for query in query_blocks(lines) {
        let header = query[0].trim_start();
        if !header.starts_with("query.list ") {
            continue;
        }
        if direct_child_value(query, "order ").is_some() {
            continue;
        }
        let name = query_name(header).unwrap_or("unknown");
        defaults.push(InspectDefault {
            name: "query_order".to_owned(),
            value: "created_at desc".to_owned(),
            origin: "language default",
            applies_to: vec![format!("query.{name}")],
        });
    }

    for generated in collect_query_filter_indexes(lines) {
        defaults.push(InspectDefault {
            name: "query_filter_index".to_owned(),
            value: generated.value,
            origin: "language default",
            applies_to: vec![
                format!("query.{}", generated.query),
                format!("filter.{}", generated.filter),
            ],
        });
    }

    defaults
}

/// Phase L Tier 4a — project `Feature.defaults` from IR into the
/// `InspectDefault` shape. `applies_to` for `tenancy`/`timestamps`
/// reads `Tier3FeatureSlice.resource_names`; for `policy`, it
/// retains the text walker over jobs/webhooks until those names are
/// also lifted to the slice (Tier 4 follow-up).
fn project_defaults_from_ir(
    slice: &Tier3FeatureSlice,
    lines: &[String],
) -> Vec<InspectDefault> {
    let mut out = Vec::new();

    if slice.defaults.timestamps {
        out.push(InspectDefault {
            name: "timestamps".to_owned(),
            value: "true".to_owned(),
            origin: "defaults",
            applies_to: slice.resource_names.clone(),
        });
    }

    if let Some(tenancy) = &slice.defaults.tenancy {
        let value = match tenancy {
            lazuli_ir::Tenancy::Org => "org".to_owned(),
            lazuli_ir::Tenancy::Team => "team".to_owned(),
            lazuli_ir::Tenancy::Custom(name) => name.clone(),
            lazuli_ir::Tenancy::None => "none".to_owned(),
        };
        out.push(InspectDefault {
            name: "tenancy".to_owned(),
            value,
            origin: "defaults",
            applies_to: slice.resource_names.clone(),
        });
    }

    // `policy` and `policy_for` retain text-derived `applies_to` until
    // jobs/webhooks have their names lifted to the slice. The IR
    // `Defaults.policy` carries the typed atom; the `applies_to`
    // projection mirrors the legacy text walker for now to keep the
    // projection JSON shape stable.
    let mut in_defaults = false;
    for line in lines {
        let trimmed = line.trim_start();
        if leading_spaces(line) == 2 {
            in_defaults = trimmed == "defaults";
            continue;
        }
        if !in_defaults || leading_spaces(line) != 4 || trimmed.is_empty() {
            continue;
        }
        if let Some(value) = trimmed.strip_prefix("policy_for ") {
            if let Some((scopes, policy)) = value.split_once(':') {
                out.push(InspectDefault {
                    name: "policy_for".to_owned(),
                    value: policy.trim().to_owned(),
                    origin: "defaults",
                    applies_to: collect_policy_for_applies_to(lines, scopes),
                });
            }
        } else if let Some(value) = trimmed.strip_prefix("policy ") {
            out.push(InspectDefault {
                name: "policy".to_owned(),
                value: value.to_owned(),
                origin: "defaults",
                applies_to: collect_job_and_webhook_names(lines),
            });
        }
    }

    out
}

/// Legacy text-pattern fallback. Retained for documents that don't
/// lower through `parse_feature_skeletons` (no Tier 3 slice). The
/// canonical-indent code path routes through `project_defaults_from_ir`.
fn inspect_defaults_legacy(lines: &[String]) -> Vec<InspectDefault> {
    let resources = collect_resource_names(lines);
    let mut defaults = Vec::new();
    let mut in_defaults = false;

    for line in lines {
        let trimmed = line.trim_start();

        if leading_spaces(line) == 2 {
            in_defaults = trimmed == "defaults";
            continue;
        }

        if !in_defaults || leading_spaces(line) != 4 || trimmed.is_empty() {
            continue;
        }

        if trimmed == "timestamps" {
            defaults.push(InspectDefault {
                name: "timestamps".to_owned(),
                value: "true".to_owned(),
                origin: "defaults",
                applies_to: resources.clone(),
            });
        } else if let Some(value) = trimmed.strip_prefix("tenancy ") {
            defaults.push(InspectDefault {
                name: "tenancy".to_owned(),
                value: value.to_owned(),
                origin: "defaults",
                applies_to: resources.clone(),
            });
        } else if let Some(value) = trimmed.strip_prefix("policy_for ") {
            if let Some((scopes, policy)) = value.split_once(':') {
                defaults.push(InspectDefault {
                    name: "policy_for".to_owned(),
                    value: policy.trim().to_owned(),
                    origin: "defaults",
                    applies_to: collect_policy_for_applies_to(lines, scopes),
                });
            }
        } else if let Some(value) = trimmed.strip_prefix("policy ") {
            defaults.push(InspectDefault {
                name: "policy".to_owned(),
                value: value.to_owned(),
                origin: "defaults",
                applies_to: collect_job_and_webhook_names(lines),
            });
        }
    }

    defaults
}

struct GeneratedFilterIndex {
    query: String,
    filter: String,
    value: String,
}

fn collect_query_filter_indexes(lines: &[String]) -> Vec<GeneratedFilterIndex> {
    let tenancy_axis = single_tenancy_axis(lines);
    let mut seen = BTreeSet::new();
    let mut indexes = Vec::new();

    for query in query_blocks(lines) {
        let header = query[0].trim_start();
        if !header.starts_with("query.list ") || query_has_scope_override(query) {
            continue;
        }
        let name = query_name(header).unwrap_or("unknown");

        for field in query_filter_index_fields(query) {
            let value = tenancy_axis
                .as_ref()
                .map(|tenant| format!("{tenant}, {field}"))
                .unwrap_or_else(|| field.clone());

            if seen.insert(value.clone()) {
                indexes.push(GeneratedFilterIndex {
                    query: name.to_owned(),
                    filter: field,
                    value,
                });
            }
        }
    }

    indexes
}

fn single_tenancy_axis(lines: &[String]) -> Option<String> {
    let axes: BTreeSet<String> = lines
        .iter()
        .filter_map(|line| {
            let trimmed = line.trim_start();
            let axis = trimmed.strip_prefix("tenancy ")?.trim();
            (!axis.is_empty() && axis != "none").then(|| axis.to_owned())
        })
        .collect();

    (axes.len() == 1).then(|| axes.into_iter().next()).flatten()
}

fn query_has_scope_override(query: &[String]) -> bool {
    query
        .iter()
        .any(|line| line.trim_start() == "scope override")
}

fn query_filter_index_fields(query: &[String]) -> Vec<String> {
    let mut fields = Vec::new();
    let mut in_filters = false;

    for line in query.iter().skip(1) {
        let trimmed = line.trim_start();
        let leading = leading_spaces(line);

        if trimmed.is_empty() {
            continue;
        }

        if leading == 6 {
            in_filters = trimmed == "filters";
            continue;
        }

        if in_filters
            && leading == 8
            && let Some(field) = filter_index_field(trimmed)
        {
            fields.push(field);
        }
    }

    fields
}

fn filter_index_field(filter: &str) -> Option<String> {
    if filter.contains(" has ")
        || filter.contains(" != ")
        || filter.contains(" = nil")
        || filter.contains(" != nil")
    {
        return None;
    }

    if let Some((field, param)) = filter.split_once(" when ") {
        let field = field.trim();
        let param = param.trim().strip_prefix("params.")?;
        if is_identifier(field) && field == param {
            return Some(field.to_owned());
        }
        return None;
    }

    if let Some((left, right)) = filter.split_once(" = ") {
        let left = left.trim();
        let param = right.trim().strip_prefix("params.")?;

        if is_identifier(left) && left == param {
            return Some(left.to_owned());
        }

        if let Some(relation) = left.strip_suffix(".id")
            && is_identifier(relation)
            && param == format!("{relation}_id")
        {
            return Some(relation.to_owned());
        }
    }

    None
}

fn collect_policy_for_applies_to(lines: &[String], scopes: &str) -> Vec<String> {
    let mut applies_to = Vec::new();

    for scope in parse_ident_list(scopes) {
        match scope.as_str() {
            "jobs" => applies_to.extend(collect_named_top_blocks(lines, "job ")),
            "webhooks" => applies_to.extend(collect_named_top_blocks(lines, "webhook ")),
            _ => {}
        }
    }

    applies_to
}

fn inspect_built_in_trace_events() -> Vec<InspectBuiltInTraceEvent> {
    lazuli_ir::built_in_trace_events()
        .into_iter()
        .map(|event| InspectBuiltInTraceEvent {
            name: event.name,
            fires_per: built_in_trace_fires_per_word(event.fires_per).to_owned(),
            payload: event
                .payload
                .into_iter()
                .map(|f| InspectBuiltInTraceField {
                    name: f.name,
                    type_text: format_type_ref(&f.type_ref),
                    optional: f.optional,
                })
                .collect(),
        })
        .collect()
}

fn built_in_trace_fires_per_word(kind: lazuli_ir::TraceFiresPer) -> &'static str {
    match kind {
        lazuli_ir::TraceFiresPer::AgentDispatch => "agent_dispatch",
        lazuli_ir::TraceFiresPer::CommandDispatch => "command_dispatch",
        lazuli_ir::TraceFiresPer::FlowStep => "flow_step",
        lazuli_ir::TraceFiresPer::JobInvocation => "job_invocation",
        lazuli_ir::TraceFiresPer::WebhookDelivery => "webhook_delivery",
    }
}

fn format_type_ref(t: &lazuli_ir::TypeRef) -> String {
    use lazuli_ir::{BuiltinType, CapabilityRef, TypeRef};
    match t {
        TypeRef::Builtin(BuiltinType::SemanticMoney { currency }) => {
            format!("@semantic.Money(currency:{})", currency.as_iso())
        }
        // B3 — surface plugin-contributed `@semantic.<Name>` back as
        // the authored alias so inspect-text renderings stay stable.
        TypeRef::Builtin(BuiltinType::SemanticPluginType { name, .. }) => {
            format!("@semantic.{}", name)
        }
        TypeRef::Builtin(b) => match b {
            BuiltinType::Text => "Text",
            BuiltinType::Integer => "Integer",
            BuiltinType::Boolean => "Boolean",
            BuiltinType::Decimal => "Decimal",
            BuiltinType::Date => "Date",
            BuiltinType::DateTime => "DateTime",
            BuiltinType::Id => "ID",
            BuiltinType::Json => "Json",
            BuiltinType::SemanticEmail => "@semantic.Email",
            BuiltinType::SemanticPhone => "@semantic.Phone",
            BuiltinType::SemanticUrl => "@semantic.Url",
            BuiltinType::SemanticUuid => "@semantic.Uuid",
            // SemanticMoney + SemanticPluginType handled above.
            BuiltinType::SemanticMoney { .. } => unreachable!(),
            BuiltinType::SemanticPluginType { .. } => unreachable!(),
            BuiltinType::SemanticCurrency => "@semantic.Currency",
            BuiltinType::SemanticGeoPoint => "@semantic.GeoPoint",
            BuiltinType::CapSecret => "@cap.Secret",
            BuiltinType::CapFile => "@cap.File",
        }
        .to_owned(),
        TypeRef::UserDefined(qn) | TypeRef::EnumRef(qn) => qn.name.clone(),
        TypeRef::Many(inner) => format!("{}*", format_type_ref(inner)),
        TypeRef::Unresolved(text) => text.clone(),
        // Phase L Tier 2 — render the typed capability back into the
        // canonical source form so inspect summary lines stay readable.
        TypeRef::Capability(CapabilityRef::File(file)) => format_file_capability(file),
        TypeRef::Capability(CapabilityRef::Hashed(h)) => format_hashed_capability(h),
        TypeRef::Capability(CapabilityRef::Encrypted(e)) => format_encrypted_capability(e),
        TypeRef::Capability(CapabilityRef::E2ee(e)) => format_e2ee_capability(e),
        TypeRef::Capability(CapabilityRef::Token(t)) => format_token_capability(t),
        TypeRef::Capability(CapabilityRef::PII(pii)) => format_pii_capability(pii),
    }
}

fn format_pii_capability(pii: &lazuli_ir::PiiCapability) -> String {
    let mut args = vec![format!("class:{}", pii.class)];
    if let Some(retention) = pii.retention.as_ref() {
        args.push(format!("retention:{}", retention));
    }
    if let Some(log_redact) = pii.log_redact {
        args.push(format!("log_redact:{}", log_redact));
    }
    format!("@cap.PII({})", args.join(","))
}

/// Encryption bucket cycle — render `E2eeCapability` back to source form.
fn format_e2ee_capability(e: &lazuli_ir::E2eeCapability) -> String {
    format!("@cap.E2ee(key:{})", e.key)
}

/// Phase L Tier 4 follow-up — render `HashedCapability` back to source form.
fn format_hashed_capability(h: &lazuli_ir::HashedCapability) -> String {
    let alg = match h.algorithm {
        lazuli_ir::HashAlgorithm::Argon2id => "argon2id",
        lazuli_ir::HashAlgorithm::Bcrypt => "bcrypt",
    };
    format!("@cap.Hashed(algorithm:{alg})")
}

fn format_encrypted_capability(e: &lazuli_ir::EncryptedCapability) -> String {
    format!("@cap.Encrypted(key:{})", e.key)
}

fn format_token_capability(t: &lazuli_ir::TokenCapability) -> String {
    let store = match t.store {
        lazuli_ir::TokenStore::Hashed => "hashed",
    };
    format!(
        "@cap.Token(ttl:{},single_use:{},store:{})",
        t.ttl, t.single_use, store
    )
}

/// Render a `FileCapability` back into the `@cap.File(...)` source form.
/// Used by both `format_type_ref` and the `--expand=storage` projection.
fn format_file_capability(file: &lazuli_ir::FileCapability) -> String {
    let mut parts: Vec<String> = Vec::new();
    parts.push(format!(
        "max_size:{}",
        format_file_size_literal(file.max_size.literal)
    ));
    let accept = file
        .accept
        .iter()
        .map(|m| format!("{}/{}", m.family, m.subtype))
        .collect::<Vec<_>>()
        .join("|");
    parts.push(format!("accept:{accept}"));
    if let Some(v) = file.visibility {
        parts.push(format!("visibility:{}", format_file_visibility(v)));
    }
    if let Some(ttl) = file.signed_ttl.as_deref() {
        parts.push(format!("signed_ttl:{ttl}"));
    }
    format!("@cap.File({})", parts.join(","))
}

fn format_file_size_literal(literal: lazuli_ir::FileSizeLiteral) -> String {
    use lazuli_ir::FileSizeLiteral::*;
    match literal {
        Kb(n) => format!("{n}kb"),
        Mb(n) => format!("{n}mb"),
        Gb(n) => format!("{n}gb"),
    }
}

fn format_file_visibility(visibility: lazuli_ir::FileVisibility) -> &'static str {
    use lazuli_ir::FileVisibility::*;
    match visibility {
        Public => "public",
        Private => "private",
        Signed => "signed",
    }
}

fn inspect_events(lines: &[String]) -> Vec<InspectEvent> {
    let event_groups = collect_event_groups(lines);
    collect_event_decls(lines)
        .into_iter()
        .map(|event| {
            let mut payload = Vec::new();
            for group in &event_groups {
                if event.name.starts_with(&group.prefix) {
                    for entry in &group.payload {
                        payload.push(inspect_inherited_payload_field(
                            entry,
                            format!("event_group:{}", group.pattern),
                        ));
                    }
                }
            }

            for field in &event.payload {
                if let Some(field) = inspect_explicit_payload_field(field, &event.name) {
                    payload.push(field);
                }
            }

            InspectEvent {
                name: event.name,
                payload,
            }
        })
        .collect()
}

fn inspect_targets(lines: &[String]) -> Vec<InspectTarget> {
    let mut targets = Vec::new();
    let has_id_lookup = feature_has_id_lookup(lines);

    for command in command_blocks(lines) {
        let name = command_name(command[0].trim_start()).unwrap_or("unknown");
        let explicit = command.iter().find_map(|line| {
            if leading_spaces(line) == 4 {
                line.trim_start().strip_prefix("target ").map(str::to_owned)
            } else {
                None
            }
        });

        if let Some(target) = explicit {
            targets.push(InspectTarget {
                command: name.to_owned(),
                target,
                origin: "explicit".to_owned(),
            });
        } else if has_id_lookup && command_needs_inferred_target(command) {
            targets.push(InspectTarget {
                command: name.to_owned(),
                target: "query.by_id(id: route.id)".to_owned(),
                origin: "inferred from local route id and query.lookup by_id".to_owned(),
            });
        }
    }

    targets
}

fn inspect_policies(
    lines: &[String],
    policy_atoms: &BTreeMap<String, Vec<String>>,
    tier3: Option<&Tier3FeatureSlice>,
) -> Vec<InspectPolicy> {
    let mut policies = Vec::new();

    // IR Error-Vocab (Cell PARSE-1) — build name -> when_denied
    // lookups so the text walker can attach the per-command override
    // (resolution-chain step 1) and the per-policy default
    // (resolution-chain step 2) onto each InspectPolicy row.
    let command_when_denied: BTreeMap<String, String> = tier3
        .map(|t| {
            t.commands
                .iter()
                .filter_map(|c| {
                    c.policy_when_denied
                        .as_ref()
                        .map(|k| (c.name.clone(), k.key.clone()))
                })
                .collect()
        })
        .unwrap_or_default();
    let policy_when_denied: BTreeMap<String, String> = tier3
        .map(|t| {
            t.policies
                .categories
                .iter()
                .filter_map(|cat| {
                    cat.when_denied
                        .as_ref()
                        .map(|k| (cat.name.clone(), k.key.clone()))
                })
                .collect()
        })
        .unwrap_or_default();

    // Helper — resolve the effective `when_denied` for a given policy
    // string. Walks the resolution chain: prefer the per-command
    // override (caller-supplied) over the per-policy category default.
    let resolve_when_denied = |policy_text: &str, override_key: Option<&str>| -> Option<String> {
        if let Some(k) = override_key {
            return Some(k.to_owned());
        }
        // The `policy_text` carries `@policy.<name>` for named
        // categories; strip the prefix to look up the category default.
        if let Some(name) = policy_text
            .trim()
            .strip_prefix("@policy.")
            .map(|s| s.split_whitespace().next().unwrap_or(""))
        {
            return policy_when_denied.get(name).cloned();
        }
        None
    };

    for command in command_blocks(lines) {
        let name = command_name(command[0].trim_start()).unwrap_or("unknown");

        if let Some(policy) = direct_child_value(command, "policy ") {
            let override_key = command_when_denied.get(name).map(String::as_str);
            let when_denied = resolve_when_denied(&policy, override_key);
            policies.push(InspectPolicy {
                subject: format!("command.{name}"),
                atoms: resolve_policy_atoms(&policy, policy_atoms),
                policy,
                origin: "explicit".to_owned(),
                requires: Vec::new(),
                when_denied,
            });
        }
    }

    for query in query_blocks(lines) {
        let name = query_name(query[0].trim_start()).unwrap_or("unknown");

        if let Some(policy) = direct_child_value(query, "policy ") {
            let when_denied = resolve_when_denied(&policy, None);
            policies.push(InspectPolicy {
                subject: format!("query.{name}"),
                atoms: resolve_policy_atoms(&policy, policy_atoms),
                policy,
                origin: "explicit".to_owned(),
                requires: Vec::new(),
                when_denied,
            });
        }
    }

    let mut workflow_name = None;
    let mut workflow_policy = None;

    for line in lines {
        let trimmed = line.trim_start();

        if trimmed.is_empty() {
            continue;
        }

        if leading_spaces(line) == 2 && trimmed.starts_with("workflow ") {
            workflow_name = trimmed.split_whitespace().nth(1).map(str::to_owned);
            workflow_policy = None;
        } else if leading_spaces(line) == 4 && workflow_name.is_some() {
            if let Some(policy) = trimmed.strip_prefix("policy ") {
                workflow_policy = Some(policy.to_owned());
            } else if is_transition_line(trimmed) {
                let transition = transition_name(trimmed).unwrap_or("unknown");
                let policy = workflow_policy.clone().unwrap_or_else(|| "none".to_owned());
                let mut requires = Vec::new();

                if let Some(required) = transition_requires(trimmed) {
                    requires.push(InspectPolicyRequirement {
                        atoms: resolve_policy_atoms(&required, policy_atoms),
                        policy: required,
                        origin: "transition.requires".to_owned(),
                    });
                }

                let when_denied = resolve_when_denied(&policy, None);
                policies.push(InspectPolicy {
                    subject: format!(
                        "workflow.{}.{}",
                        workflow_name.as_deref().unwrap_or("unknown"),
                        transition
                    ),
                    atoms: resolve_policy_atoms(&policy, policy_atoms),
                    policy,
                    origin: "workflow.policy".to_owned(),
                    requires,
                    when_denied,
                });
            }
        } else if leading_spaces(line) <= 2 {
            workflow_name = None;
            workflow_policy = None;
        }
    }

    policies
}

fn inspect_tests(
    lines: &[String],
    policy_atoms: &BTreeMap<String, Vec<String>>,
) -> Vec<InspectTests> {
    let mut tests = Vec::new();
    let mut subject_stack: Vec<(usize, String)> = Vec::new();
    let mut index = 0;

    for command in command_blocks(lines) {
        let name = command_name(command[0].trim_start()).unwrap_or("unknown");
        let Some(policy) = direct_child_value(command, "policy ") else {
            continue;
        };
        let atoms = resolve_policy_atoms(&policy, policy_atoms);
        if atoms.is_empty() {
            continue;
        }
        let subject = format!("command.{name}");
        push_inspect_test_assertion(
            &mut tests,
            &subject,
            "authz",
            format!("permits {}", atoms.join(", ")),
            format!("generated from command policy {policy}"),
        );
        push_inspect_test_assertion(
            &mut tests,
            &subject,
            "authz",
            format!("forbids actors outside {policy}"),
            format!("generated from closed-world command policy {policy}"),
        );
    }

    while index < lines.len() {
        let line = &lines[index];
        let trimmed = line.trim_start();
        let leading = leading_spaces(line);

        if trimmed.is_empty() {
            index += 1;
            continue;
        }

        while subject_stack
            .last()
            .is_some_and(|(indent, _)| *indent >= leading)
        {
            subject_stack.pop();
        }

        if let Some(subject) = inspect_subject(trimmed) {
            subject_stack.push((leading, subject));
        }

        if trimmed == "tests" {
            let subject = subject_stack
                .last()
                .map(|(_, subject)| subject.clone())
                .unwrap_or_else(|| "unknown".to_owned());
            let mut groups: BTreeMap<String, Vec<InspectTestAssertion>> = BTreeMap::new();
            let mut child_index = index + 1;

            while child_index < lines.len() && leading_spaces(&lines[child_index]) > leading {
                let assertion = lines[child_index].trim_start();
                if !assertion.is_empty() {
                    groups
                        .entry(test_group(assertion).to_owned())
                        .or_default()
                        .push(InspectTestAssertion {
                            assertion: assertion.to_owned(),
                            origin: "authored".to_owned(),
                        });
                }
                child_index += 1;
            }

            merge_inspect_tests(&mut tests, InspectTests { subject, groups });
            index = child_index;
            continue;
        }

        index += 1;
    }

    tests
}

fn push_inspect_test_assertion(
    tests: &mut Vec<InspectTests>,
    subject: &str,
    group: &str,
    assertion: String,
    origin: String,
) {
    let Some(existing) = tests.iter_mut().find(|entry| entry.subject == subject) else {
        tests.push(InspectTests {
            subject: subject.to_owned(),
            groups: BTreeMap::from([(
                group.to_owned(),
                vec![InspectTestAssertion { assertion, origin }],
            )]),
        });
        return;
    };

    existing
        .groups
        .entry(group.to_owned())
        .or_default()
        .push(InspectTestAssertion { assertion, origin });
}

fn merge_inspect_tests(tests: &mut Vec<InspectTests>, incoming: InspectTests) {
    let Some(existing) = tests
        .iter_mut()
        .find(|entry| entry.subject == incoming.subject)
    else {
        tests.push(incoming);
        return;
    };

    for (group, assertions) in incoming.groups {
        existing.groups.entry(group).or_default().extend(assertions);
    }
}

fn collect_resource_names(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .filter_map(|line| {
            let trimmed = line.trim_start();
            if leading_spaces(line) == 4 && trimmed.starts_with("resource ") {
                trimmed.split_whitespace().nth(1).map(str::to_owned)
            } else {
                None
            }
        })
        .collect()
}

fn collect_record_names(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .filter_map(|line| {
            let trimmed = line.trim_start();
            if leading_spaces(line) == 4 && trimmed.starts_with("record ") {
                trimmed.split_whitespace().nth(1).map(str::to_owned)
            } else {
                None
            }
        })
        .collect()
}

fn collect_query_names(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .filter_map(|line| {
            let trimmed = line.trim_start();
            if leading_spaces(line) == 4 && trimmed.starts_with("query.") {
                trimmed.split_whitespace().nth(1).map(str::to_owned)
            } else {
                None
            }
        })
        .collect()
}

fn collect_command_names(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .filter_map(|line| {
            let trimmed = line.trim_start();
            if leading_spaces(line) == 2 && trimmed.starts_with("command ") {
                command_name(trimmed).map(str::to_owned)
            } else {
                None
            }
        })
        .collect()
}

fn collect_workflow_summaries(lines: &[String]) -> Vec<InspectWorkflowSummary> {
    let mut workflows = Vec::new();
    let mut current: Option<InspectWorkflowSummary> = None;

    for line in lines {
        let trimmed = line.trim_start();

        if leading_spaces(line) == 2 {
            if let Some(workflow) = current.take() {
                workflows.push(workflow);
            }

            current = if trimmed.starts_with("workflow ") {
                trimmed
                    .split_whitespace()
                    .nth(1)
                    .map(|name| InspectWorkflowSummary {
                        name: name.to_owned(),
                        transitions: Vec::new(),
                    })
            } else {
                None
            };
            continue;
        }

        if leading_spaces(line) == 4 && is_transition_line(trimmed) {
            if let Some(workflow) = current.as_mut() {
                if let Some(transition) = transition_name(trimmed) {
                    workflow.transitions.push(transition.to_owned());
                }
            }
        }
    }

    if let Some(workflow) = current {
        workflows.push(workflow);
    }

    workflows
}

fn collect_named_top_blocks(lines: &[String], keyword: &str) -> Vec<String> {
    lines
        .iter()
        .filter_map(|line| {
            let trimmed = line.trim_start();
            if leading_spaces(line) == 2 && trimmed.starts_with(keyword) {
                trimmed.split_whitespace().nth(1).map(str::to_owned)
            } else {
                None
            }
        })
        .collect()
}

fn collect_event_names(lines: &[String]) -> Vec<String> {
    collect_event_decls(lines)
        .into_iter()
        .map(|event| event.name)
        .collect()
}

fn collect_surface_names(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .filter_map(|line| {
            let trimmed = line.trim_start();
            if leading_spaces(line) == 2 && trimmed.starts_with("surface ") {
                let parts: Vec<_> = trimmed.split_whitespace().skip(1).collect();
                (!parts.is_empty()).then(|| parts.join("/"))
            } else {
                None
            }
        })
        .collect()
}

fn collect_view_anchors(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .filter_map(|line| {
            let trimmed = line.trim_start();
            let (_, anchor) = trimmed.split_once(" id @anchor.")?;
            let name = anchor.split_whitespace().next()?;
            Some(format!("@anchor.{name}"))
        })
        .collect()
}

fn collect_extends_anchors(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .filter_map(|line| {
            let trimmed = line.trim_start();
            if leading_spaces(line) == 2 && trimmed.starts_with("extends @anchor.") {
                trimmed.split_whitespace().nth(1).map(str::to_owned)
            } else {
                None
            }
        })
        .collect()
}

fn collect_extensible_by_features(lines: &[String]) -> Vec<String> {
    let mut features = Vec::new();

    for line in lines {
        let trimmed = line.trim_start();
        if leading_spaces(line) == 6 && trimmed.starts_with("extensible_by ") {
            features.extend(
                trimmed
                    .trim_start_matches("extensible_by ")
                    .split(',')
                    .map(str::trim)
                    .filter(|feature| !feature.is_empty())
                    .map(str::to_owned),
            );
        }
    }

    features
}

fn collect_job_and_webhook_names(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .filter_map(|line| {
            let trimmed = line.trim_start();
            if leading_spaces(line) == 2
                && (trimmed.starts_with("job ") || trimmed.starts_with("webhook "))
            {
                trimmed.split_whitespace().nth(1).map(str::to_owned)
            } else {
                None
            }
        })
        .collect()
}

fn top_level_blocks<'a>(lines: &'a [String], prefix: &str) -> Vec<&'a [String]> {
    let mut blocks = Vec::new();
    let mut index = 0;

    while index < lines.len() {
        if leading_spaces(&lines[index]) == 2 && lines[index].trim_start().starts_with(prefix) {
            let start = index;
            index += 1;

            while index < lines.len() {
                let trimmed = lines[index].trim_start();
                if leading_spaces(&lines[index]) == 2 && !trimmed.is_empty() {
                    break;
                }
                index += 1;
            }

            blocks.push(&lines[start..index]);
        } else {
            index += 1;
        }
    }

    blocks
}

fn query_blocks(lines: &[String]) -> Vec<&[String]> {
    let mut blocks = Vec::new();
    let mut index = 0;

    while index < lines.len() {
        if leading_spaces(&lines[index]) == 4 && lines[index].trim_start().starts_with("query.") {
            let start = index;
            index += 1;

            while index < lines.len() {
                let trimmed = lines[index].trim_start();
                if leading_spaces(&lines[index]) <= 4 && !trimmed.is_empty() {
                    break;
                }
                index += 1;
            }

            blocks.push(&lines[start..index]);
        } else {
            index += 1;
        }
    }

    blocks
}

fn command_blocks(lines: &[String]) -> Vec<&[String]> {
    let mut blocks = Vec::new();
    let mut index = 0;

    while index < lines.len() {
        if leading_spaces(&lines[index]) == 2 && lines[index].trim_start().starts_with("command ") {
            let start = index;
            index += 1;

            while index < lines.len() {
                let trimmed = lines[index].trim_start();
                if leading_spaces(&lines[index]) == 2 && !trimmed.is_empty() {
                    break;
                }
                index += 1;
            }

            blocks.push(&lines[start..index]);
        } else {
            index += 1;
        }
    }

    blocks
}

fn query_name(trimmed_line: &str) -> Option<&str> {
    let mut parts = trimmed_line.split_whitespace();
    if parts.next()?.starts_with("query.") {
        parts.next()
    } else {
        None
    }
}

fn query_kind(block: &[String]) -> &'static str {
    let header = block[0].trim_start();
    let qualifier = header.strip_prefix("query.").unwrap_or("");
    match qualifier.split_whitespace().next().unwrap_or("") {
        "lookup" => "lookup",
        "sql" => "sql",
        _ => "list",
    }
}

fn named_top_block_name(trimmed_line: &str) -> Option<&str> {
    trimmed_line.split_whitespace().nth(1)
}

fn command_name(trimmed_line: &str) -> Option<&str> {
    let mut parts = trimmed_line.split_whitespace();
    if parts.next()? == "command" {
        parts.next()
    } else {
        None
    }
}

fn command_needs_inferred_target(lines: &[String]) -> bool {
    let has_route_id = lines
        .iter()
        .any(|line| leading_spaces(line) == 4 && line.trim_start() == "route id: ID");
    let mutates_existing = lines.iter().any(|line| {
        leading_spaces(line) == 4
            && (line.trim_start().starts_with("updates ")
                || line.trim_start().starts_with("deletes "))
    });

    has_route_id && mutates_existing
}

fn query_param_names(lines: &[String]) -> Vec<String> {
    let mut params = Vec::new();
    let mut in_params = false;

    for line in lines {
        let trimmed = line.trim_start();

        if leading_spaces(line) == 6 {
            in_params = trimmed == "params";
            continue;
        }

        if in_params && leading_spaces(line) == 8 {
            if let Some((name, _)) = typed_declaration(trimmed) {
                params.push(name.to_owned());
            }
        } else if leading_spaces(line) <= 6 {
            in_params = false;
        }
    }

    if params.is_empty() {
        if let Some(key) = lines
            .first()
            .and_then(|line| line.trim_start().split(" by ").nth(1))
            .and_then(|rest| rest.split_once(':').map(|(name, _)| name.trim()))
            .filter(|name| !name.is_empty())
        {
            params.push(key.to_owned());
        }
    }

    params
}

fn command_route_names(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .filter_map(|line| {
            if leading_spaces(line) == 4 {
                let trimmed = line.trim_start();
                let mut parts = trimmed.split_whitespace();
                if parts.next()? == "route" {
                    return parts
                        .next()
                        .map(|name| name.trim_end_matches(':').to_owned());
                }
            }
            None
        })
        .collect()
}

fn command_input_names(lines: &[String]) -> Vec<String> {
    let mut inputs = Vec::new();
    let mut in_input = false;

    for line in lines {
        let trimmed = line.trim_start();

        if leading_spaces(line) == 4 {
            in_input = trimmed == "input";

            if let Some(rest) = trimmed.strip_prefix("input ") {
                inputs.extend(parse_ident_list(rest));
            }
            continue;
        }

        if in_input && leading_spaces(line) == 6 {
            if let Some((name, _)) = typed_declaration(trimmed) {
                inputs.push(name.to_owned());
            }
        } else if leading_spaces(line) <= 4 {
            in_input = false;
        }
    }

    inputs
}

fn typed_declaration(trimmed_line: &str) -> Option<(&str, &str)> {
    let (name, rest) = trimmed_line.split_once(':')?;
    let name = name.trim();
    let ty = rest.trim().split_whitespace().next()?;

    if name.is_empty() || ty.is_empty() {
        None
    } else {
        Some((name, ty))
    }
}

fn emits_dependencies(feature: &str, subject: &str, lines: &[String]) -> Vec<InspectDependency> {
    let mut dependencies = Vec::new();

    for line in lines {
        let trimmed = line.trim_start();

        if let Some(events) = trimmed.strip_prefix("emits ") {
            let origin = if emits_derived_effect(events).is_some() {
                "emits.derived"
            } else {
                "emits"
            };
            for event in parse_event_list(events) {
                dependencies.push(inspect_dependency(
                    "emits_event",
                    subject,
                    qualify_event_ref(feature, &event),
                    origin,
                ));
            }
        } else if is_transition_line(trimmed) {
            if let Some(event) = trailing_scalar_value_after(trimmed, "emits") {
                dependencies.push(inspect_dependency(
                    "emits_event",
                    subject,
                    qualify_event_ref(feature, event),
                    "transition.emits",
                ));
            }
        }
    }

    dependencies
}

fn emits_derived_effect(emits_rest: &str) -> Option<&'static str> {
    let mut tokens = emits_rest.split_whitespace();
    tokens.next()?;
    if tokens.next()? != "from" {
        return None;
    }
    match tokens.next()? {
        "creates" => Some("creates"),
        "updates" => Some("updates"),
        "deletes" => Some("deletes"),
        _ => None,
    }
}

fn query_reference_dependencies(subject: &str, lines: &[String]) -> Vec<InspectDependency> {
    let mut dependencies = Vec::new();

    for line in lines {
        let trimmed = line.trim_start();

        for prefix in ["target ", "source "] {
            if let Some(value) = trimmed.strip_prefix(prefix) {
                if let Some(query) = value
                    .split_once('(')
                    .map(|(query, _)| query)
                    .or_else(|| value.split_whitespace().next())
                    .filter(|query| query.contains("query."))
                {
                    dependencies.push(inspect_dependency(
                        "query_ref",
                        subject,
                        query.trim(),
                        prefix.trim(),
                    ));
                }
            }
        }
    }

    dependencies
}

fn parse_event_list(source: &str) -> Vec<String> {
    let first = source.split_whitespace().next().unwrap_or(source);
    first
        .split(',')
        .map(str::trim)
        .filter(|event| {
            !event.is_empty()
                && event
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'.')
        })
        .map(str::to_owned)
        .collect()
}

fn qualify_event_ref(feature: &str, event: &str) -> String {
    if event.contains('.') {
        event.to_owned()
    } else {
        format!("{feature}.{event}")
    }
}

fn trailing_scalar_value_after<'a>(trimmed_line: &'a str, keyword: &str) -> Option<&'a str> {
    let mut tokens = trimmed_line.split_whitespace();
    while let Some(token) = tokens.next() {
        if token == keyword {
            return tokens.next();
        }
    }
    None
}

fn direct_child_value(lines: &[String], prefix: &str) -> Option<String> {
    let child_indent = lines.first().map(|line| leading_spaces(line) + 2)?;

    lines.iter().find_map(|line| {
        if leading_spaces(line) == child_indent {
            line.trim_start().strip_prefix(prefix).map(str::to_owned)
        } else {
            None
        }
    })
}

fn inspect_notifications(
    lines: &[String],
    tier3: Option<&Tier3FeatureSlice>,
) -> Vec<InspectNotification> {
    let mut notifications = Vec::new();

    for block in top_level_blocks(lines, "notification ") {
        let name = named_top_block_name(block[0].trim_start())
            .unwrap_or("unknown")
            .to_owned();
        let channels = direct_child_value(block, "channel ")
            .map(|raw| {
                raw.split(',')
                    .map(|c| c.trim().to_owned())
                    .filter(|c| !c.is_empty())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let recipient = direct_child_value(block, "recipient ");
        let trigger = direct_child_value(block, "trigger ");
        let template = direct_child_value(block, "template ")
            .as_deref()
            .map(strip_quotes);
        let policy = direct_child_value(block, "policy ");
        let tenant_from = direct_child_value(block, "tenant_from ");
        let idempotency = direct_child_value(block, "idempotency ");
        let retry = direct_child_value(block, "retry ");
        let rate_limit = direct_child_value(block, "rate_limit ")
            .as_deref()
            .map(strip_quotes);

        // Notifications expanded bucket cycle — typed `digest` /
        // `throttle` come from the lifted IR slice. The text-walker
        // owns the scalar fields above (legacy notation), but the
        // structured sub-blocks must surface typed so consumers can
        // read every-window / per-recipient / burst / strategy cold.
        let (digest, throttle) = tier3
            .and_then(|slice| slice.notifications.iter().find(|n| n.name == name))
            .map(|n| {
                let digest = n.digest.as_ref().map(|d| InspectNotificationDigest {
                    every: d.every.clone(),
                    group_by: d.group_by.clone(),
                    max_size: d.max_size,
                    template_strategy: d.template_strategy.map(|s| match s {
                        lazuli_ir::DigestStrategy::Merge => "merge".to_owned(),
                        lazuli_ir::DigestStrategy::Append => "append".to_owned(),
                    }),
                });
                let throttle = n.throttle.as_ref().map(|t| InspectNotificationThrottle {
                    max_per: t.max_per.clone(),
                    per_recipient: t.per_recipient,
                    per_channel: t.per_channel,
                    burst: t.burst,
                });
                (digest, throttle)
            })
            .unwrap_or((None, None));

        notifications.push(InspectNotification {
            name,
            channels,
            recipient,
            trigger,
            template,
            policy,
            tenant_from,
            idempotency,
            retry,
            rate_limit,
            digest,
            throttle,
            origin: "notification",
        });
    }

    notifications
}

fn inspect_agents(lines: &[String]) -> Vec<InspectAgent> {
    let mut agents = Vec::new();

    for block in top_level_blocks(lines, "agent ") {
        let name = named_top_block_name(block[0].trim_start())
            .unwrap_or("unknown")
            .to_owned();
        let inputs = command_input_names(block);
        let context = direct_child_value(block, "context ")
            .as_deref()
            .map(strip_quotes);
        let policy = direct_child_value(block, "policy ");
        let rate_limit = direct_child_value(block, "rate_limit ")
            .as_deref()
            .map(strip_quotes);
        let output_raw = direct_child_value(block, "output ");
        let (output_kind, output_discriminator) = classify_agent_output(output_raw.as_deref());
        let model = direct_child_value(block, "model ");
        let prompt = direct_child_value(block, "prompt ")
            .as_deref()
            .map(strip_quotes);
        // Cut A — tool entries live as indent-6 lines under a `tools`
        // child block. The legacy `tools <comma-list>` shorthand never
        // existed in the canonical syntax; the previous text extractor
        // returned `None` for the canonical form. This walker handles
        // both for safety while older fixtures linger.
        let tools = collect_agent_block_entries(block, "tools");
        let evals = collect_agent_eval_case_names(block);
        let safety = direct_child_value(block, "safety ");

        let temperature = direct_child_value(block, "temperature ");
        let max_tokens = direct_child_value(block, "max_tokens ");
        let top_p = direct_child_value(block, "top_p ");
        let seed = direct_child_value(block, "seed ");

        let eval_determinism = if evals.is_empty() {
            None
        } else {
            let temp_zero = temperature.as_deref().and_then(|s| s.parse::<f64>().ok()) == Some(0.0);
            let seed_present = seed.is_some();
            Some(if temp_zero && seed_present {
                "pinned"
            } else {
                "nondeterministic"
            })
        };

        let expose_http = collect_agent_expose(block);

        agents.push(InspectAgent {
            name,
            inputs,
            context,
            policy,
            rate_limit,
            output: output_raw,
            output_kind,
            output_discriminator,
            model,
            temperature,
            max_tokens,
            top_p,
            seed,
            prompt,
            tools,
            evals,
            eval_determinism,
            safety,
            expose_http,
            origin: "agent",
        });
    }

    agents
}

/// Derive `(output_kind, output_discriminator)` from the raw text after
/// `output `. The discriminator name surfaces for the two discriminated
/// shapes plus the bare-record form (lowering disambiguates record vs
/// text via the workspace IR; we record the symbol verbatim).
fn classify_agent_output(raw: Option<&str>) -> (Option<&'static str>, Option<String>) {
    let Some(raw) = raw else { return (None, None) };
    let trimmed = raw.trim();
    if let Some(rest) = trimmed.strip_prefix("stream ") {
        return (Some("stream"), Some(rest.trim().to_owned()));
    }
    if let Some(rest) = trimmed.strip_prefix("discriminator ") {
        return (Some("discriminated_enum"), Some(rest.trim().to_owned()));
    }
    if trimmed.is_empty() {
        return (None, None);
    }
    // Bare type ref. Text builtins keep `text`; PascalCase identifiers
    // (likely an author-defined record/enum) carry the symbol forward so
    // doctor's `agent_discriminator_target_invalid_diagnostics` and the
    // expand pass can interpret. The `text` label stays — lowering
    // promotes to `discriminated_record` when records resolve.
    let first = trimmed.chars().next();
    let looks_like_symbol = first.is_some_and(|c| c.is_ascii_uppercase());
    let kind = if matches!(
        trimmed,
        "Text" | "Integer" | "Boolean" | "Decimal" | "Date" | "DateTime" | "Json" | "ID"
    ) {
        "text"
    } else if looks_like_symbol {
        // Could be a record-with-discriminator (DiscriminatedRecord) or
        // a plain record reference; expand-pass disambiguates. We label
        // as `text` here to keep the file-local pass single-pass; the
        // symbol is surfaced via `output_discriminator`.
        "text"
    } else {
        "text"
    };
    let discriminator = if looks_like_symbol {
        Some(trimmed.to_owned())
    } else {
        None
    };
    (Some(kind), discriminator)
}

/// Walk the agent body for `<block> NEWLINE\n   <entry>\n   ...` and
/// return the indent-6 children as their raw trimmed source. Used for
/// both the `tools` and a future cut's other list-shaped children.
fn collect_agent_block_entries(block: &[String], parent: &str) -> Vec<String> {
    let Some(parent_indent) = block.first().map(|line| leading_spaces(line)) else {
        return Vec::new();
    };
    let child_indent = parent_indent + 2;
    let grandchild_indent = child_indent + 2;

    let mut entries = Vec::new();
    let mut in_block = false;
    for line in block.iter().skip(1) {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let leading = leading_spaces(line);
        if leading <= parent_indent {
            break;
        }
        if leading == child_indent {
            in_block = trimmed == parent;
            continue;
        }
        if in_block && leading == grandchild_indent {
            entries.push(trimmed.to_owned());
        }
    }
    entries
}

/// Walk the agent body for an `expose http` block and surface the
/// declared method/path/route/audience/rate_limit. Cut A.7's
/// inspect-side observable; doctor handles cross-feature resolution.
fn collect_agent_expose(block: &[String]) -> Option<InspectAgentExpose> {
    let parent_indent = block.first().map(|line| leading_spaces(line))?;
    let child_indent = parent_indent + 2;
    let grandchild_indent = child_indent + 2;

    let mut in_expose = false;
    let mut method: Option<String> = None;
    let mut path: Option<String> = None;
    let mut route_slots: Vec<String> = Vec::new();
    let mut audience: Option<String> = None;
    let mut rate_limit_override: Option<String> = None;

    for line in block.iter().skip(1) {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let leading = leading_spaces(line);
        if leading <= parent_indent {
            break;
        }
        if leading == child_indent {
            in_expose = trimmed == "expose http";
            continue;
        }
        if in_expose && leading == grandchild_indent {
            if let Some(rest) = trimmed.strip_prefix("method ") {
                method = Some(rest.trim().to_ascii_uppercase());
            } else if let Some(rest) = trimmed.strip_prefix("path ") {
                path = Some(strip_quotes(rest.trim()).to_owned());
            } else if let Some(rest) = trimmed.strip_prefix("route ") {
                if let Some((name_part, _)) = rest.split_once(':') {
                    route_slots.push(name_part.trim().to_owned());
                }
            } else if let Some(rest) = trimmed.strip_prefix("audience ") {
                audience = Some(rest.trim().to_owned());
            } else if let Some(rest) = trimmed.strip_prefix("rate_limit ") {
                rate_limit_override = Some(strip_quotes(rest.trim()).to_owned());
            }
        }
    }

    let method = method?;
    let path = path?;
    Some(InspectAgentExpose {
        method,
        path,
        route_slots,
        audience,
        rate_limit_override,
    })
}

/// Walk the agent body for `evals` and return the list of eval `case`
/// names declared inside.
fn collect_agent_eval_case_names(block: &[String]) -> Vec<String> {
    let Some(parent_indent) = block.first().map(|line| leading_spaces(line)) else {
        return Vec::new();
    };
    let child_indent = parent_indent + 2;
    let grandchild_indent = child_indent + 2;

    let mut cases = Vec::new();
    let mut in_block = false;
    for line in block.iter().skip(1) {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let leading = leading_spaces(line);
        if leading <= parent_indent {
            break;
        }
        if leading == child_indent {
            in_block = trimmed == "evals";
            continue;
        }
        if in_block && leading == grandchild_indent {
            if let Some(rest) = trimmed.strip_prefix("case ") {
                let name = rest.split_whitespace().next().unwrap_or("").to_owned();
                if !name.is_empty() {
                    cases.push(name);
                }
            }
        }
    }
    cases
}

fn parse_audit(lines: &[String], origin: &'static str) -> Option<InspectAudit> {
    let child_indent = lines.first().map(|line| leading_spaces(line) + 2)?;
    let audit_grandchild_indent = child_indent + 2;

    let mut hit_index: Option<usize> = None;
    let mut audit: Option<InspectAudit> = None;
    for (offset, line) in lines.iter().enumerate().skip(1) {
        if leading_spaces(line) != child_indent {
            continue;
        }
        let trimmed = line.trim_start();
        if trimmed == "audit" {
            audit = Some(InspectAudit {
                fields: Vec::new(),
                emit_to: None,
                origin,
            });
            hit_index = Some(offset);
            break;
        }
        if let Some(rest) = trimmed.strip_prefix("audit ") {
            let rest = rest.trim();
            if rest == "none" {
                return None;
            }
            let fields: Vec<String> = rest
                .split(',')
                .map(|part| part.trim().to_owned())
                .filter(|part| !part.is_empty())
                .collect();
            audit = Some(InspectAudit {
                fields,
                emit_to: None,
                origin,
            });
            hit_index = Some(offset);
            break;
        }
    }

    // Observability bucket cycle row 37 — scan grandchildren of the
    // `audit` line for an `emit_to <target>` slot. The slot lives one
    // indent step deeper than `audit` and stops at the next
    // sibling-or-shallower line.
    if let (Some(start), Some(audit_value)) = (hit_index, audit.as_mut()) {
        for line in lines.iter().skip(start + 1) {
            let leading = leading_spaces(line);
            if leading <= child_indent {
                break;
            }
            if leading != audit_grandchild_indent {
                continue;
            }
            let trimmed = line.trim_start();
            if let Some(rest) = trimmed.strip_prefix("emit_to ") {
                audit_value.emit_to = Some(rest.trim().to_owned());
                break;
            }
        }
    }

    audit
}

fn direct_child_values(lines: &[String], prefix: &str) -> Vec<String> {
    let Some(child_indent) = lines.first().map(|line| leading_spaces(line) + 2) else {
        return Vec::new();
    };

    lines
        .iter()
        .filter_map(|line| {
            if leading_spaces(line) == child_indent {
                line.trim_start()
                    .strip_prefix(prefix)
                    .map(str::trim)
                    .map(str::to_owned)
            } else {
                None
            }
        })
        .collect()
}

fn field_name_from_typed_line(trimmed_line: &str) -> Option<&str> {
    let (head, _) = trimmed_line.split_once(':')?;
    let name = head.trim().split_whitespace().next()?;

    if name
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        Some(name)
    } else {
        None
    }
}

fn security_markers(line: &str) -> impl Iterator<Item = String> + '_ {
    namespace_references(line)
        .into_iter()
        .filter(|namespace| matches!(*namespace, "pii" | "cap" | "key"))
        .filter_map(|namespace| full_marker_reference(line, namespace))
}

fn full_marker_reference(line: &str, namespace: &str) -> Option<String> {
    let start = line.find(&format!("@{namespace}."))?;
    let after = &line[start..];
    let mut end = after
        .bytes()
        .position(|byte| !(byte.is_ascii_alphanumeric() || matches!(byte, b'@' | b'_' | b'.')))
        .unwrap_or(after.len());

    if after.as_bytes().get(end) == Some(&b'(') {
        end = after[end..]
            .find(')')
            .map(|relative| end + relative + 1)
            .unwrap_or(after.len());
    }

    Some(after[..end].to_owned())
}

fn resolve_policy_atoms(policy: &str, policies: &BTreeMap<String, Vec<String>>) -> Vec<String> {
    let policy = policy.strip_prefix("@policy.").unwrap_or(policy);
    policies
        .get(policy)
        .cloned()
        .unwrap_or_else(|| vec![policy.to_owned()])
}

fn inspect_inherited_payload_field(entry: &str, origin: String) -> InspectPayloadField {
    let Some((name, expression)) = entry.split_once('=') else {
        return InspectPayloadField {
            name: entry.to_owned(),
            ty: "Unknown".to_owned(),
            origin,
            expression: None,
            condition: None,
        };
    };
    let (expression, condition) = expression
        .split_once(" when ")
        .map(|(value, condition)| (value.trim(), Some(condition.trim().to_owned())))
        .unwrap_or((expression.trim(), None));

    InspectPayloadField {
        name: name.trim().to_owned(),
        ty: infer_payload_type(name.trim(), expression).to_owned(),
        origin,
        expression: Some(expression.to_owned()),
        condition,
    }
}

fn inspect_explicit_payload_field(line: &str, event_name: &str) -> Option<InspectPayloadField> {
    let (name, rest) = line.split_once(':')?;
    let ty = rest.split_whitespace().next()?;

    Some(InspectPayloadField {
        name: name.trim().to_owned(),
        ty: ty.to_owned(),
        origin: format!("event:{event_name}"),
        expression: None,
        condition: None,
    })
}

fn infer_payload_type(name: &str, expression: &str) -> &'static str {
    if name.ends_with("_id") || expression == "id" || expression.ends_with(".id") {
        "ID"
    } else {
        "Unknown"
    }
}

fn transition_name(trimmed_line: &str) -> Option<&str> {
    trimmed_line.split_once(':')?.0.split_whitespace().next()
}

fn is_transition_line(trimmed_line: &str) -> bool {
    let Some((left, right)) = trimmed_line.split_once(':') else {
        return false;
    };

    !left.trim().is_empty() && right.contains("->")
}

fn transition_requires(trimmed_line: &str) -> Option<String> {
    let (_, rhs) = trimmed_line.split_once(':')?;
    let (_, after_arrow) = rhs.trim().split_once("->")?;
    let mut tokens = after_arrow.split_whitespace();
    tokens.next()?;

    while let Some(token) = tokens.next() {
        if token == "requires" {
            return tokens.next().map(str::to_owned);
        }
    }

    None
}

fn inspect_subject(trimmed_line: &str) -> Option<String> {
    if let Some(name) = command_name(trimmed_line) {
        Some(format!("command.{name}"))
    } else if trimmed_line.starts_with("rule ") {
        Some(format!(
            "rule.{}",
            trimmed_line
                .trim_start_matches("rule ")
                .trim_matches('"')
                .to_owned()
        ))
    } else if view_anchor_line(trimmed_line) {
        trimmed_line
            .split(" id ")
            .nth(1)
            .and_then(|rest| rest.split_whitespace().next())
            .map(|anchor| format!("view.{anchor}"))
            .or_else(|| Some("view.anchor".to_owned()))
    } else if is_transition_line(trimmed_line) {
        transition_name(trimmed_line).map(|name| format!("transition.{name}"))
    } else {
        None
    }
}

fn view_anchor_line(trimmed_line: &str) -> bool {
    trimmed_line.starts_with("view ") && trimmed_line.contains(" id @anchor.")
}

fn test_group(assertion: &str) -> &'static str {
    if assertion.starts_with("permits @")
        || assertion.starts_with("forbids @")
        || assertion.contains(" as @")
    {
        "authz"
    } else if assertion.contains(" from ") {
        "transition"
    } else if assertion.contains(" when ") {
        "predicate"
    } else if assertion.starts_with("accepted by ") || assertion.starts_with("rejected by ") {
        "anchor"
    } else {
        "other"
    }
}

#[cfg(test)]
fn expand_canonical_source(source: &str) -> String {
    expand_canonical_source_with(source, ExpandSet::all())
}

fn expand_canonical_source_with(source: &str, expansions: ExpandSet) -> String {
    let newline = if source.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let lines: Vec<String> = source.lines().map(str::to_owned).collect();
    let inferred = if expansions.targets {
        infer_local_targets(&lines)
    } else {
        lines
    };
    let expanded = expand_feature_syntax(&inferred, expansions);
    let mut output = expanded.join(newline);

    if source.ends_with('\n') {
        output.push_str(newline);
    }

    output
}

fn infer_local_targets(lines: &[String]) -> Vec<String> {
    let mut output = Vec::new();
    let mut index = 0;

    while index < lines.len() {
        if leading_spaces(&lines[index]) == 0 && lines[index].trim_start().starts_with("feature ") {
            let start = index;
            index += 1;

            while index < lines.len() {
                if leading_spaces(&lines[index]) == 0
                    && lines[index].trim_start().starts_with("feature ")
                {
                    break;
                }
                index += 1;
            }

            output.extend(infer_local_targets_in_feature(&lines[start..index]));
        } else {
            output.push(lines[index].to_owned());
            index += 1;
        }
    }

    output
}

fn infer_local_targets_in_feature(lines: &[String]) -> Vec<String> {
    if !feature_has_id_lookup(lines) {
        return lines.to_vec();
    }

    let mut output = Vec::new();
    let mut index = 0;

    while index < lines.len() {
        if leading_spaces(&lines[index]) == 2 && lines[index].trim_start().starts_with("command ") {
            let start = index;
            index += 1;

            while index < lines.len() {
                let trimmed = lines[index].trim_start();
                if leading_spaces(&lines[index]) == 2 && !trimmed.is_empty() {
                    break;
                }
                index += 1;
            }

            output.extend(infer_local_target_in_command(&lines[start..index]));
        } else {
            output.push(lines[index].to_owned());
            index += 1;
        }
    }

    output
}

fn feature_has_id_lookup(lines: &[String]) -> bool {
    for (index, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();

        if leading_spaces(line) == 4 && trimmed.starts_with("query.lookup by_id by id:") {
            return true;
        }

        if leading_spaces(line) == 4 && trimmed == "query.lookup by_id" {
            let mut child_index = index + 1;

            while child_index < lines.len() && leading_spaces(&lines[child_index]) > 4 {
                if lines[child_index].trim_start() == "key id = params.id" {
                    return true;
                }
                child_index += 1;
            }
        }
    }

    false
}

fn infer_local_target_in_command(lines: &[String]) -> Vec<String> {
    let has_target = lines
        .iter()
        .any(|line| leading_spaces(line) == 4 && line.trim_start().starts_with("target "));
    let has_route_id = lines
        .iter()
        .any(|line| leading_spaces(line) == 4 && line.trim_start() == "route id: ID");
    let mutates_existing = lines.iter().any(|line| {
        leading_spaces(line) == 4
            && (line.trim_start().starts_with("updates ")
                || line.trim_start().starts_with("deletes "))
    });

    if has_target || !has_route_id || !mutates_existing {
        return lines.to_vec();
    }

    let mut output = Vec::new();
    let mut inserted = false;

    for line in lines {
        if !inserted && leading_spaces(line) == 4 && line.trim_start().starts_with("policy ") {
            output.push("    target query.by_id(id: route.id)".to_owned());
            inserted = true;
        }

        output.push(line.to_owned());
    }

    output
}

fn expand_feature_syntax(lines: &[String], expansions: ExpandSet) -> Vec<String> {
    let mut output = Vec::new();
    let mut index = 0;

    while index < lines.len() {
        if leading_spaces(&lines[index]) == 0 && lines[index].trim_start().starts_with("feature ") {
            let start = index;
            index += 1;

            while index < lines.len() {
                if leading_spaces(&lines[index]) == 0
                    && lines[index].trim_start().starts_with("feature ")
                {
                    break;
                }
                index += 1;
            }

            output.extend(expand_feature_block(&lines[start..index], expansions));
        } else {
            output.push(lines[index].to_owned());
            index += 1;
        }
    }

    output
}

#[derive(Debug, Clone)]
struct EventGroup {
    pattern: String,
    prefix: String,
    payload: Vec<String>,
}

#[derive(Debug, Clone)]
struct EventDecl {
    kind: &'static str,
    name: String,
    payload: Vec<String>,
}

fn expand_feature_block(lines: &[String], expansions: ExpandSet) -> Vec<String> {
    let event_groups = collect_event_groups(lines);
    let mut output = Vec::new();
    let mut index = 0;
    let mut in_command = false;
    let mut command_inputs = Vec::new();

    while index < lines.len() {
        let line = &lines[index];
        let trimmed = line.trim_start();
        let leading = leading_spaces(line);

        if leading == 2 && !trimmed.is_empty() {
            in_command = trimmed.starts_with("command ");
            command_inputs.clear();
        }

        if expansions.events && is_event_group_start(line) {
            let next_index = skip_nested_block(lines, index, leading);
            for event in collect_event_decls(&lines[index..next_index]) {
                let indent = " ".repeat(leading);
                let child_indent = " ".repeat(leading + 2);
                output.push(format!("{indent}{} {}", event.kind, event.name));

                for group in &event_groups {
                    if event.name.starts_with(&group.prefix) {
                        for payload in &group.payload {
                            output.push(format!("{child_indent}{}", expand_payload_entry(payload)));
                        }
                    }
                }

                for field in event.payload {
                    output.push(format!("{child_indent}{field}"));
                }
            }
            index = next_index;
            continue;
        }

        if in_command && leading == 4 && trimmed == "input" {
            command_inputs.clear();
        } else if in_command && leading == 4 && trimmed.starts_with("input ") {
            command_inputs = parse_ident_list(trimmed.trim_start_matches("input "));
        }

        if expansions.defaults
            && let Some(expanded) = expand_lookup_shorthand(line)
        {
            output.extend(expanded);
        } else if expansions.defaults
            && let Some(expanded) = expand_creates_from_input(line, &command_inputs)
        {
            output.extend(expanded);
        } else if expansions.defaults
            && let Some(expanded) = expand_transition_clauses(line)
        {
            output.extend(expanded);
        } else {
            output.push(line.to_owned());

            if expansions.events
                && let Some(event_name) = event_name(trimmed)
            {
                for group in &event_groups {
                    if event_name.starts_with(&group.prefix) {
                        let child_indent = " ".repeat(leading + 2);
                        for payload in &group.payload {
                            output.push(format!("{child_indent}{}", expand_payload_entry(payload)));
                        }
                    }
                }
            }
        }

        index += 1;
    }

    output
}

fn collect_event_groups(lines: &[String]) -> Vec<EventGroup> {
    let mut groups = Vec::new();
    let mut index = 0;

    while index < lines.len() {
        let line = &lines[index];

        if !is_event_group_start(line) {
            index += 1;
            continue;
        }

        let Some((pattern, prefix)) = event_group_pattern(line.trim_start()) else {
            index += 1;
            continue;
        };

        let mut payload = Vec::new();
        let mut payload_block = false;
        let mut child_index = index + 1;

        while child_index < lines.len() {
            let child = &lines[child_index];
            let child_trimmed = child.trim_start();

            if child_trimmed.is_empty() {
                child_index += 1;
                continue;
            }

            if leading_spaces(child) <= 4 {
                break;
            }

            if leading_spaces(child) == 6 {
                payload_block = child_trimmed == "payload";
            } else if payload_block && leading_spaces(child) == 8 && !child_trimmed.is_empty() {
                payload.push(child_trimmed.to_owned());
            }

            child_index += 1;
        }

        groups.push(EventGroup {
            pattern,
            prefix,
            payload,
        });
        index = child_index;
    }

    groups
}

fn collect_event_decls(lines: &[String]) -> Vec<EventDecl> {
    let mut events = Vec::new();
    let mut current_group: Option<(usize, String)> = None;

    for index in 0..lines.len() {
        let line = &lines[index];
        let trimmed = line.trim_start();
        let leading = leading_spaces(line);

        if is_event_group_start(line) {
            if let Some((_, prefix)) = event_group_pattern(trimmed) {
                current_group = Some((leading, prefix));
            }
            continue;
        }

        if let Some((group_indent, _)) = current_group.as_ref()
            && !trimmed.is_empty()
            && leading <= *group_indent
        {
            current_group = None;
        }

        if let Some((kind, raw_name)) = event_kind_and_name(trimmed) {
            let name = if let Some((group_indent, prefix)) = current_group.as_ref() {
                if leading == *group_indent + 2 {
                    qualify_group_event_name(prefix, raw_name)
                } else {
                    raw_name.to_owned()
                }
            } else {
                raw_name.to_owned()
            };
            events.push(EventDecl {
                kind,
                name,
                payload: collect_event_payload_fields(lines, index),
            });
        }
    }

    events
}

fn collect_event_payload_fields(lines: &[String], event_index: usize) -> Vec<String> {
    let event_indent = leading_spaces(&lines[event_index]);
    let mut fields = Vec::new();
    let mut index = event_index + 1;

    while index < lines.len() && leading_spaces(&lines[index]) > event_indent {
        if leading_spaces(&lines[index]) == event_indent + 2 {
            let trimmed = lines[index].trim_start();
            if field_name_from_typed_line(trimmed).is_some() {
                fields.push(trimmed.to_owned());
            }
        }
        index += 1;
    }

    fields
}

fn qualify_group_event_name(prefix: &str, raw_name: &str) -> String {
    if raw_name.starts_with(prefix) {
        raw_name.to_owned()
    } else {
        format!("{prefix}{raw_name}")
    }
}

fn is_event_group_start(line: &str) -> bool {
    leading_spaces(line) == 4
        && matches!(
            line.trim_start().split_whitespace().next(),
            Some("event_group" | "events")
        )
}

fn event_group_pattern(trimmed_line: &str) -> Option<(String, String)> {
    let mut parts = trimmed_line.split_whitespace();
    if !matches!(parts.next()?, "event_group" | "events") {
        return None;
    }

    let pattern = parts.next()?;
    pattern
        .strip_suffix('*')
        .map(|prefix| (pattern.to_owned(), prefix.to_owned()))
}

fn skip_nested_block(lines: &[String], start: usize, parent_indent: usize) -> usize {
    let mut index = start + 1;

    while index < lines.len() {
        let trimmed = lines[index].trim_start();
        if !trimmed.is_empty() && leading_spaces(&lines[index]) <= parent_indent {
            break;
        }
        index += 1;
    }

    index
}

fn event_name(trimmed_line: &str) -> Option<&str> {
    event_kind_and_name(trimmed_line).map(|(_, name)| name)
}

fn event_kind_and_name(trimmed_line: &str) -> Option<(&'static str, &str)> {
    if let Some(rest) = trimmed_line.strip_prefix("event.trace ") {
        rest.split_whitespace()
            .next()
            .map(|name| ("event.trace", name))
    } else {
        let rest = trimmed_line.strip_prefix("event ")?;
        rest.split_whitespace().next().map(|name| ("event", name))
    }
}

fn expand_payload_entry(entry: &str) -> String {
    let Some((name, expression)) = entry.split_once('=') else {
        return entry.to_owned();
    };
    let name = name.trim();
    let expression = expression
        .split_once(" when ")
        .map(|(value, _)| value)
        .unwrap_or(expression)
        .trim();
    let ty = if name.ends_with("_id") || expression == "id" || expression.ends_with(".id") {
        "ID"
    } else {
        "Unknown"
    };

    format!("{name}: {ty}")
}

fn expand_lookup_shorthand(line: &str) -> Option<Vec<String>> {
    let leading = leading_spaces(line);
    let indent = " ".repeat(leading);
    let trimmed = line.trim_start();
    let rest = trimmed.strip_prefix("query.lookup ")?;
    let (name, key) = rest.split_once(" by ")?;
    let (field, ty) = key.split_once(':')?;
    let name = name.trim();
    let field = field.trim();
    let ty = ty.trim();

    if name.is_empty() || field.is_empty() || ty.is_empty() {
        return None;
    }

    Some(vec![
        format!("{indent}query.lookup {name}"),
        format!("{indent}  params"),
        format!("{indent}    {field}: {ty}"),
        String::new(),
        format!("{indent}  key {field} = params.{field}"),
    ])
}

fn expand_creates_from_input(line: &str, inputs: &[String]) -> Option<Vec<String>> {
    if inputs.is_empty() {
        return None;
    }

    let leading = leading_spaces(line);
    let indent = " ".repeat(leading);
    let child_indent = " ".repeat(leading + 2);
    let trimmed = line.trim_start();
    let resource = trimmed
        .strip_prefix("creates ")?
        .strip_suffix(" from input")?
        .trim();

    if resource.is_empty() {
        return None;
    }

    let mut expanded = vec![format!("{indent}creates {resource}")];
    for input in inputs {
        expanded.push(format!("{child_indent}{input} = input.{input}"));
    }
    Some(expanded)
}

fn expand_transition_clauses(line: &str) -> Option<Vec<String>> {
    let leading = leading_spaces(line);
    let indent = " ".repeat(leading);
    let child_indent = " ".repeat(leading + 2);
    let trimmed = line.trim_start();
    let (left, right) = trimmed.split_once(':')?;
    let (from, after_arrow) = right.trim().split_once("->")?;
    let mut tokens = after_arrow.split_whitespace();
    let to = tokens.next()?;
    let remaining: Vec<&str> = tokens.collect();

    if remaining.is_empty() {
        return None;
    }

    let mut requires = None;
    let mut emits = None;
    let mut index = 0;

    while index < remaining.len() {
        match remaining[index] {
            "requires" if index + 1 < remaining.len() && requires.is_none() => {
                requires = Some(remaining[index + 1]);
                index += 2;
            }
            "emits" if index + 1 < remaining.len() && emits.is_none() => {
                emits = Some(remaining[index + 1]);
                index += 2;
            }
            _ => return None,
        }
    }

    let mut expanded = vec![format!(
        "{indent}{}: {} -> {}",
        left.trim(),
        from.trim(),
        to
    )];
    if let Some(policy) = requires {
        expanded.push(format!("{child_indent}requires {policy}"));
    }
    if let Some(event) = emits {
        expanded.push(format!("{child_indent}emits {event}"));
    }

    Some(expanded)
}

fn parse_ident_list(source: &str) -> Vec<String> {
    source
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_owned)
        .collect()
}

fn is_identifier(source: &str) -> bool {
    let mut chars = source.chars();
    matches!(chars.next(), Some(first) if first == '_' || first.is_ascii_alphabetic())
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

fn is_type_name(source: &str) -> bool {
    let mut chars = source.chars();
    matches!(chars.next(), Some(first) if first.is_ascii_uppercase())
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

fn namespace_references(line: &str) -> Vec<&str> {
    let mut namespaces = Vec::new();
    let mut rest = line;

    while let Some(start) = rest.find('@') {
        let after_at = &rest[start + 1..];
        let Some(dot) = after_at.find('.') else {
            rest = after_at;
            continue;
        };

        let namespace = &after_at[..dot];
        if !namespace.is_empty()
            && namespace
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            namespaces.push(namespace);
        }

        rest = &after_at[dot + 1..];
    }

    namespaces
}

fn leading_spaces(line: &str) -> usize {
    line.bytes().take_while(|byte| *byte == b' ').count()
}

#[cfg(test)]
mod tests;
