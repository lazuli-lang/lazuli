//! Clap surface for the `lazuli` binary.
//!
//! Carved out of `main.rs` as part of Wave R6-5 (Rails-style refactor):
//! this module owns every `#[derive(Parser)]` / `#[derive(Subcommand)]`
//! / `#[derive(ValueEnum)]` declaration plus the `From` adapters that
//! lower the clap-side enums into the analyzer/runtime-side types
//! (`SecurityProfile`, `cmd_test_types::Layer`, `cmd_design::*`).
//!
//! No behavior lives here. `fn main()` in `main.rs` matches on
//! [`Commands`] and dispatches into the `commands::*` subtree.
//!
//! ABI: `lazuli --help` is byte-identical with the pre-split build.

use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};
use lazuli_doctor_config::DoctorProfile as SecurityProfile;

use crate::cmd_design;
use crate::cmd_test_types;

#[derive(Debug, Parser)]
#[command(name = "lazuli", version = env!("CARGO_PKG_VERSION"))]
#[command(about = "Lazuli application metalinguage compiler")]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Commands,
    /// Skip the Lazurite.toml [lazuli] runtime version pin check.
    ///
    /// Use when intentionally bumping mid-project; ship a follow-up commit
    /// updating Lazurite.toml to match the new pin.
    #[arg(long, global = true)]
    pub(crate) allow_version_mismatch: bool,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Commands {
    /// (framework-dev) Dump the parser skeleton AST. Hidden from the
    /// published surface — contributor tooling, not an app-dev command.
    #[command(hide = true)]
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
        /// Security/doctor profile. When omitted, the manifest's
        /// `[doctor] profile` is honored (falling back to `strict`).
        /// An explicit flag always wins over the toml.
        #[arg(long, value_enum)]
        security_profile: Option<CheckSecurityProfile>,
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
        #[arg(long = "self", hide = true)]
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
    /// (framework-dev) Manage this repo's curated example fixtures.
    /// Hidden from the published surface — operates on the Lazuli repo,
    /// not an app developer's project.
    #[command(hide = true)]
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
    #[command(hide = true)]
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


mod subcommands;
mod value_enums;

pub(crate) use subcommands::{DesignCommand, ExamplesCommand, MigrateCommand, TranslateCommand};
pub(crate) use value_enums::{
    CheckSecurityProfile, DesignExportTarget, DesignImportFormat, GenerateKind, InspectFormat,
    InspectInclude, PlaywrightTarget, TestLayerFlag,
};
