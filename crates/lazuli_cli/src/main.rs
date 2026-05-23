use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use lazuli_lsp::SecurityProfile;
use serde::Serialize;
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

mod app_manifest;
mod cmd_design;
mod cmd_generate_feature;
mod cmd_generate_handler;
mod cmd_generate_playwright;
mod cmd_mcp;
mod cmd_new_frontends;
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
mod plugin_manifest;
mod plugin_semantic_resolver;
mod profile;
mod seed;
mod templates;
mod upgrade;
mod version;

const DEFAULT_TEMPLATE: &str = include_str!("../../../examples/crm.lzi");
const REGISTRY_TEMPLATE: &str =
    "registry\n  # capabilities: name typed\n  # integrations: provider-neutral declarations\n";
// Closes WAR-SCAFFOLD-GITIGNORE-01. The previous template's blanket
// `dist/` rule ignored user-authored handler files at
// `dist/go/<bc>/<name>.go`, violating Lazuli's regen contract (gen
// files are overwritable, non-gen files are sacred). The granular
// pattern below ignores ONLY regen-overwritable artifacts:
//   - `*.gen.go` / `*.gen.ts` / `*.zod.ts` (codegen outputs)
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
        Commands::Parse { input } => parse_command(&input),
        Commands::Check {
            input,
            security_profile,
        } => check_command(&input, security_profile, cli.allow_version_mismatch),
        Commands::Doctor {
            input,
            security_profile,
            check_release,
        } => doctor::doctor_command(
            &input,
            security_profile.into(),
            check_release,
            cli.allow_version_mismatch,
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
        } => debug_command(&project, error.as_deref(), capsule, &format),
        Commands::Profile {
            profile,
            top,
            by,
            format,
        } => profile_command(&profile, top, &by, &format),
        Commands::Examples { sub } => {
            let project_root =
                std::env::current_dir().context("failed to determine current directory")?;
            match sub {
                ExamplesCommand::Bundle { out } => {
                    examples_bundle::run_examples_bundle(&project_root, out.as_deref())
                }
                ExamplesCommand::Validate { check_decay } => {
                    examples_bundle::run_examples_validate(&project_root, check_decay)
                }
            }
            .map_err(|err| anyhow::anyhow!("{err}"))
        }
        Commands::Init { path } => init_command(&path),
        Commands::New {
            project_name,
            template,
            bare,
            no_git,
            module,
            frontends,
            in_place,
        } => new_command(
            project_name.as_deref(),
            &template,
            bare,
            no_git,
            module,
            frontends,
            in_place,
        ),
        Commands::Lsp { stdio: _ } => lsp_command(),
        Commands::SpikeGenerate { root, spec } => spike_generate_command(&root, spec.as_deref()),
        Commands::Plan { input, check } => plan_command(&input, check.as_deref()),
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
        } => generate_command(
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
        } => dev::run_dev(dev::DevOptions {
            source_root: path,
            out,
            no_run,
            debounce: std::time::Duration::from_millis(debounce),
        }),
        Commands::Migrate { sub } => {
            let project_root = std::env::current_dir().context("reading current directory")?;
            match sub {
                MigrateCommand::Up { target, yes } => {
                    migrate::run_migrate(&project_root, migrate::MigrateAction::Up { target, yes })
                        .map_err(|err| anyhow::anyhow!("{err}"))
                }
                MigrateCommand::Down { steps, yes } => {
                    migrate::run_migrate(&project_root, migrate::MigrateAction::Down { steps, yes })
                        .map_err(|err| anyhow::anyhow!("{err}"))
                }
                MigrateCommand::Status => {
                    migrate::run_migrate(&project_root, migrate::MigrateAction::Status)
                        .map_err(|err| anyhow::anyhow!("{err}"))
                }
                MigrateCommand::Dsl {
                    from,
                    to,
                    dry_run,
                    path,
                } => {
                    let root = path.unwrap_or(project_root);
                    let report = migrate::dsl::run_migrate_dsl(&root, &from, &to, dry_run)
                        .map_err(|err| anyhow::anyhow!("{err}"))?;
                    print!("{}", migrate::dsl::render_report(&report, dry_run));
                    if !report.rolled_back.is_empty() {
                        bail!(
                            "lazuli migrate dsl rolled back {} file(s); fix the recipe and re-run",
                            report.rolled_back.len()
                        );
                    }
                    Ok(())
                }
            }
        }
        Commands::Design { sub } => design_command(sub),
        Commands::Upgrade {
            from,
            to,
            target,
            dry_run,
        } => {
            let project_root = std::env::current_dir().context("reading current directory")?;
            let report = upgrade::run_upgrade(&project_root, &from, &to, &target, dry_run)
                .map_err(|err| anyhow::anyhow!("{err}"))?;
            for recipe in &report.applied {
                println!("applied {}", recipe.display());
            }
            for (recipe, error) in &report.failed {
                println!("failed {}: {}", recipe.display(), error);
            }
            if !report.failed.is_empty() {
                bail!("lazuli upgrade failed");
            }
            println!("lazuli upgrade applied {} recipe(s)", report.applied.len());
            Ok(())
        }
        Commands::Seed { only, force } => {
            let project_root =
                std::env::current_dir().context("failed to determine current directory")?;
            seed::run_seed(&project_root, only.as_deref(), force)
                .map_err(|err| anyhow::anyhow!("{err}"))
        }
        Commands::Changelog { from, to, output } => {
            changelog_command(&from, &to, output.as_deref())
        }
        Commands::Translate { sub } => match sub {
            TranslateCommand::Extract {
                input,
                out,
                locale,
                check,
            } => translate_extract_command(&input, &out, locale.as_deref(), check),
        },
        Commands::Mcp => cmd_mcp::run_mcp_server(),
    }
}

/// OpenAPI / Lazuli Go bucket cycle — emit an artifact derived from
/// the typed IR. Dispatch by closed-catalog `GenerateKind`.
fn generate_command(
    kind: GenerateKind,
    input: &Path,
    output: Option<&Path>,
    api_version: Option<&str>,
    module: Option<&str>,
    lazuli_go_version: Option<&str>,
    check: bool,
    with_source: bool,
    allow_drops: bool,
    allow_version_mismatch: bool,
    playwright_target: Option<PlaywrightTarget>,
) -> Result<()> {
    if !allow_version_mismatch {
        let project_root = project_root_for_input(input);
        let manifest = lazurite_manifest::load(&project_root).with_context(|| {
            format!(
                "failed to read {}",
                project_root.join("Lazurite.toml").display()
            )
        })?;
        version::enforce_manifest_pin(manifest.as_ref())?;
    }

    match kind {
        GenerateKind::Openapi => generate_openapi(input, output, api_version),
        GenerateKind::Go => generate_go(
            input,
            output,
            module,
            lazuli_go_version,
            check,
            with_source,
            allow_drops,
        ),
        GenerateKind::Feature => {
            reject_generate_feature_options(
                output,
                api_version,
                module,
                lazuli_go_version,
                check,
                with_source,
            )?;
            let name = input.to_str().context("feature name must be valid UTF-8")?;
            let project_root =
                std::env::current_dir().context("failed to determine current directory")?;
            cmd_generate_feature::run(name, &project_root)
        }
        GenerateKind::Handler => {
            let ident = input
                .to_str()
                .context("handler ident must be valid UTF-8")?;
            let project_root =
                std::env::current_dir().context("failed to determine current directory")?;
            cmd_generate_handler::run(ident, &project_root)
        }
        GenerateKind::Playwright => {
            let target =
                playwright_target.context("--playwright-target is required when kind=playwright")?;
            cmd_generate_playwright::run(input, target)
        }
        GenerateKind::Ts => generate_ts(input, output, check),
    }
}

/// L0 #3 — emit TypeScript user-code for a Lazuli/Lazurite project.
/// Walks the package, runs every TS-side emitter (design tokens, per-feature
/// SDK, .lzx view hooks, slot interfaces, Zod schemas), and writes to
/// `dist/ts-<frontend>/`. Honors `Lazurite.toml [frontends.<name>]`.
fn generate_ts(input: &Path, output: Option<&Path>, check: bool) -> Result<()> {
    let project_root = project_root_for_input(input);
    let manifest = lazurite_manifest::load(&project_root).with_context(|| {
        format!(
            "failed to read {}",
            project_root.join("Lazurite.toml").display()
        )
    })?;
    let module = build_module_from_path(input)?;
    let lzx_bundle = collect_lzx_bundle(input);

    let mut files: Vec<lazuli_codegen_ts::GeneratedFile> = Vec::new();

    // Design tokens emission — same artifacts the legacy `generate_ts`
    // would have produced. Skips silently when `module.design` is None
    // (project hasn't authored design.lzi yet).
    if let Some(design) = module.design.as_ref() {
        files.extend(emit_design_files(design, &manifest));
    }

    // Mobile-target runtime: emit `dist/ts-mobile/runtime/layout.tsx`
    // once when the project declares an Expo frontend
    // (`docs/proposals/mobile-target.md` §5.4). The user-owned
    // `app/clients/mobile/app/_layout.tsx` is a one-line re-export of
    // this body; regen always rewrites this file.
    if manifest_has_expo_frontend(&manifest) {
        files.push(lazuli_codegen_ts::GeneratedFile {
            path: "dist/ts-mobile/runtime/layout.tsx".to_owned(),
            contents: lazuli_codegen_ts::mobile_runtime::emit_mobile_runtime_layout(),
        });
    }

    files.extend(
        lazuli_codegen_ts::lzx_audience_slot::emit_route_guard_artifacts(
            module.app.as_ref().or(lzx_bundle.app.as_ref()),
            &lzx_bundle.routes,
            &lzx_bundle.surfaces,
            &lzx_bundle.experiences,
            &module.features,
            lazuli_codegen_ts::lzx_audience_slot::RouteGuardTarget::Web,
        ),
    );
    files.push(lazuli_codegen_ts::GeneratedFile {
        path: "dist/ts-web/tests/fixtures.gen.ts".to_owned(),
        contents: lazuli_codegen_ts::playwright::emit_playwright_fixtures(
            &module,
            &lzx_bundle.routes,
            &lzx_bundle.surfaces,
            &lzx_bundle.experiences,
            &playwright_fixture_config(&project_root, manifest.as_ref()),
        ),
    });

    if let Some(contents) = lazuli_codegen_ts::emit_semantic_formatters_ts(&module) {
        for target_prefix in app_ts_target_prefixes(&module, &manifest) {
            files.push(lazuli_codegen_ts::GeneratedFile {
                path: format!("dist/{target_prefix}/runtime/formatters.gen.ts"),
                contents: contents.clone(),
            });
        }
    }

    // router-w1 (Wave 1): emit_routes_todo flipped — per-target routes.gen.tsx.
    let lzx_module = collect_lzx_experience_module(input);
    files.extend(lazuli_codegen_ts::routes::emit_routes_artifacts(
        lzx_module.app.as_ref().or(module.app.as_ref()),
        &lzx_module.routes,
        &lzx_module.surfaces,
        &lzx_module.experiences,
        lazuli_codegen_ts::routes::RoutesTarget::Web,
    ));
    files.extend(lazuli_codegen_ts::routes::emit_routes_artifacts(
        lzx_module.app.as_ref().or(module.app.as_ref()),
        &lzx_module.routes,
        &lzx_module.surfaces,
        &lzx_module.experiences,
        lazuli_codegen_ts::routes::RoutesTarget::Mobile,
    ));

    // Per-feature: SDK (audience-filtered if frontend declares audiences),
    // Zod schemas, .lzx view hooks (one file per audience/view tuple),
    // slot interfaces (one per @client.<slot> binding).
    let mut features: Vec<&lazuli_ir::Feature> = module.features.iter().collect();
    features.sort_by(|a, b| a.name.cmp(&b.name));
    for feature in features {
        files.extend(emit_feature_ts_artifacts(feature, &module, &manifest));
    }

    if check {
        println!("lazuli generate ts --check");
        println!("would emit {} file(s):", files.len());
        for file in &files {
            println!("  {}", file.path);
        }
        return Ok(());
    }

    // Emitters return project-relative paths (e.g. `dist/ts-web/slug/...`).
    // When the user passes `--output <dir>` we honour it as a literal base
    // (legacy override + tests); otherwise default to project root so the
    // `dist/<target>/` prefix encoded in each path lands at its canonical
    // location. The manifest's `[frontends.<x>].out` is declarative — it
    // describes WHERE the dist directory lives but is NOT a join prefix.
    let out_dir = output.map(Path::to_path_buf).unwrap_or(project_root);

    fs::create_dir_all(&out_dir)
        .with_context(|| format!("creating output directory {}", out_dir.display()))?;

    for file in &files {
        write_generated_file(&out_dir, &file.path, &file.contents)?;
    }

    // Per-view mobile scaffolds. Each mobile surface view writes one
    // `app/clients/mobile/app/<audience>/<expo-route>.tsx` placeholder
    // ONCE (idempotent — never overwrites user edits, mirroring
    // `cmd_new_frontends::scaffold_frontend_mobile`). Author replaces
    // the placeholder JSX with real RN components as soon as the
    // component library is chosen. See
    // `docs/proposals/mobile-target.md` §5.2.
    let scaffold_count = scaffold_mobile_view_files(&module, &out_dir)?;

    println!(
        "wrote {} file(s) to {} ({} mobile view scaffold{} written)",
        files.len(),
        out_dir.display(),
        scaffold_count,
        if scaffold_count == 1 { "" } else { "s" }
    );
    Ok(())
}

/// Walk every mobile surface and write a per-view scaffold under
/// `app/clients/mobile/app/<audience>/<expo-route>.tsx`. Returns the count
/// of files actually written (excludes already-present files left
/// untouched by the `write_if_absent` guard).
fn scaffold_mobile_view_files(
    module: &lazuli_ir::Module,
    out_dir: &Path,
) -> Result<usize> {
    let mut written = 0usize;

    for feature in &module.features {
        for surface in &feature.surfaces {
            if !matches!(surface.target, lazuli_ir::SurfaceTarget::Mobile) {
                continue;
            }
            for audience in &surface.audiences {
                for view in &audience.views {
                    let route = view_route_string(view);
                    let path = lazuli_codegen_ts::mobile_view_scaffold::expo_app_file_path(
                        &audience.name,
                        &route,
                    );
                    let abs_path = out_dir.join(&path);
                    if abs_path.exists() {
                        continue;
                    }
                    let body = lazuli_codegen_ts::mobile_view_scaffold::scaffold_body_for_view(
                        &surface.feature,
                        &audience.name,
                        view,
                    );
                    if let Some(parent) = abs_path.parent() {
                        fs::create_dir_all(parent).with_context(|| {
                            format!("creating {} for mobile scaffold", parent.display())
                        })?;
                    }
                    fs::write(&abs_path, body).with_context(|| {
                        format!("writing mobile scaffold {}", abs_path.display())
                    })?;
                    written += 1;
                }
            }
        }
    }

    Ok(written)
}

/// Extract the `at "<path>"` string from a view declaration. Stored as
/// `route: Option<String>` on each view kind in the IR. Falls back to
/// `/` for views that omit the clause entirely (rare — Expo Router's
/// `app/<audience>/index.tsx` is the natural landing target).
fn view_route_string(view: &lazuli_ir::View) -> String {
    match view {
        lazuli_ir::View::List(v) => v.route.clone().unwrap_or_else(|| "/".to_owned()),
        lazuli_ir::View::Detail(v) => v.route.clone().unwrap_or_else(|| "/".to_owned()),
        lazuli_ir::View::Create(v) => v.route.clone().unwrap_or_else(|| "/".to_owned()),
    }
}

/// Stub design emission walker. Wires the 6 design emitters from L0 #2
/// Cell B in `lazuli_codegen_ts::design`.
fn emit_design_files(
    design: &lazuli_ir::Design,
    _manifest: &Option<lazurite_manifest::Manifest>,
) -> Vec<lazuli_codegen_ts::GeneratedFile> {
    // Hook point: Cell B's individual emitters live as `pub fn emit_*` in
    // `lazuli_codegen_ts::design::*`. Wire them inline here so the CLI
    // doesn't depend on a yet-to-exist `lazuli_codegen_ts::generate_design`.
    let mut out = Vec::new();
    out.push(lazuli_codegen_ts::GeneratedFile {
        path: "dist/ts-web/design/tokens.ts".to_owned(),
        contents: lazuli_codegen_ts::design::emit_tokens_ts(design),
    });
    out.push(lazuli_codegen_ts::GeneratedFile {
        path: "dist/ts-web/design/tokens.css".to_owned(),
        contents: lazuli_codegen_ts::design::emit_tokens_css(design),
    });
    out.push(lazuli_codegen_ts::GeneratedFile {
        path: "dist/ts-web/design/tailwind.gen.ts".to_owned(),
        contents: lazuli_codegen_ts::design::emit_tailwind_v3_preset(design),
    });
    out.push(lazuli_codegen_ts::GeneratedFile {
        path: "dist/ts-web/design/tailwind.theme.css".to_owned(),
        contents: lazuli_codegen_ts::design::emit_tailwind_v4_theme(design),
    });
    out.push(lazuli_codegen_ts::GeneratedFile {
        path: "dist/ts-web/design/allowlist.json".to_owned(),
        contents: lazuli_codegen_ts::design::emit_allowlist_json(design),
    });
    out
}

/// Per-feature TS emission walker. Wires the .lzx view emitters from
/// Wave 3 Cell B (`lazuli_codegen_ts::lzx::emit_surface_views`).
fn emit_feature_ts_artifacts(
    feature: &lazuli_ir::Feature,
    module: &lazuli_ir::Module,
    manifest: &Option<lazurite_manifest::Manifest>,
) -> Vec<lazuli_codegen_ts::GeneratedFile> {
    let mut out = Vec::new();
    let target_prefixes = feature_ts_target_prefixes(feature, manifest);
    if !feature.resources.is_empty()
        || !feature.records.is_empty()
        || !feature.commands.is_empty()
        || !feature.queries.is_empty()
    {
        for target_prefix in &target_prefixes {
            out.push(lazuli_codegen_ts::GeneratedFile {
                path: format!(
                    "dist/{}/{}/{}.gen.ts",
                    target_prefix, feature.name, feature.name
                ),
                contents: emit_feature_sdk_ts(feature, module),
            });
            out.push(lazuli_codegen_ts::GeneratedFile {
                path: format!(
                    "dist/{}/{}/{}.zod.ts",
                    target_prefix, feature.name, feature.name
                ),
                contents: emit_feature_zod_ts(feature, module),
            });
            if *target_prefix == "ts-web" {
                if let Some(contents) = lazuli_codegen_ts::emit_cap_file_hooks_ts(feature) {
                    out.push(lazuli_codegen_ts::GeneratedFile {
                        path: format!(
                            "dist/{}/{}/{}.react.gen.ts",
                            target_prefix, feature.name, feature.name
                        ),
                        contents,
                    });
                }
            }
        }
    }
    for target_prefix in &target_prefixes {
        if let Some(contents) =
            lazuli_codegen_ts::lzx_route_params::emit_route_params_ts(feature, module, target_prefix)
        {
            out.push(lazuli_codegen_ts::GeneratedFile {
                path: format!(
                    "dist/{}/{}/{}.routes.gen.ts",
                    target_prefix, feature.name, feature.name
                ),
                contents,
            });
        }
    }
    let app_name = manifest
        .as_ref()
        .map(|m| m.project.name.as_str())
        .unwrap_or("");
    for surface in &feature.surfaces {
        let target = match surface.target {
            lazuli_ir::SurfaceTarget::Web => {
                lazuli_codegen_ts::lzx::lzx_router_adapter::RouterTarget::ViteReact
            }
            lazuli_ir::SurfaceTarget::Mobile => {
                lazuli_codegen_ts::lzx::lzx_router_adapter::RouterTarget::Expo
            }
        };
        // surface carries its feature owner; emitter resolves refs internally.
        let _ = feature;
        out.extend(lazuli_codegen_ts::lzx::emit_surface_views(
            surface, target, app_name,
        ));
    }
    out
}

/// True when the manifest declares at least one Expo frontend. Drives
/// the singleton `dist/ts-mobile/runtime/layout.tsx` emission per
/// `docs/proposals/mobile-target.md` §5.4. Manifest-less generation
/// (legacy/test paths) returns false — the runtime layout only matters
/// when an Expo-targeted scaffold consumes it.
fn manifest_has_expo_frontend(manifest: &Option<lazurite_manifest::Manifest>) -> bool {
    manifest
        .as_ref()
        .map(|m| {
            m.frontends
                .values()
                .any(|f| matches!(f.target, lazurite_manifest::FrontendTarget::Expo))
        })
        .unwrap_or(false)
}

fn feature_ts_target_prefixes(
    feature: &lazuli_ir::Feature,
    manifest: &Option<lazurite_manifest::Manifest>,
) -> BTreeSet<&'static str> {
    let mut targets = BTreeSet::new();
    if let Some(manifest) = manifest {
        for frontend in manifest.frontends.values() {
            match frontend.target {
                lazurite_manifest::FrontendTarget::TanstackVite => {
                    targets.insert("ts-web");
                }
                lazurite_manifest::FrontendTarget::Expo => {
                    targets.insert("ts-mobile");
                }
            }
        }
    }
    for surface in &feature.surfaces {
        match surface.target {
            lazuli_ir::SurfaceTarget::Web => {
                targets.insert("ts-web");
            }
            lazuli_ir::SurfaceTarget::Mobile => {
                targets.insert("ts-mobile");
            }
        }
    }
    if targets.is_empty() {
        targets.insert("ts-web");
    }
    targets
}

fn app_ts_target_prefixes(
    module: &lazuli_ir::Module,
    manifest: &Option<lazurite_manifest::Manifest>,
) -> BTreeSet<&'static str> {
    let mut targets = BTreeSet::new();
    if let Some(manifest) = manifest {
        for frontend in manifest.frontends.values() {
            match frontend.target {
                lazurite_manifest::FrontendTarget::TanstackVite => {
                    targets.insert("ts-web");
                }
                lazurite_manifest::FrontendTarget::Expo => {
                    targets.insert("ts-mobile");
                }
            }
        }
    }
    for feature in &module.features {
        for surface in &feature.surfaces {
            match surface.target {
                lazuli_ir::SurfaceTarget::Web => {
                    targets.insert("ts-web");
                }
                lazuli_ir::SurfaceTarget::Mobile => {
                    targets.insert("ts-mobile");
                }
            }
        }
    }
    if targets.is_empty() {
        targets.insert("ts-web");
    }
    targets
}

fn emit_feature_sdk_ts(feature: &lazuli_ir::Feature, module: &lazuli_ir::Module) -> String {
    let mut s = String::new();
    writeln!(s, "// Code generated by lazuli; DO NOT EDIT.").ok();
    writeln!(
        s,
        "import {{ defineCommand, defineQuery, type ID, type Money, type Time }} from \"@lazuli/runtime\";"
    )
    .ok();
    writeln!(s).ok();
    write_cross_feature_imports(&mut s, feature, module);
    write_plugin_semantic_aliases(&mut s, feature);
    write_referenced_enum_aliases(&mut s, feature, module);

    let mut records: Vec<&lazuli_ir::Record> = feature.records.iter().collect();
    records.sort_by(|a, b| a.name.cmp(&b.name));
    for record in records {
        write_record_interface(&mut s, record, module);
    }

    let mut resources: Vec<&lazuli_ir::Resource> = feature.resources.iter().collect();
    resources.sort_by(|a, b| a.name.cmp(&b.name));
    for resource in resources {
        write_resource_interface(&mut s, resource, module);
    }

    let mut commands: Vec<&lazuli_ir::Command> = feature.commands.iter().collect();
    commands.sort_by(|a, b| a.name.cmp(&b.name));
    for command in commands {
        write_command_sdk(&mut s, feature, command, module);
    }

    let mut queries: Vec<&lazuli_ir::Query> = feature.queries.iter().collect();
    queries.sort_by(|a, b| a.name().cmp(b.name()));
    for query in queries {
        write_query_sdk(&mut s, feature, query, module);
    }

    s
}

/// Emit `import { X } from '../other-feature/other-feature.gen';` lines
/// for every enum/record referenced by this feature but declared in
/// another feature. Closes WAR-CODEGEN-TS-01 + WAR-CODEGEN-XFEAT-01/02:
/// previously such cross-feature references silently dropped the import
/// and produced `tsc` errors at the consumer site, forcing users to
/// duplicate enums/records across every consuming feature.
fn write_cross_feature_imports(
    s: &mut String,
    feature: &lazuli_ir::Feature,
    module: &lazuli_ir::Module,
) {
    // Map of owner-feature name → set of type names imported from it.
    let mut imports: std::collections::BTreeMap<String, BTreeSet<String>> =
        std::collections::BTreeMap::new();
    collect_cross_feature_refs(feature, module, &mut imports);

    if imports.is_empty() {
        return;
    }

    let mut emitted = false;
    for (owner_feature, names) in &imports {
        let mut sorted: Vec<&String> = names.iter().collect();
        sorted.sort();
        let joined = sorted
            .iter()
            .map(|n| pascal_case(n))
            .collect::<Vec<_>>()
            .join(", ");
        // Emit both import (for local use in resource/command shapes)
        // AND re-export (so existing consumer code that imports the
        // type from this feature's .gen.ts continues to work after a
        // duplicate alias is removed). `export type { ... }` is
        // required because enum/record cross-feature refs are
        // type-only and isolatedModules rejects bare `export { ... }`
        // when the symbol carries no value.
        writeln!(
            s,
            "import type {{ {joined} }} from \"../{owner_feature}/{owner_feature}.gen\";"
        )
        .ok();
        writeln!(
            s,
            "export type {{ {joined} }} from \"../{owner_feature}/{owner_feature}.gen\";"
        )
        .ok();
        emitted = true;
    }
    if emitted {
        writeln!(s).ok();
    }
}

/// Walk every field/slot of every record/resource/command/query in
/// `feature` and accumulate the set of enum/record names that are
/// referenced but DECLARED in another feature.
fn collect_cross_feature_refs(
    feature: &lazuli_ir::Feature,
    module: &lazuli_ir::Module,
    out: &mut std::collections::BTreeMap<String, BTreeSet<String>>,
) {
    let walk_type = |type_ref: &lazuli_ir::TypeRef,
                     out: &mut std::collections::BTreeMap<String, BTreeSet<String>>| {
        let mut stack: Vec<&lazuli_ir::TypeRef> = vec![type_ref];
        while let Some(t) = stack.pop() {
            match t {
                lazuli_ir::TypeRef::Many(inner) => stack.push(inner),
                lazuli_ir::TypeRef::EnumRef(qn) | lazuli_ir::TypeRef::UserDefined(qn) => {
                    if let Some(owner) = owner_feature_for_type(qn, module, feature) {
                        out.entry(owner)
                            .or_insert_with(BTreeSet::new)
                            .insert(qn.name.clone());
                    }
                }
                _ => {}
            }
        }
    };
    for record in &feature.records {
        for field in &record.fields {
            walk_type(&field.type_ref, out);
        }
    }
    for resource in &feature.resources {
        for field in &resource.fields {
            walk_type(&field.type_ref, out);
        }
    }
    for command in &feature.commands {
        for slot in command_sdk_slots(feature, command, module) {
            walk_type(&slot.type_ref, out);
        }
        if let lazuli_ir::CommandEffect::Returns(effect) = &command.effect {
            walk_type(&effect.return_type, out);
        }
    }
    for query in &feature.queries {
        for slot in query_args(feature, query, module) {
            walk_type(&slot.type_ref, out);
        }
    }
}

/// Resolve a type reference to its owner feature name, but only when
/// the type lives in a DIFFERENT feature than `consumer`. Returns None
/// when the type is local (no import needed), defined in both consumer
/// and another feature (treat the duplicate as local — happens when
/// authors copy enums between features per WAR-CODEGEN-XFEAT-01), or
/// builtin/unresolvable.
fn owner_feature_for_type(
    qn: &lazuli_ir::QualifiedName,
    module: &lazuli_ir::Module,
    consumer: &lazuli_ir::Feature,
) -> Option<String> {
    let local_hit = consumer
        .enums
        .iter()
        .any(|e| e.name.eq_ignore_ascii_case(&qn.name))
        || consumer
            .records
            .iter()
            .any(|r| r.name.eq_ignore_ascii_case(&qn.name));
    if local_hit {
        return None;
    }
    // Honor the QualifiedName.feature hint if present (preferred owner).
    if let Some(hint) = qn.feature.as_deref() {
        if module.features.iter().any(|f| f.name == hint) {
            return Some(hint.to_owned());
        }
    }
    // Otherwise, find the first feature that declares this enum/record.
    for feature in &module.features {
        if feature.name == consumer.name {
            continue;
        }
        let owns = feature
            .enums
            .iter()
            .any(|e| e.name.eq_ignore_ascii_case(&qn.name))
            || feature
                .records
                .iter()
                .any(|r| r.name.eq_ignore_ascii_case(&qn.name));
        if owns {
            return Some(feature.name.clone());
        }
    }
    None
}

/// B3 — emit `export type <Name> = string;` brand aliases for every
/// plugin-contributed `@semantic.<Name>` referenced by this feature.
/// Per `docs/proposals/semantic-types-plugin-locales.md` §Codegen the
/// TS layer is type-only — no runtime validation — so an opaque alias
/// is the right surface. The Go side keeps the validate dispatch.
///
/// Sorted output keeps generated TS byte-stable across runs.
fn write_plugin_semantic_aliases(s: &mut String, feature: &lazuli_ir::Feature) {
    let mut aliases: BTreeSet<String> = BTreeSet::new();
    collect_plugin_semantic_aliases_in_feature(feature, &mut aliases);
    if aliases.is_empty() {
        return;
    }
    writeln!(
        s,
        "// Plugin-contributed semantic types (docs/proposals/semantic-types-plugin-locales.md)."
    )
    .ok();
    for name in aliases {
        // Carrier is `Text` in v1 → `string`. The proposal closed
        // carrier catalog locks to `String`; widening needs a separate
        // proposal that also threads a non-string TS shape.
        writeln!(s, "export type {} = string;", pascal_case(&name)).ok();
    }
    writeln!(s).ok();
}

fn collect_plugin_semantic_aliases_in_feature(
    feature: &lazuli_ir::Feature,
    out: &mut BTreeSet<String>,
) {
    for resource in &feature.resources {
        for field in &resource.fields {
            collect_plugin_semantic_aliases_in_type(&field.type_ref, out);
        }
    }
    for record in &feature.records {
        for field in &record.fields {
            collect_plugin_semantic_aliases_in_type(&field.type_ref, out);
        }
    }
    for event in &feature.events {
        for field in &event.payload {
            collect_plugin_semantic_aliases_in_type(&field.type_ref, out);
        }
    }
    for command in &feature.commands {
        if let lazuli_ir::CommandInput::Typed(slots) = &command.input {
            for slot in slots {
                collect_plugin_semantic_aliases_in_type(&slot.type_ref, out);
            }
        }
    }
}

fn collect_plugin_semantic_aliases_in_type(
    type_ref: &lazuli_ir::TypeRef,
    out: &mut BTreeSet<String>,
) {
    match type_ref {
        lazuli_ir::TypeRef::Builtin(lazuli_ir::BuiltinType::SemanticPluginType { name, .. }) => {
            out.insert(name.clone());
        }
        lazuli_ir::TypeRef::Many(inner) => {
            collect_plugin_semantic_aliases_in_type(inner, out);
        }
        _ => {}
    }
}

fn write_referenced_enum_aliases(
    s: &mut String,
    feature: &lazuli_ir::Feature,
    module: &lazuli_ir::Module,
) {
    let mut referenced = BTreeSet::new();
    collect_referenced_feature_enums(feature, module, &mut referenced);
    // Closes WAR-CODEGEN-TS-01: also emit enums referenced by OTHER
    // features (via cross-feature import). Without this, the owner
    // feature's .gen.ts wouldn't export the type the consumer imports.
    for other in &module.features {
        if other.name == feature.name {
            continue;
        }
        let mut other_refs = BTreeSet::new();
        collect_referenced_feature_enums(other, module, &mut other_refs);
        for r in other_refs {
            if feature.enums.iter().any(|e| e.name == r) {
                referenced.insert(r);
            }
        }
        // Also walk cross-feature import collection — captures enums
        // used in command inputs/outputs that the simple "referenced"
        // walk may have missed for the consumer side.
        let mut cross = std::collections::BTreeMap::new();
        collect_cross_feature_refs(other, module, &mut cross);
        if let Some(names) = cross.get(&feature.name) {
            for n in names {
                if feature.enums.iter().any(|e| e.name == *n) {
                    referenced.insert(n.clone());
                }
            }
        }
    }

    let mut emitted = false;
    let mut enums: Vec<&lazuli_ir::EnumDecl> = feature.enums.iter().collect();
    enums.sort_by(|a, b| a.name.cmp(&b.name));
    for enum_decl in enums {
        if !referenced.contains(&enum_decl.name) {
            continue;
        }
        let type_name = pascal_case(&enum_decl.name);
        let const_name = enum_value_constant_name(&enum_decl.name);
        let options_name = enum_option_constant_name(&enum_decl.name);
        let values = enum_decl
            .variants
            .iter()
            .map(enum_variant_ts_literal)
            .collect::<Vec<_>>()
            .join(", ");
        writeln!(s, "export const {const_name} = [{values}] as const;").ok();
        writeln!(s, "export type {type_name} = typeof {const_name}[number];").ok();
        if enum_has_option_metadata(enum_decl) {
            write_enum_options_alias(s, enum_decl, &type_name, &options_name);
        }
        emitted = true;
    }
    if emitted {
        writeln!(s).ok();
    }
}

fn collect_referenced_feature_enums(
    feature: &lazuli_ir::Feature,
    module: &lazuli_ir::Module,
    out: &mut BTreeSet<String>,
) {
    for record in &feature.records {
        for field in &record.fields {
            collect_enum_ref(&field.type_ref, feature, out);
        }
    }
    for resource in &feature.resources {
        for field in &resource.fields {
            collect_enum_ref(&field.type_ref, feature, out);
        }
    }
    for command in &feature.commands {
        for slot in command_sdk_slots(feature, command, module) {
            collect_enum_ref(&slot.type_ref, feature, out);
        }
        if let lazuli_ir::CommandEffect::Returns(effect) = &command.effect {
            collect_enum_ref(&effect.return_type, feature, out);
        }
    }
    for query in &feature.queries {
        for slot in query_args(feature, query, module) {
            collect_enum_ref(&slot.type_ref, feature, out);
        }
    }
}

fn collect_enum_ref(
    type_ref: &lazuli_ir::TypeRef,
    feature: &lazuli_ir::Feature,
    out: &mut BTreeSet<String>,
) {
    match type_ref {
        lazuli_ir::TypeRef::EnumRef(name) if enum_ref_matches_feature(feature, name) => {
            out.insert(name.name.clone());
        }
        // UserDefined-tagged enum fields. Parallel to the
        // `UserDefined → enum_decl` fallback in `ts_type_for_type_ref`:
        // when the analyzer leaves an enum reference as
        // `UserDefined("CustomerTier")` (default-bearing fields seem
        // to take this path), the emitter resolves it to a real enum
        // — but the alias only lands at the top of the file if we ALSO
        // record the reference here. Without this branch the generated
        // TS references an undeclared `CustomerTier` symbol.
        lazuli_ir::TypeRef::UserDefined(name) if enum_ref_matches_feature(feature, name) => {
            out.insert(name.name.clone());
        }
        lazuli_ir::TypeRef::Many(inner) => collect_enum_ref(inner, feature, out),
        // Bare-name `Unresolved` fallback for the same reason — kept
        // narrow so we never invent a reference: the bare name MUST
        // already exist in the same feature's enum catalog.
        lazuli_ir::TypeRef::Unresolved(raw) if !raw.starts_with('@') => {
            if feature
                .enums
                .iter()
                .any(|enum_decl| enum_decl.name.eq_ignore_ascii_case(raw))
            {
                out.insert(raw.clone());
            }
        }
        _ => {}
    }
}

fn enum_ref_matches_feature(feature: &lazuli_ir::Feature, name: &lazuli_ir::QualifiedName) -> bool {
    name.feature
        .as_ref()
        .is_none_or(|owner| owner == &feature.name)
        && feature
            .enums
            .iter()
            .any(|enum_decl| enum_decl.name.eq_ignore_ascii_case(&name.name))
}

fn write_record_interface(s: &mut String, record: &lazuli_ir::Record, module: &lazuli_ir::Module) {
    // Field keys in camelCase — idiomatic JS/TS. The wire JSON
    // contract stays snake_case (Go runtime); `LazuliClient` re-keys
    // at the boundary via `runtime/ts/lazuli/src/case-mapper.ts`.
    writeln!(s, "export interface {} {{", pascal_case(&record.name)).ok();
    let mut fields: Vec<&lazuli_ir::Field> = record.fields.iter().collect();
    fields.sort_by(|a, b| a.name.cmp(&b.name));
    for field in fields {
        let ty = ts_type_for_type_ref(&field.type_ref, module);
        let camel = lazuli_codegen_ts::lower_camel_export(&field.name);
        if field.required {
            writeln!(s, "  {}: {};", camel, ty).ok();
        } else {
            writeln!(s, "  {}?: {} | null;", camel, ty).ok();
        }
    }
    writeln!(s, "}}").ok();
    writeln!(s).ok();
}

fn write_resource_interface(
    s: &mut String,
    resource: &lazuli_ir::Resource,
    module: &lazuli_ir::Module,
) {
    writeln!(s, "export interface {} {{", pascal_case(&resource.name)).ok();
    writeln!(s, "  id: ID;").ok();
    let mut fields: Vec<&lazuli_ir::Field> = resource.fields.iter().collect();
    fields.sort_by(|a, b| a.name.cmp(&b.name));
    for field in fields {
        if matches!(
            field.name.as_str(),
            "id" | "created_at" | "updated_at" | "deleted_at"
        ) {
            continue;
        }
        let name = resource_field_ts_name(field, module);
        let camel = lazuli_codegen_ts::lower_camel_export(&name);
        let ty = resource_field_ts_type(field, module);
        if field.required {
            writeln!(s, "  {camel}: {ty};").ok();
        } else {
            writeln!(s, "  {camel}?: {ty} | null;").ok();
        }
    }
    writeln!(s, "  createdAt: Time;").ok();
    writeln!(s, "  updatedAt: Time;").ok();
    if resource.soft_delete {
        writeln!(s, "  deletedAt?: Time | null;").ok();
    }
    writeln!(s, "}}").ok();
    writeln!(s).ok();
}

fn write_command_sdk(
    s: &mut String,
    feature: &lazuli_ir::Feature,
    command: &lazuli_ir::Command,
    module: &lazuli_ir::Module,
) {
    let feature_pascal = pascal_case(&feature.name);
    let input_iface = command_input_iface(&command.name, &feature_pascal);
    let output_ty = command_output_ts_type(feature, command, module);

    writeln!(s, "export interface {input_iface} {{").ok();
    for slot in command_sdk_slots(feature, command, module) {
        let optional = if slot.required { "" } else { "?" };
        let camel = lazuli_codegen_ts::lower_camel_export(&slot.name);
        writeln!(
            s,
            "  {}{}: {};",
            camel,
            optional,
            ts_type_for_type_ref(&slot.type_ref, module)
        )
        .ok();
    }
    writeln!(s, "}}").ok();
    writeln!(s).ok();

    let invalidates: Vec<String> = command
        .invalidates
        .iter()
        .map(|i| {
            // Wire registry key: `<feature>.<query_name>` (cell B1 dropped
            // `.query.` infix). The pseudo-feature `query` (legacy parser
            // output for `query.<name>` same-feature shorthand) and the
            // None fallback both resolve to the host feature.
            let feature_name = match i.query.feature.as_deref() {
                Some("query") | None => feature.name.as_str(),
                Some(feat) => feat,
            };
            format!("{}.{}", feature_name, i.query.name)
        })
        .collect();
    // Wave 0 (ir-returns-list-2026-05-22 §2.2): pure-read commands lower
    // to `defineQuery` so the React app gets react-query semantics
    // (cache, refetch, suspense, useLazuliQuery). The wire is identical;
    // only the client-side factory differs. Non-read commands stay on
    // `defineCommand` and keep carrying invalidates / policy / rate-limit
    // / audit metadata for `useLazuliCommand` callers.
    if command_is_pure_read(command) {
        writeln!(
            s,
            "export const {} = defineQuery<{}, {}>(\"{}.{}\");",
            command_ident(&feature.name, &command.name),
            input_iface,
            output_ty,
            feature.name,
            command.name
        )
        .ok();
        writeln!(s).ok();
        return;
    }
    writeln!(
        s,
        "export const {} = defineCommand<{}, {}>(\"{}.{}\", {{",
        command_ident(&feature.name, &command.name),
        input_iface,
        output_ty,
        feature.name,
        command.name
    )
    .ok();
    writeln!(s, "  invalidates: {},", format_string_array(&invalidates)).ok();
    // Operational metadata (review bug #7, 2026-05-15) — the Go side
    // already carries Policy / RateLimit / Audit on `lazuli.Command[I,O]`.
    // The TS SDK previously lost them, so clients had no way to drive
    // policy-aware affordances or rate-limit-aware backoff without a
    // separate metadata call.
    if let Some(policy_literal) = format_policy_ts(&command.policy, feature) {
        writeln!(s, "  policy: {policy_literal},").ok();
    }
    if let Some(rate_limit) = command.rate_limit.as_ref() {
        // `ir-rate-limit-env-aware` cell 1 — SDK shim: surface the
        // default literal. Cell 2 extends the wire shape to carry the
        // env-qualified slice for client-side affordance.
        writeln!(
            s,
            "  rateLimit: \"{}\",",
            escape_js_string(&rate_limit.default)
        )
        .ok();
    }
    if let Some(audit_literal) = format_audit_ts(command.audit.as_ref()) {
        writeln!(s, "  audit: {audit_literal},").ok();
    }
    writeln!(s, "}});").ok();
    writeln!(s).ok();
}

fn write_query_sdk(
    s: &mut String,
    feature: &lazuli_ir::Feature,
    query: &lazuli_ir::Query,
    module: &lazuli_ir::Module,
) {
    let args = query_args(feature, query, module);
    let args_ty = if args.is_empty() {
        "{}".to_owned()
    } else {
        let fields = args
            .iter()
            .map(|slot| {
                let optional = if slot.required { "" } else { "?" };
                format!(
                    "{}{}: {}",
                    slot.name,
                    optional,
                    ts_type_for_type_ref(&slot.type_ref, module)
                )
            })
            .collect::<Vec<_>>()
            .join("; ");
        format!("{{ {fields} }}")
    };
    // Pick the resource most likely matching the query's intent.
    // Previous heuristic was `feature.resources.first()` which produced
    // wildly wrong types when the first resource isn't the "main" one
    // (e.g. `host.lookupHostByMyHost` typed as `IntermediationTermsAcceptance`
    // because that's the first resource declared in `host.lzi`; see
    // WAR-VOCAB-HOSTHOME-01). New heuristic: find a resource whose
    // PascalCase name appears as a token in the query name, falling
    // back to the first resource when no match is found.
    let resource_ty = pick_query_resource_ts(feature, query.name())
        .unwrap_or_else(|| {
            feature
                .resources
                .first()
                .map(|r| pascal_case(&r.name))
                .unwrap_or_else(|| "unknown".to_owned())
        });
    let returns = match query {
        lazuli_ir::Query::Lookup(_) => resource_ty,
        lazuli_ir::Query::List(_) => format!("{resource_ty}[]"),
        lazuli_ir::Query::Sql(q) => ts_type_for_type_ref(&q.returns, module),
    };
    let query_ref_kind = match query {
        lazuli_ir::Query::List(_) => lazuli_ir::QueryKind::List,
        lazuli_ir::Query::Lookup(_) => lazuli_ir::QueryKind::Lookup,
        lazuli_ir::Query::Sql(q) => match q.sql_kind {
            lazuli_ir::SqlQueryKind::Sql => lazuli_ir::QueryKind::Sql,
            lazuli_ir::SqlQueryKind::View => lazuli_ir::QueryKind::View,
        },
    };
    // Query-side operational metadata (review bug #7, 2026-05-15).
    // Today `lazuli_ir::Query` carries no explicit policy/rate_limit at
    // the variant level — `query.list/lookup/sql` are universally
    // readable inside a tenant (see audience_sdk.rs's note). The TS
    // signature already accepts a `DefineQueryOptions` block so when
    // policy lands on Query the codegen will populate it here without
    // a runtime contract change.
    writeln!(
        s,
        // Wire registry key: `<feature>.<query_name>` (cell B1 dropped
        // `.query.` infix — the `/q/` HTTP prefix already disambiguates kind).
        "export const {} = defineQuery<{}, {}>(\"{}.{}\");",
        query_ident(&feature.name, query_ref_kind, query.name()),
        args_ty,
        returns,
        feature.name,
        query.name()
    )
    .ok();
    writeln!(s).ok();
}

fn emit_feature_zod_ts(feature: &lazuli_ir::Feature, module: &lazuli_ir::Module) -> String {
    let mut s = String::new();
    writeln!(s, "// Code generated by lazuli; DO NOT EDIT.").ok();
    writeln!(s, "import {{ z }} from \"zod\";").ok();
    writeln!(s).ok();

    let feature_pascal = pascal_case(&feature.name);
    let mut commands: Vec<&lazuli_ir::Command> = feature.commands.iter().collect();
    commands.sort_by(|a, b| a.name.cmp(&b.name));
    for command in commands {
        let schema_ident = command_schema_ident(&command.name, &feature_pascal);
        writeln!(s, "export const {schema_ident} = z.object({{").ok();
        for slot in command_zod_slots(feature, command, module) {
            // Zod schemas mirror the camelCase SDK interfaces emitted
            // in `messaging.gen.ts` etc. The wire JSON contract stays
            // snake_case; `LazuliClient` rekeys at the boundary
            // (`case-mapper.ts`). Apps validating client-side state
            // (forms, local cache) speak in camelCase, matching
            // the typed interface.
            writeln!(
                s,
                "  {}: {},",
                lazuli_codegen_ts::lower_camel_export(&slot.name),
                zod_expr_for_slot(&slot.type_ref, &slot.constraints, !slot.required)
            )
            .ok();
        }
        writeln!(s, "}});").ok();
        writeln!(s).ok();
    }

    s
}

#[derive(Clone)]
struct TsSlot {
    name: String,
    type_ref: lazuli_ir::TypeRef,
    required: bool,
    constraints: lazuli_ir::FieldConstraints,
}

fn command_sdk_slots(
    feature: &lazuli_ir::Feature,
    command: &lazuli_ir::Command,
    module: &lazuli_ir::Module,
) -> Vec<TsSlot> {
    let mut slots = Vec::new();
    for route in &command.route {
        slots.push(TsSlot {
            name: route.name.clone(),
            type_ref: route.type_ref.clone(),
            required: route.from.is_none(),
            constraints: lazuli_ir::FieldConstraints::default(),
        });
    }
    slots.extend(command_input_slots(feature, command, module));
    slots
}

fn command_zod_slots(
    feature: &lazuli_ir::Feature,
    command: &lazuli_ir::Command,
    module: &lazuli_ir::Module,
) -> Vec<TsSlot> {
    let input_slots = command_input_slots(feature, command, module);
    if input_slots.is_empty() {
        command
            .route
            .iter()
            .map(|route| TsSlot {
                name: route.name.clone(),
                type_ref: route.type_ref.clone(),
                required: route.from.is_none(),
                constraints: lazuli_ir::FieldConstraints::default(),
            })
            .collect()
    } else {
        input_slots
    }
}

fn command_input_slots(
    feature: &lazuli_ir::Feature,
    command: &lazuli_ir::Command,
    module: &lazuli_ir::Module,
) -> Vec<TsSlot> {
    match &command.input {
        lazuli_ir::CommandInput::Empty => Vec::new(),
        lazuli_ir::CommandInput::Typed(slots) => slots
            .iter()
            .map(|slot| TsSlot {
                name: slot.name.clone(),
                type_ref: slot.type_ref.clone(),
                required: slot.required,
                constraints: slot.constraints.clone(),
            })
            .collect(),
        lazuli_ir::CommandInput::Short(names) => {
            let resource = command_resource(feature, command, module);
            names
                .iter()
                .map(|name| {
                    let field = resource.and_then(|r| r.fields.iter().find(|f| f.name == *name));
                    TsSlot {
                        name: name.clone(),
                        type_ref: field
                            .map(|f| f.type_ref.clone())
                            .unwrap_or(lazuli_ir::TypeRef::Builtin(lazuli_ir::BuiltinType::Text)),
                        required: field.map(|f| f.required).unwrap_or(true),
                        constraints: field.map(|f| f.constraints.clone()).unwrap_or_default(),
                    }
                })
                .collect()
        }
    }
}

fn query_args(
    feature: &lazuli_ir::Feature,
    query: &lazuli_ir::Query,
    module: &lazuli_ir::Module,
) -> Vec<TsSlot> {
    match query {
        lazuli_ir::Query::List(q) => q.params.iter().map(ts_slot_from_typed).collect(),
        lazuli_ir::Query::Sql(q) => q.params.iter().map(ts_slot_from_typed).collect(),
        lazuli_ir::Query::Lookup(q) => {
            let mut slots: Vec<TsSlot> = q.params.iter().map(ts_slot_from_typed).collect();
            if slots.is_empty() {
                for key in &q.keys {
                    if let lazuli_ir::Expr::Path(path) = &key.equals {
                        if path.segments.first().is_some_and(|s| s == "input") {
                            if let Some(name) = path.segments.get(1) {
                                slots.push(query_input_slot(feature, module, name));
                            }
                        }
                    }
                }
            }
            if slots.is_empty() {
                collect_input_slots_from_filters(feature, module, &q.filters, &mut slots);
            }
            if slots.is_empty() {
                if let Some(name) = q.name.strip_prefix("by_") {
                    slots.push(query_input_slot(feature, module, name));
                }
            }
            slots
        }
    }
}

fn collect_input_slots_from_filters(
    feature: &lazuli_ir::Feature,
    module: &lazuli_ir::Module,
    filters: &[lazuli_ir::Filter],
    slots: &mut Vec<TsSlot>,
) {
    for filter in filters {
        collect_input_slots_from_predicate(feature, module, &filter.predicate, slots);
    }
}

fn collect_input_slots_from_predicate(
    feature: &lazuli_ir::Feature,
    module: &lazuli_ir::Module,
    predicate: &lazuli_ir::Predicate,
    slots: &mut Vec<TsSlot>,
) {
    match predicate {
        lazuli_ir::Predicate::Comparison { left, right, .. } => {
            collect_input_slot_from_expr(feature, module, left, slots);
            collect_input_slot_from_expr(feature, module, right, slots);
        }
        lazuli_ir::Predicate::Has {
            collection,
            element,
        } => {
            collect_input_slot_from_expr(feature, module, collection, slots);
            collect_input_slot_from_expr(feature, module, element, slots);
        }
        lazuli_ir::Predicate::And(predicates) | lazuli_ir::Predicate::Or(predicates) => {
            for predicate in predicates {
                collect_input_slots_from_predicate(feature, module, predicate, slots);
            }
        }
    }
}

fn collect_input_slot_from_expr(
    feature: &lazuli_ir::Feature,
    module: &lazuli_ir::Module,
    expr: &lazuli_ir::Expr,
    slots: &mut Vec<TsSlot>,
) {
    let lazuli_ir::Expr::Path(path) = expr else {
        return;
    };
    if !path
        .segments
        .first()
        .is_some_and(|segment| segment == "input")
    {
        return;
    }
    let Some(name) = path.segments.get(1) else {
        return;
    };
    if slots.iter().any(|slot| slot.name == *name) {
        return;
    }
    slots.push(query_input_slot(feature, module, name));
}

fn query_input_slot(
    feature: &lazuli_ir::Feature,
    module: &lazuli_ir::Module,
    name: &str,
) -> TsSlot {
    let field = feature
        .resources
        .first()
        .and_then(|resource| resource.fields.iter().find(|field| field.name == name))
        .or_else(|| {
            module
                .features
                .iter()
                .flat_map(|feature| feature.resources.iter())
                .flat_map(|resource| resource.fields.iter())
                .find(|field| field.name == name)
        });
    TsSlot {
        name: name.to_owned(),
        type_ref: field
            .map(|field| field.type_ref.clone())
            .or_else(|| {
                name.eq_ignore_ascii_case("id")
                    .then_some(lazuli_ir::TypeRef::Builtin(lazuli_ir::BuiltinType::Id))
            })
            .unwrap_or(lazuli_ir::TypeRef::Builtin(lazuli_ir::BuiltinType::Text)),
        required: true,
        constraints: field
            .map(|field| field.constraints.clone())
            .unwrap_or_default(),
    }
}

fn ts_slot_from_typed(slot: &lazuli_ir::TypedSlot) -> TsSlot {
    TsSlot {
        name: slot.name.clone(),
        type_ref: slot.type_ref.clone(),
        required: slot.required,
        constraints: slot.constraints.clone(),
    }
}

/// Pick the most likely resource for a `query.list` / `query.lookup` /
/// `query.sql` return type. Walks the feature's resources, returns the
/// one whose snake-cased name appears as a substring of the query
/// name (e.g. `my_host` → "host" → Host; `property_detail` → "property"
/// → Property). Returns None when no resource matches; caller falls
/// back to `feature.resources.first()`. Closes WAR-VOCAB-HOSTHOME-01.
fn pick_query_resource_ts(feature: &lazuli_ir::Feature, query_name: &str) -> Option<String> {
    let query_lc = query_name.to_ascii_lowercase();
    // Prefer the longest match (so "service_transaction" beats
    // "service" + "transaction" tie). Sort by length desc.
    let mut candidates: Vec<&lazuli_ir::Resource> = feature.resources.iter().collect();
    candidates.sort_by(|a, b| b.name.len().cmp(&a.name.len()));
    for resource in candidates {
        let snake = to_snake_case(&resource.name);
        if query_lc.contains(&snake) {
            return Some(pascal_case(&resource.name));
        }
        // Also try a token-by-token match for compound names like
        // "ServiceTransaction" vs query "transaction_detail".
        let last_token = snake.rsplit('_').next().unwrap_or("");
        if !last_token.is_empty() && last_token.len() > 3 && query_lc.contains(last_token) {
            return Some(pascal_case(&resource.name));
        }
    }
    None
}

/// Wave 0 (ir-returns-list-2026-05-22 §2.2): a command is a *pure read*
/// when its sole declared effect is `Returns(_)`, carries no declared
/// side-effects (no event emits, no lifecycle triggers, no invalidations,
/// no external calls), is NOT synthesized from `@cap.File` (those are
/// upload-protocol commands with implicit side effects the analyzer
/// doesn't surface as `emits`/`triggers`), AND its name starts with a
/// read-verb prefix (`list_`, `get_`, `lookup_`, `search_`, `find_`,
/// `count_`).
///
/// Pure-read commands lower to `defineQuery<I, O>` on the TS side
/// (consumable via `useLazuliQuery`) so the React app gets cache +
/// refetch + suspense semantics for free, instead of `defineCommand`
/// (which forces `useLazuliCommand` and imperative call sites). The
/// wire payload is identical — only the client-side factory differs.
///
/// The name-prefix gate exists because pilots and the analyzer leave
/// the IR side-effect surface empty for many side-effecting commands —
/// e.g. `account.login` (mints a session but has no `emits` because the
/// session table is private), `request_profile_photo_upload` (mints a
/// presigned URL but has no `triggers`). Trusting only the IR's empty
/// side-effect set produced false positives (W0-5 surfaced this:
/// hostpoint app failed to typecheck because login + photo-upload
/// commands shipped as `defineQuery`, breaking existing
/// `useLazuliCommand` callsites). The name-prefix gate makes the
/// classification conservative — false negatives (a read that doesn't
/// follow the naming convention) ship as `defineCommand`, which still
/// works; false positives ship a wire mismatch, which doesn't.
fn command_is_pure_read(command: &lazuli_ir::Command) -> bool {
    if !matches!(command.effect, lazuli_ir::CommandEffect::Returns(_)) {
        return false;
    }
    if !command.emits.is_empty()
        || !command.triggers.is_empty()
        || !command.invalidates.is_empty()
        || !command.external_calls.is_empty()
    {
        return false;
    }
    // cap_file synth: Request/Confirm/Clear are upload-protocol writes;
    // only GetUrl is a pure read (mints a signed download URL, no
    // mutation). c-2 worker surfaced this nuance; integrated 2026-05-22.
    if command
        .synthesized_from_cap_file
        .as_ref()
        .is_some_and(|marker| marker.role != lazuli_ir::AutoPhotoCommandRole::GetUrl)
    {
        return false;
    }
    const READ_VERB_PREFIXES: &[&str] = &[
        "list_", "get_", "lookup_", "search_", "find_", "count_",
    ];
    READ_VERB_PREFIXES
        .iter()
        .any(|prefix| command.name.starts_with(prefix))
}

fn command_output_ts_type(
    _feature: &lazuli_ir::Feature,
    command: &lazuli_ir::Command,
    module: &lazuli_ir::Module,
) -> String {
    match &command.effect {
        lazuli_ir::CommandEffect::Creates(effect) => resource_ts_name(&effect.resource, module),
        lazuli_ir::CommandEffect::Updates(effect) => resource_ts_name(&effect.resource, module),
        lazuli_ir::CommandEffect::Deletes(effect) => resource_ts_name(&effect.resource, module),
        // For `returns User` we want the full resource interface (User)
        // not the FK collapse to `ID`. `ts_type_for_type_ref` collapses
        // any `UserDefined(<Resource>)` to `ID` because that's correct
        // for resource-field positions (FK column). But the return
        // position carries the typed row — same fix as the Go side
        // (`types::go_return_type_for`).
        lazuli_ir::CommandEffect::Returns(effect) => {
            ts_return_type_for_type_ref(&effect.return_type, module)
        }
        // CommandEffect::None means the command has an `@fn.*` handler
        // with no declared return effect — the Go side returns `struct{}`
        // (empty object). TS surface mirrors that as `void`. Previously
        // this fell back to `feature.resources.first()`, which produced
        // wildly wrong types (e.g. every catalog command typed as
        // `UploadedAsset` — see WAR-VOCAB-HOSTPROPDETAIL-02).
        lazuli_ir::CommandEffect::None => "void".to_owned(),
    }
}

/// Variant of [`ts_type_for_type_ref`] that resolves resource refs to
/// their full interface name (`User`) instead of the FK collapse (`ID`).
/// Used by [`command_output_ts_type`] for `Returns` — the handler emits
/// the typed row, not the row id. Mirrors the Go side's
/// `go_return_type_for` / `command_output_type` split (see
/// `crates/lazuli_codegen_go/src/emitter/types.rs`).
fn ts_return_type_for_type_ref(
    type_ref: &lazuli_ir::TypeRef,
    module: &lazuli_ir::Module,
) -> String {
    match type_ref {
        lazuli_ir::TypeRef::UserDefined(name) if is_resource_ref(type_ref, module) => {
            // Skip the FK collapse — return the resource interface name.
            find_resource(module, name)
                .map(|r| pascal_case(&r.name))
                .unwrap_or_else(|| pascal_case(&name.name))
        }
        lazuli_ir::TypeRef::Many(inner) => {
            format!("{}[]", ts_return_type_for_type_ref(inner, module))
        }
        // Everything else (builtins, capabilities, enums, records,
        // unresolved) shares the same shape as field-position resolution.
        other => ts_type_for_type_ref(other, module),
    }
}

fn command_resource<'a>(
    feature: &'a lazuli_ir::Feature,
    command: &lazuli_ir::Command,
    module: &'a lazuli_ir::Module,
) -> Option<&'a lazuli_ir::Resource> {
    match &command.effect {
        lazuli_ir::CommandEffect::Creates(effect) => find_resource(module, &effect.resource),
        lazuli_ir::CommandEffect::Updates(effect) => find_resource(module, &effect.resource),
        lazuli_ir::CommandEffect::Deletes(effect) => find_resource(module, &effect.resource),
        lazuli_ir::CommandEffect::Returns(_) | lazuli_ir::CommandEffect::None => {
            feature.resources.first()
        }
    }
}

fn find_resource<'a>(
    module: &'a lazuli_ir::Module,
    name: &lazuli_ir::QualifiedName,
) -> Option<&'a lazuli_ir::Resource> {
    module
        .features
        .iter()
        .filter(|feature| name.feature.as_ref().is_none_or(|n| n == &feature.name))
        .flat_map(|feature| feature.resources.iter())
        .find(|resource| resource.name.eq_ignore_ascii_case(&name.name))
}

fn resource_ts_name(name: &lazuli_ir::QualifiedName, module: &lazuli_ir::Module) -> String {
    find_resource(module, name)
        .map(|r| pascal_case(&r.name))
        .unwrap_or_else(|| pascal_case(&name.name))
}

fn resource_field_ts_name(field: &lazuli_ir::Field, module: &lazuli_ir::Module) -> String {
    if is_resource_ref(&field.type_ref, module) && !field.name.ends_with("_id") {
        format!("{}_id", field.name)
    } else {
        field.name.clone()
    }
}

fn resource_field_ts_type(field: &lazuli_ir::Field, module: &lazuli_ir::Module) -> String {
    if is_resource_ref(&field.type_ref, module) {
        "ID".to_owned()
    } else {
        ts_type_for_type_ref(&field.type_ref, module)
    }
}

fn is_resource_ref(type_ref: &lazuli_ir::TypeRef, module: &lazuli_ir::Module) -> bool {
    match type_ref {
        lazuli_ir::TypeRef::UserDefined(name) => module
            .features
            .iter()
            .flat_map(|feature| feature.resources.iter())
            .any(|resource| resource.name.eq_ignore_ascii_case(&name.name)),
        _ => false,
    }
}

fn ts_type_for_type_ref(type_ref: &lazuli_ir::TypeRef, module: &lazuli_ir::Module) -> String {
    match type_ref {
        lazuli_ir::TypeRef::Builtin(builtin) => match builtin {
            lazuli_ir::BuiltinType::Id => "ID".to_owned(),
            lazuli_ir::BuiltinType::Text
            | lazuli_ir::BuiltinType::SemanticEmail
            | lazuli_ir::BuiltinType::SemanticPhone
            | lazuli_ir::BuiltinType::SemanticUrl
            | lazuli_ir::BuiltinType::SemanticUuid
            | lazuli_ir::BuiltinType::SemanticCurrency
            | lazuli_ir::BuiltinType::CapSecret => "string".to_owned(),
            // B3 — plugin-contributed `@semantic.<Name>` projects to
            // the brand alias name (e.g. `BrazilianCPF`). The SDK
            // emitter (`emit_feature_sdk_ts`) writes the
            // `export type <Name> = string;` line at file head so
            // every consuming interface picks up the alias.
            lazuli_ir::BuiltinType::SemanticPluginType { name, .. } => pascal_case(name),
            lazuli_ir::BuiltinType::Boolean => "boolean".to_owned(),
            lazuli_ir::BuiltinType::Integer
            | lazuli_ir::BuiltinType::Decimal => "number".to_owned(),
            // Per `semantic-types-money-brazilian.md` v0.3 — Money is
            // the rich struct on the TS side too. `Money` interface
            // lives in `@lazuli/runtime`; downstream consumers get the
            // shape via the typed import.
            lazuli_ir::BuiltinType::SemanticMoney { .. } => "Money".to_owned(),
            lazuli_ir::BuiltinType::Date | lazuli_ir::BuiltinType::DateTime => "Time".to_owned(),
            lazuli_ir::BuiltinType::Json
            | lazuli_ir::BuiltinType::SemanticGeoPoint
            | lazuli_ir::BuiltinType::CapFile => "unknown".to_owned(),
        },
        lazuli_ir::TypeRef::Capability(capability) => match capability {
            lazuli_ir::CapabilityRef::Hashed(_)
            | lazuli_ir::CapabilityRef::Encrypted(_)
            | lazuli_ir::CapabilityRef::E2ee(_)
            | lazuli_ir::CapabilityRef::Token(_)
            | lazuli_ir::CapabilityRef::PII(_) => "string".to_owned(),
            lazuli_ir::CapabilityRef::File(_) => "unknown".to_owned(),
        },
        lazuli_ir::TypeRef::Many(inner) => format!("{}[]", ts_type_for_type_ref(inner, module)),
        lazuli_ir::TypeRef::EnumRef(name) => find_enum_decl(module, name)
            .map(|enum_decl| pascal_case(&enum_decl.name))
            .unwrap_or_else(|| "unknown".to_owned()),
        lazuli_ir::TypeRef::UserDefined(name) => {
            if is_resource_ref(type_ref, module) {
                "ID".to_owned()
            } else if let Some(enum_decl) = find_enum_decl(module, name) {
                // Enum referenced via UserDefined path. The parser
                // sometimes tags an enum field as UserDefined when the
                // analyzer hasn't promoted it to EnumRef (review bug #3,
                // 2026-05-15: `tier: CustomerTier = free` and
                // `source: CustomerSource = manual` both flowed as
                // UserDefined-with-no-record-match and lowered to
                // `unknown` — even though `CustomerTier`/`CustomerSource`
                // are declared above the resource block).
                pascal_case(&enum_decl.name)
            } else {
                module
                    .features
                    .iter()
                    .flat_map(|feature| feature.records.iter())
                    .find(|record| record.name.eq_ignore_ascii_case(&name.name))
                    .map(|record| pascal_case(&record.name))
                    .unwrap_or_else(|| "unknown".to_owned())
            }
        }
        lazuli_ir::TypeRef::Unresolved(raw) => {
            if raw.starts_with("@cap.Hashed")
                || raw.starts_with("@cap.Encrypted")
                || raw.starts_with("@cap.Token")
                || raw == "@semantic.Email"
            {
                return "string".to_owned();
            }
            // Bare PascalCase fallback: the analyzer occasionally leaves
            // a `Unresolved("Foo")` even when `Foo` is a declared enum /
            // record / resource somewhere in the module (review bug #3,
            // 2026-05-15: `tier: CustomerTier = manual` flowed as
            // `Unresolved("CustomerTier")` and lowered to `unknown`
            // even though `CustomerTier` is declared three lines above).
            // Recover by walking the module's catalogs here so the TS
            // SDK preserves typing instead of falling to opaque
            // `unknown` whenever the analyzer's resolve pass misses an
            // edge case.
            if !raw.starts_with('@') {
                let synthetic = lazuli_ir::QualifiedName {
                    feature: None,
                    name: raw.clone(),
                };
                if let Some(enum_decl) = find_enum_decl(module, &synthetic) {
                    return pascal_case(&enum_decl.name);
                }
                if let Some(record) = module
                    .features
                    .iter()
                    .flat_map(|feature| feature.records.iter())
                    .find(|record| record.name.eq_ignore_ascii_case(raw))
                {
                    return pascal_case(&record.name);
                }
                if module
                    .features
                    .iter()
                    .flat_map(|feature| feature.resources.iter())
                    .any(|resource| resource.name.eq_ignore_ascii_case(raw))
                {
                    return "ID".to_owned();
                }
            }
            "unknown".to_owned()
        }
    }
}

/// Lower a `PolicyRef` to a TypeScript object literal matching the
/// `PolicySpec` shape exported by `@lazuli/runtime/spec`. Returns `None`
/// when the policy is omitted or explicitly `None` so the caller can
/// elide the `policy: ...` line entirely (review bug #7).
fn format_policy_ts(policy: &lazuli_ir::PolicyRef, feature: &lazuli_ir::Feature) -> Option<String> {
    // Re-prepend `@` when the parser dropped it. PolicyRef::Local
    // carries either the bare category name (`"update"`) or the
    // partial-qualified form (`"policy.update"`); PolicyRef::Atom can
    // arrive with or without the `@` host prefix. Normalize to the
    // DSL-faithful surface (`@policy.update`, `@role.admin`, …) so
    // clients see what they wrote.
    fn ensure_at_prefix(s: &str) -> String {
        if s.starts_with('@') {
            s.to_owned()
        } else {
            format!("@{}", s)
        }
    }
    let (name, atoms): (String, Vec<&str>) = match policy {
        lazuli_ir::PolicyRef::None => return None,
        lazuli_ir::PolicyRef::Local(local) => {
            let qualified = if local.contains('.') {
                ensure_at_prefix(local)
            } else {
                format!("@policy.{}", local)
            };
            let resolved_atoms: Vec<&str> = feature
                .policies
                .categories
                .iter()
                .find(|cat| cat.name == *local)
                .map(|cat| cat.atoms.iter().map(String::as_str).collect())
                .unwrap_or_default();
            (qualified, resolved_atoms)
        }
        lazuli_ir::PolicyRef::Atom(atom) => {
            let qualified = ensure_at_prefix(atom);
            // When the parser stored a `@policy.<name>` reference as
            // an Atom (vs Local), the literal `atom` itself is the
            // POLICY NAME, not an actual `@role.X`/`@scope.X`/`@actor.X`
            // atom. Resolve via the feature's policies dictionary to
            // recover the real atoms; fall back to treating it as a
            // standalone atom only when no category matches.
            let body = atom.trim_start_matches('@');
            let local_name = body.strip_prefix("policy.").unwrap_or("");
            let resolved_atoms: Vec<&str> = if !local_name.is_empty() {
                feature
                    .policies
                    .categories
                    .iter()
                    .find(|cat| cat.name == local_name)
                    .map(|cat| cat.atoms.iter().map(String::as_str).collect())
                    .unwrap_or_default()
            } else {
                vec![atom.as_str()]
            };
            (qualified, resolved_atoms)
        }
        lazuli_ir::PolicyRef::External { feature, name } => {
            (
                format!("{}.{}", feature, ensure_at_prefix(name)),
                Vec::new(),
            )
        }
        lazuli_ir::PolicyRef::Unresolved(raw) => (raw.clone(), Vec::new()),
    };
    let atoms_lit = if atoms.is_empty() {
        "[]".to_owned()
    } else {
        let entries: Vec<String> = atoms
            .iter()
            .filter_map(|atom| parse_policy_atom_ts(atom))
            .collect();
        format!("[{}]", entries.join(", "))
    };
    Some(format!(
        "{{ name: \"{}\", atoms: {} }}",
        escape_js_string(&name),
        atoms_lit
    ))
}

/// Parse a raw policy atom string like `@role.admin` (or `role.admin`
/// when the parser dropped the host prefix) into the TS
/// `{ namespace: "role", name: "admin" }` literal. Returns `None` when
/// the atom does not parse — caller drops it from the literal rather
/// than emitting an invalid spec.
fn parse_policy_atom_ts(raw: &str) -> Option<String> {
    let body = raw.trim_start_matches('@');
    let (namespace, name) = body.split_once('.')?;
    if namespace.is_empty() || name.is_empty() {
        return None;
    }
    Some(format!(
        "{{ namespace: \"{}\", name: \"{}\" }}",
        escape_js_string(namespace),
        escape_js_string(name)
    ))
}

/// Lower an `AuditSpec` to a TypeScript literal matching the
/// `AuditSpec` union exported by `@lazuli/runtime/spec`:
///   - `Some({subjects: [], ..})`        → `"default"` sentinel
///   - `Some({subjects: ["actor", ..]})` → string array literal
///   - `None`                             → caller elides the field
fn format_audit_ts(audit: Option<&lazuli_ir::AuditSpec>) -> Option<String> {
    let audit = audit?;
    if audit.subjects.is_empty() {
        return Some("\"default\"".to_owned());
    }
    let entries: Vec<String> = audit
        .subjects
        .iter()
        .map(|s| format!("\"{}\"", escape_js_string(s)))
        .collect();
    Some(format!("[{}]", entries.join(", ")))
}

/// Escape a string for embedding in a TS double-quoted literal. Conservative:
/// covers `"`, `\`, and control chars that would terminate or break the
/// literal. Newlines collapse to `\n`; nothing else is interpreted.
fn escape_js_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if (ch as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", ch as u32));
            }
            _ => out.push(ch),
        }
    }
    out
}

fn find_enum_decl<'a>(
    module: &'a lazuli_ir::Module,
    name: &lazuli_ir::QualifiedName,
) -> Option<&'a lazuli_ir::EnumDecl> {
    module
        .features
        .iter()
        .filter(|feature| {
            name.feature
                .as_ref()
                .is_none_or(|owner| owner == &feature.name)
        })
        .flat_map(|feature| feature.enums.iter())
        .find(|enum_decl| enum_decl.name.eq_ignore_ascii_case(&name.name))
}

fn enum_value_constant_name(type_ref: &str) -> String {
    let local = type_ref
        .rsplit(['.', ':'])
        .find(|part| !part.is_empty())
        .unwrap_or(type_ref);
    let mut out = String::with_capacity(local.len() + "_VALUES".len());
    let mut prev_lower_or_digit = false;

    for ch in local.chars() {
        if ch.is_ascii_alphanumeric() {
            if ch.is_ascii_uppercase() && prev_lower_or_digit && !out.ends_with('_') {
                out.push('_');
            }
            out.push(ch.to_ascii_uppercase());
            prev_lower_or_digit = ch.is_ascii_lowercase() || ch.is_ascii_digit();
        } else {
            if !out.is_empty() && !out.ends_with('_') {
                out.push('_');
            }
            prev_lower_or_digit = false;
        }
    }

    while out.ends_with('_') {
        out.pop();
    }

    out.push_str("_VALUES");
    out
}

fn enum_option_constant_name(type_ref: &str) -> String {
    let mut out = enum_value_constant_name(type_ref);
    if out.ends_with("_VALUES") {
        let prefix_len = out.len() - "_VALUES".len();
        out.truncate(prefix_len);
        out.push_str("_OPTIONS");
    }
    out
}

fn enum_variant_ts_literal(variant: &lazuli_ir::EnumVariant) -> String {
    match &variant.storage_value {
        Some(lazuli_ir::StorageValue::String(value)) => format_ts_string(value),
        Some(lazuli_ir::StorageValue::Integer(value)) => value.to_string(),
        None => format_ts_string(&variant.name.to_ascii_lowercase()),
    }
}

fn enum_has_option_metadata(enum_decl: &lazuli_ir::EnumDecl) -> bool {
    enum_decl.variants.iter().any(|variant| {
        variant.label_key.is_some() || variant.hint_key.is_some() || variant.icon_key.is_some()
    })
}

fn write_enum_options_alias(
    s: &mut String,
    enum_decl: &lazuli_ir::EnumDecl,
    type_name: &str,
    options_name: &str,
) {
    let label_required = enum_decl
        .variants
        .iter()
        .all(|variant| variant.label_key.is_some());
    let label_prop = if label_required {
        "labelKey: string;"
    } else {
        "labelKey?: string;"
    };
    writeln!(s, "export const {options_name}: ReadonlyArray<{{").ok();
    writeln!(s, "  value: {type_name};").ok();
    writeln!(s, "  {label_prop}").ok();
    writeln!(s, "  hintKey?: string;").ok();
    writeln!(s, "  iconKey?: string;").ok();
    writeln!(s, "}}> = [").ok();
    for variant in &enum_decl.variants {
        writeln!(s, "  {},", enum_variant_option_ts_literal(variant)).ok();
    }
    writeln!(s, "];").ok();
}

fn enum_variant_option_ts_literal(variant: &lazuli_ir::EnumVariant) -> String {
    let mut props = vec![format!("value: {}", enum_variant_ts_literal(variant))];
    if let Some(label_key) = &variant.label_key {
        props.push(format!("labelKey: {}", format_ts_string(label_key)));
    }
    if let Some(hint_key) = &variant.hint_key {
        props.push(format!("hintKey: {}", format_ts_string(hint_key)));
    }
    if let Some(icon_key) = &variant.icon_key {
        props.push(format!("iconKey: {}", format_ts_string(icon_key)));
    }
    format!("{{ {} }}", props.join(", "))
}

fn format_ts_string(value: &str) -> String {
    let escaped = value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r");
    format!("\"{escaped}\"")
}

fn zod_expr_for_slot(
    type_ref: &lazuli_ir::TypeRef,
    constraints: &lazuli_ir::FieldConstraints,
    optional: bool,
) -> String {
    let base = zod_base_for_type_ref(type_ref);
    let is_text_base = zod_is_text_base(type_ref);
    let mut out = format!(
        "{}{}",
        base,
        lazuli_codegen_ts::zod_constraint_chain(constraints, is_text_base)
    );
    if optional {
        out.push_str(".optional()");
    }
    out
}

fn zod_base_for_type_ref(type_ref: &lazuli_ir::TypeRef) -> String {
    match type_ref {
        lazuli_ir::TypeRef::Builtin(builtin) => match builtin {
            lazuli_ir::BuiltinType::Boolean => "z.boolean()".to_owned(),
            lazuli_ir::BuiltinType::Integer
            | lazuli_ir::BuiltinType::Decimal
            | lazuli_ir::BuiltinType::SemanticMoney { .. } => "z.number()".to_owned(),
            lazuli_ir::BuiltinType::Json
            | lazuli_ir::BuiltinType::SemanticGeoPoint
            | lazuli_ir::BuiltinType::CapFile => "z.unknown()".to_owned(),
            _ => "z.string()".to_owned(),
        },
        lazuli_ir::TypeRef::Capability(capability) => match capability {
            lazuli_ir::CapabilityRef::File(_) => "z.unknown()".to_owned(),
            _ => "z.string()".to_owned(),
        },
        // Wave 0 (ir-returns-list-2026-05-22 §2.3): closes the
        // `SCHEMA-RICH-001` list axis early. `list <X>` lifts to
        // `TypeRef::Many(X)` in the analyzer; emit `z.array(<inner>)`
        // so form/wire schemas validate list-of-record at runtime
        // instead of accepting any `unknown[]` shape.
        lazuli_ir::TypeRef::Many(inner) => format!("z.array({})", zod_base_for_type_ref(inner)),
        lazuli_ir::TypeRef::EnumRef(_) => "z.string()".to_owned(),
        lazuli_ir::TypeRef::UserDefined(_) | lazuli_ir::TypeRef::Unresolved(_) => {
            "z.unknown()".to_owned()
        }
    }
}

fn zod_is_text_base(type_ref: &lazuli_ir::TypeRef) -> bool {
    matches!(
        type_ref,
        lazuli_ir::TypeRef::Builtin(
            lazuli_ir::BuiltinType::Id
                | lazuli_ir::BuiltinType::Text
                | lazuli_ir::BuiltinType::Date
                | lazuli_ir::BuiltinType::DateTime
                | lazuli_ir::BuiltinType::SemanticEmail
                | lazuli_ir::BuiltinType::SemanticPhone
                | lazuli_ir::BuiltinType::SemanticUrl
                | lazuli_ir::BuiltinType::SemanticUuid
                | lazuli_ir::BuiltinType::SemanticCurrency
                | lazuli_ir::BuiltinType::CapSecret
        ) | lazuli_ir::TypeRef::EnumRef(_)
            | lazuli_ir::TypeRef::Capability(
                lazuli_ir::CapabilityRef::Hashed(_)
                    | lazuli_ir::CapabilityRef::Encrypted(_)
                    | lazuli_ir::CapabilityRef::E2ee(_)
                    | lazuli_ir::CapabilityRef::Token(_)
            )
    )
}

fn command_ident(feature: &str, command_name: &str) -> String {
    let resource_pascal = pascal_case(feature);
    let feature_lc = feature.to_ascii_lowercase();
    let mut parts = command_name.split('_');
    let verb = parts.next().unwrap_or("");
    let mut out = verb.to_ascii_lowercase();
    out.push_str(&resource_pascal);
    // Closes WAR-CODEGEN-TS-02: when the command name already contains
    // the feature name as a token (e.g. `save_host_basic_details` in
    // feature `host`), skip the duplicate token so we get
    // `saveHostBasicDetails` instead of `saveHostHostBasicDetails`.
    let mut skipped_dup = false;
    for word in parts {
        if !skipped_dup && word.eq_ignore_ascii_case(&feature_lc) {
            skipped_dup = true;
            continue;
        }
        out.push_str(&pascal_case(word));
    }
    out
}

fn query_ident(feature: &str, kind: lazuli_ir::QueryKind, query_name: &str) -> String {
    let resource_pascal = pascal_case(feature);
    match kind {
        lazuli_ir::QueryKind::List | lazuli_ir::QueryKind::Sql | lazuli_ir::QueryKind::View => {
            if query_name.eq_ignore_ascii_case("list") {
                format!("list{}s", resource_pascal)
            } else if query_name.eq_ignore_ascii_case("fulltext") {
                format!("search{}sFulltext", resource_pascal)
            } else if let Some(rest) = strip_query_verb_prefix(query_name, "list_") {
                // `conventions [crud]` synth produces `list_<resource>s`;
                // without the dedup the legacy shape would emit
                // `listListTravelersTravelers` from `list_travelers`.
                format!("list{}", pascal_case(rest))
            } else {
                format!("list{}{}s", pascal_case(query_name), resource_pascal)
            }
        }
        lazuli_ir::QueryKind::Lookup => {
            if let Some(rest) = strip_query_verb_prefix(query_name, "lookup_") {
                // `conventions [crud, me]` synth produces `lookup_<r>` and
                // `lookup_my_<r>`; without the dedup the legacy
                // `lookup<R>By<X>` shape would emit
                // `lookupHostByLookupMyHost` from `lookup_my_host`.
                format!("lookup{}", pascal_case(rest))
            } else {
                let stripped = query_name.strip_prefix("by_").unwrap_or(query_name);
                format!("lookup{}By{}", resource_pascal, pascal_case(stripped))
            }
        }
    }
}

/// Strip a verb prefix (`lookup_` / `list_`) from a query name, returning
/// `Some(rest)` only when the remainder pascal-cases to a non-empty
/// segment. Returns `None` for bare prefix (`lookup_`), missing prefix,
/// or empty/whitespace remainder — callers fall back to the legacy hook
/// shape. Mirrors `lazuli_codegen_ts::lzx::strip_verb_prefix`; duplicated
/// here to keep the CLI's identifier-casing rules self-contained.
fn strip_query_verb_prefix<'a>(name: &'a str, prefix: &str) -> Option<&'a str> {
    let rest = name.strip_prefix(prefix)?;
    if rest.is_empty() {
        return None;
    }
    if pascal_case(rest).is_empty() {
        return None;
    }
    Some(rest)
}

fn command_input_iface(command_name: &str, feature_pascal: &str) -> String {
    let feature_lc = feature_pascal.to_ascii_lowercase();
    let mut parts = command_name.split('_');
    let verb = parts.next().unwrap_or("");
    let mut out = pascal_case(verb);
    out.push_str(feature_pascal);
    // Mirror command_ident's WAR-CODEGEN-TS-02 dedup so the *Input
    // interface name matches the command identifier shape.
    let mut skipped_dup = false;
    for word in parts {
        if !skipped_dup && word.eq_ignore_ascii_case(&feature_lc) {
            skipped_dup = true;
            continue;
        }
        out.push_str(&pascal_case(word));
    }
    out.push_str("Input");
    out
}

fn command_schema_ident(command_name: &str, feature_pascal: &str) -> String {
    let iface = command_input_iface(command_name, feature_pascal);
    lower_camel(&iface) + "Schema"
}

fn format_string_array(items: &[String]) -> String {
    if items.is_empty() {
        return "[]".to_owned();
    }
    let parts: Vec<String> = items.iter().map(|s| format!("\"{s}\"")).collect();
    format!("[{}]", parts.join(", "))
}

fn lower_camel(s: &str) -> String {
    let pascal = if s.chars().any(|ch| !ch.is_ascii_alphanumeric()) {
        pascal_case(s)
    } else {
        s.to_owned()
    };
    let mut chars = pascal.chars();
    let Some(first) = chars.next() else {
        return pascal;
    };
    let mut out = String::with_capacity(pascal.len());
    for c in first.to_lowercase() {
        out.push(c);
    }
    out.push_str(chars.as_str());
    out
}

fn reject_generate_feature_options(
    output: Option<&Path>,
    api_version: Option<&str>,
    module: Option<&str>,
    lazuli_go_version: Option<&str>,
    check: bool,
    with_source: bool,
) -> Result<()> {
    if output.is_some()
        || api_version.is_some()
        || module.is_some()
        || lazuli_go_version.is_some()
        || check
        || with_source
    {
        bail!(
            "`lazuli generate feature <name>` does not accept codegen flags like --out, --api-version, --module, --check, or --with-source"
        );
    }
    Ok(())
}

fn design_command(sub: DesignCommand) -> Result<()> {
    let project_root = std::env::current_dir().context("failed to determine current directory")?;
    let design_path = cmd_design::default_design_path(&project_root);
    match sub {
        DesignCommand::Import {
            from,
            format,
            overwrite,
        } => cmd_design::import(&from, format.into(), &design_path, overwrite),
        DesignCommand::Export { target, out } => {
            let design = cmd_design::read_design(&design_path)?;
            cmd_design::export(&out, target.into(), &design)
        }
        DesignCommand::Diff { against } => {
            let design = cmd_design::read_design(&design_path)?;
            let report = cmd_design::diff(&against, &design)?;
            print!("{}", report.render());
            if report.is_empty() {
                Ok(())
            } else {
                bail!("design diff found changes")
            }
        }
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

fn profile_command(profile_path: &Path, top: usize, by: &str, format: &str) -> Result<()> {
    let axis = match by {
        "cpu" => profile::ProfileAxis::Cpu,
        "alloc" => profile::ProfileAxis::Alloc,
        "block" => profile::ProfileAxis::Block,
        other => bail!("unknown profile axis `{other}`; expected cpu, alloc, or block"),
    };
    let report = profile::run_profile(profile_path, top, axis)
        .map_err(|err| anyhow::anyhow!("failed to read profile: {err}"))?;
    match format {
        "text" => {
            print!("{}", profile::format_report(&report));
            Ok(())
        }
        "json" => {
            let payload = serde_json::json!({
                "top_ops": report.top_ops,
                "top_patterns": report.top_patterns,
            });
            println!("{}", serde_json::to_string_pretty(&payload)?);
            Ok(())
        }
        other => bail!("unknown profile format `{other}`; expected text or json"),
    }
}

/// Lazuli Go bucket cycle — emit Lazuli Go user-code from the typed
/// IR. Walks `lazuli_codegen_go::generate_v1`, then either writes the
/// resulting files into `--out` or, in `--check` mode, prints what
/// would be emitted without touching the filesystem.
///
/// Per the proposal §1.1, `--out` is required for `go` because the
/// emitter produces multiple files (one per feature plus a root
/// `go.mod`). `--check` short-circuits the write step and exits 1
/// when the emitter surfaces unresolved references. The §6.2.1 error
/// catalog is wired in cell I4; this cell ships the coarse pass/fail
/// signal.
pub(crate) fn generate_go(
    input: &Path,
    output: Option<&Path>,
    module: Option<&str>,
    lazuli_go_version: Option<&str>,
    check: bool,
    with_source: bool,
    allow_drops: bool,
) -> Result<()> {
    // Cell A11 — `--allow-drops` gates the ALTER migration emitter's
    // treatment of `SchemaDiff.drops`. Without the flag, drops are
    // emitted as commented-out lines under a WARNING header so authors
    // explicitly opt in to destructive ALTERs. With the flag, the drops
    // become live `DROP COLUMN IF EXISTS` statements.
    //
    // The diff-vs-baseline orchestration that produces a non-empty
    // `SchemaDiff` lives in cell A10 (`schema_diff.rs`). Once A10 lands,
    // wire it here: read `migrations/` from `out_dir`, parse the latest
    // CREATE TABLE per resource, compare to the IR, hand the diff +
    // `AlterEmitOptions { allow_drops }` to
    // `lazuli_codegen_go::emitter::migration_ddl::emit_alter_migration_file`,
    // and append the returned (up, down) pair to `files`. Until A10 is
    // in tree, `--allow-drops` is accepted on the CLI but has no
    // observable effect because no diff is computed.
    let alter_options =
        lazuli_codegen_go::emitter::migration_ddl::AlterEmitOptions { allow_drops };
    // `_ = alter_options;` suppresses dead_code while A10 is in flight;
    // delete this discard when A10's caller wires `emit_alter_migration_file`.
    let _ = alter_options;
    let project_root = project_root_for_input(input);
    let manifest = lazurite_manifest::load(&project_root).with_context(|| {
        format!(
            "failed to read {}",
            project_root.join("Lazurite.toml").display()
        )
    })?;
    let (module_ir, source_context) = if with_source {
        let (module_ir, source_map, feature_file_ids) = build_module_with_source_from_path(input)?;
        (module_ir, Some((source_map, feature_file_ids)))
    } else {
        (build_module_from_path(input)?, None)
    };
    let manifest_out = manifest
        .as_ref()
        .and_then(|m| m.generate.go.as_ref())
        .map(|go| project_root.join(&go.out));
    let out_dir = output.or(manifest_out.as_deref());
    let codegen_manifest = manifest
        .as_ref()
        .map(|manifest| codegen_lazurite_manifest(manifest, &project_root, out_dir));

    let module_name = match module {
        Some(name) => name.to_owned(),
        None => default_go_module_name(&module_ir),
    };
    let go_version = lazuli_go_version
        .map(|s| s.to_owned())
        .unwrap_or_else(|| lazuli_codegen_go::LAZULI_GO_VERSION.to_owned());

    // Closed §6.2.1 error catalog (CODEGEN-GO-PLUGIN-001,
    // CODEGEN-GO-TYPE-007, …). Run BEFORE codegen so the emitter never
    // produces broken Go for a module that already fails policy. Errors
    // abort the run; warnings stream to stderr but still allow emission.
    let issues = lazuli_codegen_go::emitter::check::run_checks(&module_ir);
    let errors: Vec<_> = issues
        .iter()
        .filter(|i| matches!(i.severity, lazuli_codegen_go::emitter::check::Severity::Error))
        .collect();
    let warnings: Vec<_> = issues
        .iter()
        .filter(|i| !matches!(i.severity, lazuli_codegen_go::emitter::check::Severity::Error))
        .collect();
    for w in &warnings {
        eprintln!(
            "[{}] warn: {}{}{}",
            w.code,
            w.message,
            w.feature
                .as_deref()
                .map(|f| format!(" (feature `{f}`)"))
                .unwrap_or_default(),
            w.site
                .as_deref()
                .map(|s| format!(" at {s}"))
                .unwrap_or_default(),
        );
    }
    if !errors.is_empty() {
        for e in &errors {
            eprintln!(
                "[{}] error: {}{}{}",
                e.code,
                e.message,
                e.feature
                    .as_deref()
                    .map(|f| format!(" (feature `{f}`)"))
                    .unwrap_or_default(),
                e.site
                    .as_deref()
                    .map(|s| format!(" at {s}"))
                    .unwrap_or_default(),
            );
        }
        anyhow::bail!(
            "lazuli generate go: {} blocking issue(s) in the closed codegen error catalog",
            errors.len()
        );
    }

    // PG.C — compute plan-and-gate facts from the .lzi sources so
    // codegen emits `dist/go/plan/catalog.gen.go` when the package
    // authors plans.
    let plan_gate = collect_plan_gate_facts_for_generate(input);

    let options = lazuli_codegen_go::GoEmitOptions {
        module_name: Some(module_name),
        lazuli_go_version: go_version,
        check,
        plan_gate,
    };
    let files = if let Some((source_map, feature_file_ids)) = source_context.as_ref() {
        lazuli_codegen_go::generate_v1_with_manifest_and_source(
            &module_ir,
            &options,
            codegen_manifest.as_ref(),
            lazuli_codegen_go::GoSourceContext {
                source_map,
                feature_file_ids,
            },
        )
    } else {
        lazuli_codegen_go::generate_v1_with_manifest(
            &module_ir,
            &options,
            codegen_manifest.as_ref(),
        )
    };

    if check {
        // Coarse pass/fail signal (catalog above already aborted on
        // Error severity; the closed §6.2.1 catalog continues to grow
        // in cell I4). Enumerates what would be written.
        println!("lazuli generate go --check");
        println!("would emit {} file(s):", files.len());
        for file in &files {
            println!("  {}", file.path);
        }
        return Ok(());
    }

    let out_dir = out_dir.ok_or_else(|| {
        anyhow::anyhow!(
            "`lazuli generate go` requires --out <dir>; the emitter writes multiple files"
        )
    })?;

    fs::create_dir_all(out_dir)
        .with_context(|| format!("creating output directory {}", out_dir.display()))?;

    let mut handler_stubs_written = 0usize;
    let mut handler_stubs_skipped = 0usize;
    for file in &files {
        if file.path == "go.work" {
            write_go_work_preserving_entries(&project_root, &file.contents)?;
        } else if file.path.starts_with("app/features/") {
            // Handler stubs are Tier 1 portable code under
            // `app/features/<feature>/<name>.go` — written to the
            // project root, NOT under the codegen `out_dir`. They're
            // user territory once authored, so we skip files that
            // already exist (idempotent: scaffold-once, never
            // overwrite). See `docs/project-structure.md`.
            let target = project_root.join(&file.path);
            if target.exists() {
                handler_stubs_skipped += 1;
                continue;
            }
            // Legacy fallbacks — pre-pivot scaffolds had handlers at:
            //   1. `dist/go/<f>/<name>.go` (first failed pivot)
            //   2. `app/features/<f>/<name>.go` (flat layout, no
            //      `handlers/` sub-folder)
            // Don't overwrite either — consumer migration relocates
            // them deliberately. Both translations skip the
            // `handlers/` segment that the canonical path carries.
            let canonical = &file.path;
            let mut legacy_skipped = false;
            if let Some(after_features) =
                canonical.strip_prefix("app/features/")
            {
                if let Some((feature, after_feature)) =
                    after_features.split_once('/')
                {
                    if let Some(name) = after_feature.strip_prefix("handlers/") {
                        let legacy_flat_app =
                            format!("app/features/{feature}/{name}");
                        let legacy_dist = format!("dist/go/{feature}/{name}");
                        for legacy in [legacy_flat_app, legacy_dist] {
                            if project_root.join(&legacy).exists() {
                                handler_stubs_skipped += 1;
                                legacy_skipped = true;
                                break;
                            }
                        }
                    }
                }
            }
            if legacy_skipped {
                continue;
            }
            write_generated_file(&project_root, &file.path, &file.contents)?;
            handler_stubs_written += 1;
        } else {
            write_generated_file(out_dir, &file.path, &file.contents)?;
        }
    }

    let codegen_count = files.len() - handler_stubs_written - handler_stubs_skipped;
    println!("wrote {} file(s) to {}", codegen_count, out_dir.display());
    if handler_stubs_written > 0 {
        println!(
            "wrote {} handler stub(s) to {}/app/features/",
            handler_stubs_written,
            project_root.display(),
        );
    }
    if handler_stubs_skipped > 0 {
        println!(
            "skipped {} existing handler stub(s) (user-authored)",
            handler_stubs_skipped,
        );
    }
    Ok(())
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
    let generate_go = manifest.generate.go.as_ref().map(|go| {
        let detected =
            out_dir.and_then(|out_dir| detect_runtime_dev_replace(project_root, out_dir));
        lazuli_codegen_go::LazuriteGenerateGo {
            emit_main: go.emit_main,
            submodule: go.submodule,
            dev_replace: go
                .dev_replace
                .clone()
                .or_else(|| detected.as_ref().map(|paths| paths.go_mod.clone())),
            dev_work_replace: go
                .dev_replace
                .clone()
                .or_else(|| detected.map(|paths| paths.go_work)),
        }
    });
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

fn absolutize_project_root(project_root: &Path) -> std::path::PathBuf {
    if project_root.is_absolute() {
        project_root.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| Path::new(".").to_path_buf())
            .join(project_root)
    }
}

fn absolutize_for_codegen(project_root: &Path, path: &Path) -> std::path::PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else if path.starts_with(project_root) {
        std::env::current_dir()
            .unwrap_or_else(|_| project_root.to_path_buf())
            .join(path)
    } else {
        project_root.join(path)
    }
}

fn relative_path(from_dir: &Path, to_dir: &Path) -> String {
    let from_components = from_dir
        .components()
        .map(|component| component.as_os_str().to_owned())
        .collect::<Vec<_>>();
    let to_components = to_dir
        .components()
        .map(|component| component.as_os_str().to_owned())
        .collect::<Vec<_>>();

    let mut common = 0;
    while common < from_components.len()
        && common < to_components.len()
        && from_components[common] == to_components[common]
    {
        common += 1;
    }

    let mut parts = Vec::new();
    for _ in common..from_components.len() {
        parts.push("..".to_owned());
    }
    for component in &to_components[common..] {
        parts.push(component.to_string_lossy().into_owned());
    }

    if parts.is_empty() {
        ".".to_owned()
    } else {
        parts.join("/")
    }
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

/// Lower-snake / lower-kebab caser used by `default_module_name`. Kept
/// local to avoid pulling the codegen-go internal helpers into the CLI
/// surface; mirrors the small kebab caser in
/// `lazuli_codegen_go::to_kebab_case`.
fn to_kebab_case(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut prev_lower = false;
    for ch in value.chars() {
        if ch == '_' || ch == ' ' {
            out.push('-');
            prev_lower = false;
            continue;
        }
        if ch.is_ascii_uppercase() {
            if prev_lower && !out.is_empty() {
                out.push('-');
            }
            for low in ch.to_lowercase() {
                out.push(low);
            }
            prev_lower = false;
            continue;
        }
        out.push(ch);
        prev_lower = ch.is_ascii_lowercase() || ch.is_ascii_digit();
    }
    out
}

fn generate_openapi(input: &Path, output: Option<&Path>, api_version: Option<&str>) -> Result<()> {
    let module = build_module_from_path(input)?;
    let opts = lazuli_openapi::EmitOptions {
        api_version: api_version.map(|s| s.to_owned()),
        strict_typed_only: false,
    };
    let yaml = lazuli_openapi::emit(&module, opts);
    match output {
        Some(path) => {
            if let Some(parent) = path.parent() {
                if !parent.as_os_str().is_empty() {
                    fs::create_dir_all(parent).with_context(|| {
                        format!("creating output directory {}", parent.display())
                    })?;
                }
            }
            fs::write(path, &yaml)
                .with_context(|| format!("writing OpenAPI spec to {}", path.display()))?;
            println!("wrote {}", path.display());
        }
        None => print!("{}", yaml),
    }
    Ok(())
}

/// i18n bucket cycle — `lazuli translate extract` walks the package,
/// harvests every translatable surface, and writes per-locale catalog
/// stub files. Sources walked:
///
/// 1. `translation` blocks per feature — declared key + variants.
/// 2. `rule message @translation.<key>` references — fail in `--check`
///    when unresolved (otherwise warned).
/// 3. `notification template "<path>"` with `<locale>` placeholder —
///    one file per supported locale.
///
/// Idempotent: never overwrites authored translation text. Missing
/// variants are emitted as `{ "<key>": "" }` with a warning. When
/// `--check` is set, the CLI exits with code 1 if any key is missing
/// a variant for any supported locale, or if any `@translation.<key>`
/// reference is unresolved.
fn translate_extract_command(
    input: &Path,
    out: &Path,
    locale_filter: Option<&str>,
    check: bool,
) -> Result<()> {
    let module = build_module_from_path(input)?;

    // Locale catalog from the app manifest. Defaults to `[default]` when
    // a project authors only the bare scalar.
    let supported: Vec<String> = match module.app.as_ref() {
        Some(app) => match app.locale.as_ref() {
            Some(locale) => locale.supported.clone(),
            None => app
                .default_locale
                .as_ref()
                .map(|d| vec![d.clone()])
                .unwrap_or_default(),
        },
        None => Vec::new(),
    };
    let default_locale = module
        .app
        .as_ref()
        .and_then(|app| {
            app.locale
                .as_ref()
                .map(|l| l.default.clone())
                .or_else(|| app.default_locale.clone())
        })
        .unwrap_or_default();
    if supported.is_empty() {
        anyhow::bail!(
            "no `app.locale.supported` (or `default_locale`) declared; cannot extract translations"
        );
    }

    let mut missing: Vec<String> = Vec::new();
    let mut unresolved_refs: Vec<String> = Vec::new();

    // Per-feature catalog stubs.
    for feature in &module.features {
        let Some(translation) = &feature.translation else {
            continue;
        };
        let declared: std::collections::BTreeSet<&str> =
            translation.keys.iter().map(|k| k.name.as_str()).collect();

        // Resolve `@translation.<key>` references walked in the source
        // file. The legacy `Rule` IR slot does not yet carry
        // `message_ref`; doctor uses a text-pattern walk for this and
        // we mirror that here.
        let feature_paths: Vec<PathBuf> = match feature.span_ref.as_ref() {
            Some(_) => collect_feature_lzi_paths(input, &feature.name)?,
            None => Vec::new(),
        };
        for path in &feature_paths {
            let text =
                fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
            for line in text.lines() {
                let trimmed = line.trim_start();
                if let Some(rest) = trimmed.strip_prefix("message @translation.") {
                    let key = rest.split_whitespace().next().unwrap_or("");
                    if !key.is_empty() && !declared.contains(key) {
                        unresolved_refs.push(format!("{}.{}", feature.name, key));
                    }
                }
            }
        }
        for locale in &supported {
            if let Some(filter) = locale_filter {
                if filter != locale.as_str() {
                    continue;
                }
            }
            let catalog_path = translation.catalog.replace("<locale>", locale);
            let stub_path = out
                .join(format!("{}.{}.json", feature.name, locale))
                .to_owned();
            // Write a minimal `{ "<key>": "<text or empty>" }` stub.
            let mut entries: Vec<(String, String)> = Vec::new();
            for key in &translation.keys {
                let variant = key
                    .variants
                    .iter()
                    .find(|v| v.locale.as_str() == locale.as_str());
                let text = match variant {
                    Some(v) => v.text.clone(),
                    None => {
                        let key_id = format!("{}.{}.{}", feature.name, key.name, locale);
                        missing.push(key_id);
                        String::new()
                    }
                };
                entries.push((key.name.clone(), text));
            }
            let mut json = String::new();
            json.push_str("{\n");
            for (idx, (k, v)) in entries.iter().enumerate() {
                json.push_str(&format!(
                    "  \"{}\": \"{}\"{}\n",
                    json_escape(k),
                    json_escape(v),
                    if idx + 1 < entries.len() { "," } else { "" }
                ));
            }
            json.push_str("}\n");
            if let Some(parent) = stub_path.parent() {
                if !parent.as_os_str().is_empty() {
                    fs::create_dir_all(parent).with_context(|| {
                        format!("creating output directory {}", parent.display())
                    })?;
                }
            }
            fs::write(&stub_path, &json)
                .with_context(|| format!("writing {}", stub_path.display()))?;
            println!(
                "extracted {} keys to {} (catalog template: {})",
                entries.len(),
                stub_path.display(),
                catalog_path
            );
        }
    }

    if check {
        let mut failures: Vec<String> = Vec::new();
        for entry in &missing {
            // The default locale must always be authored; warn for
            // non-default supported tags but only fail CI for default.
            if entry.ends_with(&format!(".{}", default_locale)) {
                failures.push(format!("missing variant for default locale: {entry}"));
            } else {
                eprintln!("warning: missing variant for supported locale: {entry}");
            }
        }
        for entry in &unresolved_refs {
            failures.push(format!("unresolved `@translation.{entry}` reference"));
        }
        if !failures.is_empty() {
            for failure in &failures {
                eprintln!("error: {failure}");
            }
            anyhow::bail!(
                "translate extract --check failed ({} issue(s))",
                failures.len()
            );
        }
    } else if !missing.is_empty() {
        for entry in &missing {
            eprintln!("warning: missing variant: {entry}");
        }
    }

    Ok(())
}

/// `lazuli translate extract` helper — collect the `.lzi` paths that
/// host a given feature. We mirror what `build_module_from_path` does
/// — walk the package's `.lzi` files and return any that contain a
/// `feature <name>` header.
fn collect_feature_lzi_paths(root: &Path, feature_name: &str) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    let candidates: Vec<PathBuf> = if root.is_dir() {
        let mut acc: Vec<PathBuf> = Vec::new();
        for entry in
            fs::read_dir(root).with_context(|| format!("reading directory {}", root.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("lzi") {
                acc.push(path);
            }
        }
        acc
    } else {
        vec![root.to_path_buf()]
    };
    let header = format!("feature {feature_name}");
    for path in candidates {
        let text =
            fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        for line in text.lines() {
            if line.trim_start() == header || line.trim_start().starts_with(&format!("{header} ")) {
                out.push(path.clone());
                break;
            }
        }
    }
    Ok(out)
}

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out
}

/// OpenAPI bucket cycle — emit a changelog markdown from two inspect
/// JSON payloads.
fn changelog_command(from: &Path, to: &Path, output: Option<&Path>) -> Result<()> {
    let old_text =
        fs::read_to_string(from).with_context(|| format!("reading {}", from.display()))?;
    let new_text = fs::read_to_string(to).with_context(|| format!("reading {}", to.display()))?;
    let old_module: lazuli_ir::Module = serde_json::from_str(&old_text)
        .with_context(|| format!("parsing {} as IR JSON", from.display()))?;
    let new_module: lazuli_ir::Module = serde_json::from_str(&new_text)
        .with_context(|| format!("parsing {} as IR JSON", to.display()))?;
    let report = lazuli_changelog::diff(&old_module, &new_module);
    let md = lazuli_changelog::render_markdown(&report);
    match output {
        Some(path) => {
            if let Some(parent) = path.parent() {
                if !parent.as_os_str().is_empty() {
                    fs::create_dir_all(parent).with_context(|| {
                        format!("creating output directory {}", parent.display())
                    })?;
                }
            }
            fs::write(path, &md)
                .with_context(|| format!("writing changelog to {}", path.display()))?;
            println!("wrote {}", path.display());
        }
        None => print!("{}", md),
    }
    Ok(())
}

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
fn build_module_from_path(input: &Path) -> Result<lazuli_ir::Module> {
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
fn plan_command(input: &Path, check: Option<&str>) -> Result<()> {
    let Some(check_name) = check else {
        bail!("`lazuli plan` currently requires `--check <snapshot_name>`");
    };

    // Locate `app.lzi` — accept either a direct path or a directory.
    let app_path = if input.is_dir() {
        lazurite_manifest::resolve_in_app_dir(input, "app.lzi")
    } else {
        input.to_path_buf()
    };
    if !app_path.exists() {
        bail!("app manifest not found at {}", app_path.display());
    }

    let source = fs::read_to_string(&app_path)
        .with_context(|| format!("failed to read {}", app_path.display()))?;

    let manifest = app_manifest::parse_app_manifest(&source)
        .ok_or_else(|| anyhow::anyhow!("{} does not declare an `app` block", app_path.display()))?;

    let Some(deploy) = manifest.deploy.as_ref() else {
        bail!(
            "app `{}` declares no `deploy` block — nothing to plan",
            manifest.name
        );
    };
    let Some(checkpoint) = deploy.checkpoint.as_ref() else {
        bail!(
            "app `{}` declares no `deploy.checkpoint` — add `checkpoint <name> \"<path>\"` first",
            manifest.name
        );
    };
    if checkpoint.name != check_name {
        bail!(
            "checkpoint `{}` not declared in app `{}` (found `{}`)",
            check_name,
            manifest.name,
            checkpoint.name
        );
    }

    // Resolve checkpoint path relative to app.lzi's directory.
    let app_dir = app_path.parent().unwrap_or_else(|| Path::new("."));
    let snapshot_path = app_dir.join(&checkpoint.path);
    if !snapshot_path.exists() {
        bail!(
            "checkpoint `{}` references path `{}` that does not exist relative to {}",
            check_name,
            checkpoint.path,
            app_path.display()
        );
    }

    let text = fs::read_to_string(&snapshot_path)
        .with_context(|| format!("failed to read snapshot {}", snapshot_path.display()))?;
    let value: serde_json::Value = serde_json::from_str(&text)
        .with_context(|| format!("snapshot {} is not valid JSON", snapshot_path.display()))?;

    let expected_version = env!("CARGO_PKG_VERSION");
    let snapshot_version = value
        .get("lazuli_version")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if snapshot_version.is_empty() {
        println!(
            "checkpoint {}: ok (snapshot missing `lazuli_version`; regenerate to enable version drift detection)",
            check_name
        );
        return Ok(());
    }
    if snapshot_version != expected_version {
        println!(
            "checkpoint {}: ok (snapshot lazuli_version {} lags analyzer {}; consider regenerating)",
            check_name, snapshot_version, expected_version
        );
        return Ok(());
    }
    println!("checkpoint {}: ok", check_name);
    Ok(())
}

fn spike_generate_command(root: &Path, spec: Option<&Path>) -> Result<()> {
    let feature = match spec {
        Some(path) => {
            let text =
                fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
            serde_json::from_str(&text)
                .with_context(|| format!("parse runtime spec JSON {}", path.display()))?
        }
        None => lazuli_codegen_spec::customer_spike(),
    };
    let go_path = root.join("dist/go/customer/customer.gen.go");
    let ts_path = root.join("dist/web/customer/src/customer.gen.ts");

    let go_source = lazuli_codegen_go::emit_feature_go(&feature);
    let ts_source = lazuli_codegen_ts::emit_feature_ts(&feature);

    if let Some(parent) = go_path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    if let Some(parent) = ts_path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }

    fs::write(&go_path, go_source).with_context(|| format!("write {}", go_path.display()))?;
    fs::write(&ts_path, ts_source).with_context(|| format!("write {}", ts_path.display()))?;

    println!("wrote {}", go_path.display());
    println!("wrote {}", ts_path.display());
    Ok(())
}

fn check_command(
    input: &Path,
    security_profile: CheckSecurityProfile,
    allow_version_mismatch: bool,
) -> Result<()> {
    if !allow_version_mismatch {
        let project_root = project_root_for_input(input);
        let manifest = lazurite_manifest::load(&project_root).with_context(|| {
            format!(
                "failed to read {}",
                project_root.join("Lazurite.toml").display()
            )
        })?;
        version::enforce_manifest_pin(manifest.as_ref())?;
    }

    let inputs = check_inputs(input)?;
    let mut has_error = false;

    for path in &inputs {
        let source =
            fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
        let diagnostics =
            lazuli_lsp::diagnostics_for_source_with_profile(&source, security_profile.into());
        has_error |= diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Some(DiagnosticSeverity::ERROR));

        for diagnostic in &diagnostics {
            print_diagnostic(path, diagnostic);
        }
    }

    if has_error {
        bail!(
            "{} failed Lazuli checks under {:?} security profile",
            input.display(),
            security_profile
        );
    }

    println!("{} passed Lazuli checks", input.display());
    Ok(())
}

fn check_inputs(input: &Path) -> Result<Vec<PathBuf>> {
    if !input.is_dir() {
        return Ok(vec![input.to_path_buf()]);
    }

    let mut paths = Vec::new();
    let mut stack = vec![input.to_path_buf()];
    while let Some(path) = stack.pop() {
        for entry in fs::read_dir(&path)
            .with_context(|| format!("failed to read {}", path.display()))?
        {
            let path = entry
                .with_context(|| format!("failed to read entry under {}", path.display()))?
                .path();
            if path.is_dir() {
                stack.push(path);
            } else if matches!(
                path.extension().and_then(|extension| extension.to_str()),
                Some("lzi" | "lzx")
            ) {
                paths.push(path);
            }
        }
    }

    paths.sort();
    if paths.is_empty() {
        bail!("no .lzi or .lzx files found under {}", input.display());
    }
    Ok(paths)
}

fn print_diagnostic(input: &Path, diagnostic: &Diagnostic) {
    let severity = match diagnostic.severity {
        Some(DiagnosticSeverity::ERROR) => "error",
        Some(DiagnosticSeverity::WARNING) => "warning",
        Some(DiagnosticSeverity::INFORMATION) => "info",
        Some(DiagnosticSeverity::HINT) => "hint",
        _ => "diagnostic",
    };
    let code = diagnostic
        .code
        .as_ref()
        .map(|code| match code {
            tower_lsp::lsp_types::NumberOrString::String(value) => format!(" [{value}]"),
            tower_lsp::lsp_types::NumberOrString::Number(value) => format!(" [{value}]"),
        })
        .unwrap_or_default();
    println!(
        "{}:{}:{}: {severity}{code}: {}",
        input.display(),
        diagnostic.range.start.line + 1,
        diagnostic.range.start.character + 1,
        diagnostic.message
    );
}

fn parse_command(input: &Path) -> Result<()> {
    let app = compile_to_ir(input)?;
    println!("{}", serde_json::to_string_pretty(&app)?);
    Ok(())
}

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

fn debug_command(
    project_root: &Path,
    error_path: Option<&Path>,
    capsule: Option<String>,
    format: &str,
) -> Result<()> {
    let input = match error_path {
        Some(path) => fs::read_to_string(path)
            .with_context(|| format!("failed to read error envelope {}", path.display()))?,
        None => {
            let mut input = String::new();
            std::io::stdin()
                .read_to_string(&mut input)
                .context("failed to read error envelope from stdin")?;
            input
        }
    };
    let mut envelope: debug::ErrorEnvelopeInput =
        serde_json::from_str(&input).context("failed to parse error envelope JSON")?;
    if let Some(capsule) = capsule {
        envelope.capsule = capsule;
    }

    let bundle =
        debug::run_debug(project_root, envelope).map_err(|err| anyhow::anyhow!("{err}"))?;
    match format {
        "json" => println!("{}", serde_json::to_string_pretty(&bundle)?),
        "markdown" => print!("{}", debug::format_markdown(&bundle)),
        other => bail!("unsupported debug format `{other}`; expected json or markdown"),
    }

    Ok(())
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

fn project_root_for_input(input: &Path) -> PathBuf {
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

fn init_command(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create directory {}", parent.display()))?;
        }
    }

    fs::write(path, DEFAULT_TEMPLATE)
        .with_context(|| format!("failed to write {}", path.display()))?;
    println!("created {}", path.display());
    Ok(())
}

fn new_command(
    project: Option<&Path>,
    template: &str,
    bare: bool,
    no_git: bool,
    module: Option<String>,
    frontends: Option<String>,
    in_place: bool,
) -> Result<()> {
    if in_place {
        return new_in_place_command(project, template, bare, no_git, module, frontends);
    }

    let project = project.ok_or_else(|| {
        anyhow::anyhow!("missing project directory; pass a project name or use --in-place")
    })?;
    new_project_command(project, template, bare, no_git, module, frontends)
}

fn new_project_command(
    project: &Path,
    template: &str,
    bare: bool,
    no_git: bool,
    module: Option<String>,
    frontends: Option<String>,
) -> Result<()> {
    if project
        .try_exists()
        .with_context(|| format!("failed to inspect {}", project.display()))?
    {
        bail!("project path already exists: {}", project.display());
    }

    let app_name = pascal_case_project_name(project)?;
    let bare = bare || template == "bare";
    if !bare && template != "default" {
        bail!("unknown template `{template}`; supported templates: default, bare");
    }

    if bare {
        scaffold_bare(project, &app_name)?;
    } else {
        let module = module.unwrap_or_else(|| default_module_name(project));
        scaffold_from_template(&templates::DEFAULT_TEMPLATE, project, &app_name, &module)?;

        // The default `go.work` lists `.` and `./dist/go`. If we can
        // discover the local Lazuli runtime source (`runtime/go/`) on
        // this machine — either from `LAZULI_RUNTIME_PATH` or by
        // walking from this CLI binary's location — append a third
        // `use <abs path>` so `go build`/`go mod tidy` resolves
        // `lazuli.dev/runtime` without a published module. Hands-off
        // for installed (system) Lazuli binaries: if no runtime is
        // discovered the file stays as the user can wire the path
        // manually following the README hint.
        if let Some(runtime_dir) = locate_lazuli_runtime_dir() {
            if let Err(err) = inject_runtime_into_go_work(project, &runtime_dir) {
                eprintln!(
                    "warning: failed to write runtime path into go.work ({}): {err:#}",
                    runtime_dir.display()
                );
            }
        }

        if let Err(err) = run_go_mod_tidy(project) {
            eprintln!("warning: failed to run `go mod tidy`: {err:#}");
        }
        if let Err(err) = run_doctor_sanity_check(project) {
            eprintln!("warning: failed to run `lazuli doctor`: {err:#}");
        }
    }

    if let Some(frontends) = frontends.as_deref() {
        for frontend in parse_frontends(frontends)? {
            match frontend {
                FrontendScaffold::Web => {
                    cmd_new_frontends::scaffold_frontend_web(project, &app_name)?
                }
                FrontendScaffold::Mobile => {
                    cmd_new_frontends::scaffold_frontend_mobile(project, &app_name)?
                }
            }
        }
    }

    if !no_git {
        run_git_init(project)?;
    }

    println!("created {}", project.display());
    Ok(())
}

fn new_in_place_command(
    project: Option<&Path>,
    template: &str,
    bare: bool,
    _no_git: bool,
    module: Option<String>,
    frontends: Option<String>,
) -> Result<()> {
    if bare || template != "default" || module.is_some() {
        bail!("--in-place only supports --frontends on an existing Lazurite project");
    }

    let project_root = match project {
        Some(project) => project.to_path_buf(),
        None => std::env::current_dir().context("failed to determine current directory")?,
    };
    let manifest = project_root.join("Lazurite.toml");
    if !manifest
        .try_exists()
        .with_context(|| format!("failed to inspect {}", manifest.display()))?
    {
        bail!(
            "no Lazurite project in {}; run without --in-place to scaffold a new project",
            project_root.display()
        );
    }

    let frontends = frontends.as_deref().ok_or_else(|| {
        anyhow::anyhow!("--in-place requires --frontends web, mobile, or web,mobile")
    })?;
    let app_name = pascal_case_project_name(&project_root)?;

    for frontend in parse_frontends(frontends)? {
        match frontend {
            FrontendScaffold::Web => {
                let package_json = project_root
                    .join("app")
                    .join("web")
                    .join("package.json");
                let package_json_exists = package_json
                    .try_exists()
                    .with_context(|| format!("failed to inspect {}", package_json.display()))?;
                log_user_owned_frontend_skips(&project_root)?;
                cmd_new_frontends::scaffold_frontend_web(&project_root, &app_name)?;
                if package_json_exists {
                    merge_or_write_package_json(&package_json, templates::FRONTEND_PACKAGE_JSON)?;
                }
            }
            FrontendScaffold::Mobile => {
                cmd_new_frontends::scaffold_frontend_mobile(&project_root, &app_name)?
            }
        }
    }

    println!("updated {}", project_root.display());
    Ok(())
}

fn log_user_owned_frontend_skips(project_root: &Path) -> Result<()> {
    for relative in [
        "app/web/tailwind.config.ts",
        "app/web/tsconfig.json",
        "app/web/vite.config.ts",
    ] {
        let path = project_root.join(relative);
        if path
            .try_exists()
            .with_context(|| format!("failed to inspect {}", path.display()))?
        {
            eprintln!("skipping {relative}: already exists; user-owned");
        }
    }
    Ok(())
}

fn merge_or_write_package_json(path: &Path, template: &str) -> Result<()> {
    if !path
        .try_exists()
        .with_context(|| format!("failed to inspect {}", path.display()))?
    {
        fs::write(path, template).with_context(|| format!("writing {}", path.display()))?;
        return Ok(());
    }

    let existing_text =
        fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let mut existing: serde_json::Value = serde_json::from_str(&existing_text)
        .with_context(|| format!("parsing {}", path.display()))?;
    let template: serde_json::Value =
        serde_json::from_str(template).context("parsing frontend package.json template")?;

    merge_package_json_object(&mut existing, &template)?;

    let mut out = serde_json::to_string_pretty(&existing)?;
    out.push('\n');
    fs::write(path, out).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

fn merge_package_json_object(
    existing: &mut serde_json::Value,
    template: &serde_json::Value,
) -> Result<()> {
    let existing_obj = existing
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("package.json root must be a JSON object"))?;
    let template_obj = template
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("package.json template root must be a JSON object"))?;

    for key in ["name", "private", "type"] {
        if !existing_obj.contains_key(key) {
            if let Some(value) = template_obj.get(key) {
                existing_obj.insert(key.to_string(), value.clone());
            }
        }
    }

    for key in ["scripts", "dependencies", "devDependencies"] {
        merge_package_json_section(existing_obj, template_obj, key)?;
    }

    Ok(())
}

fn merge_package_json_section(
    existing_obj: &mut serde_json::Map<String, serde_json::Value>,
    template_obj: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<()> {
    let Some(template_section) = template_obj.get(key) else {
        return Ok(());
    };
    let Some(template_section) = template_section.as_object() else {
        bail!("package.json template section `{key}` must be an object");
    };

    if !existing_obj.contains_key(key) {
        existing_obj.insert(
            key.to_string(),
            serde_json::Value::Object(template_section.clone()),
        );
        return Ok(());
    }

    let existing_section = existing_obj
        .get_mut(key)
        .and_then(serde_json::Value::as_object_mut)
        .ok_or_else(|| anyhow::anyhow!("package.json section `{key}` must be an object"))?;
    for (dep, version) in template_section {
        existing_section
            .entry(dep.clone())
            .or_insert_with(|| version.clone());
    }

    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FrontendScaffold {
    Web,
    Mobile,
}

fn parse_frontends(raw: &str) -> Result<Vec<FrontendScaffold>> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for item in raw.split(',') {
        let item = item.trim();
        if item.is_empty() {
            bail!("empty frontend in --frontends; expected web, mobile, or web,mobile");
        }
        let frontend = match item {
            "web" => FrontendScaffold::Web,
            "mobile" => FrontendScaffold::Mobile,
            other => bail!("unknown frontend `{other}`; expected web, mobile, or web,mobile"),
        };
        if seen.insert(item.to_string()) {
            out.push(frontend);
        }
    }
    if out.is_empty() {
        bail!("--frontends requires web, mobile, or web,mobile");
    }
    Ok(out)
}

fn scaffold_bare(project: &Path, app_name: &str) -> Result<()> {
    let features_dir = project.join("features");
    fs::create_dir_all(&features_dir)
        .with_context(|| format!("failed to create directory {}", features_dir.display()))?;

    write_scaffold_file(&project.join("app.lzi"), &app_template(app_name))?;
    write_scaffold_file(&project.join("registry.lzi"), REGISTRY_TEMPLATE)?;
    write_scaffold_file(&project.join("README.md"), &readme_template(app_name))?;
    write_scaffold_file(&project.join(".gitignore"), GITIGNORE_TEMPLATE)?;
    write_scaffold_file(&features_dir.join(".gitkeep"), "")?;
    Ok(())
}

fn scaffold_from_template(
    template: &include_dir::Dir<'_>,
    target: &Path,
    app_name: &str,
    module: &str,
) -> Result<()> {
    // Snake_case slug for contexts that demand IDENT_LOWER (design
    // names, feature names, etc.). Templates emit verbatim — they can't
    // call helpers — so we precompute and substitute via `{{app_slug}}`.
    let app_slug = to_snake_case(app_name);
    for entry in template.entries() {
        match entry {
            include_dir::DirEntry::File(file) => {
                let mut out_path = target.join(file.path());
                let contents = if out_path.extension().and_then(|ext| ext.to_str()) == Some("tmpl")
                {
                    out_path.set_extension("");
                    file.contents_utf8()
                        .with_context(|| {
                            format!(
                                "template file is not valid UTF-8: {}",
                                file.path().display()
                            )
                        })?
                        .replace("{{app_name}}", app_name)
                        .replace("{{app_slug}}", &app_slug)
                        .replace("{{module}}", module)
                        .into_bytes()
                } else {
                    file.contents().to_vec()
                };

                if let Some(parent) = out_path.parent() {
                    fs::create_dir_all(parent).with_context(|| {
                        format!("failed to create directory {}", parent.display())
                    })?;
                }
                fs::write(&out_path, contents)
                    .with_context(|| format!("failed to write {}", out_path.display()))?;
            }
            include_dir::DirEntry::Dir(dir) => {
                let out_path = target.join(dir.path());
                fs::create_dir_all(&out_path).with_context(|| {
                    format!("failed to create directory {}", out_path.display())
                })?;
                scaffold_from_template(dir, target, app_name, module)?;
            }
        }
    }
    Ok(())
}

/// IDENT_LOWER (snake_case) of an arbitrary identifier. Used for
/// `design <name>` headers and other contexts that require lowercase
/// idents with `_` separators. Mirrors `to_kebab_case` but uses `_`.
fn to_snake_case(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut prev_lower = false;
    for ch in value.chars() {
        if ch == '-' || ch == ' ' {
            out.push('_');
            prev_lower = false;
            continue;
        }
        if ch.is_ascii_uppercase() {
            if prev_lower && !out.is_empty() {
                out.push('_');
            }
            for low in ch.to_lowercase() {
                out.push(low);
            }
            prev_lower = false;
            continue;
        }
        out.push(ch);
        prev_lower = ch.is_ascii_lowercase() || ch.is_ascii_digit();
    }
    out
}

fn default_module_name(project: &Path) -> String {
    let name = project
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("app");
    format!("lazuli/{}", to_kebab_case(name))
}

fn run_git_init(project: &Path) -> Result<()> {
    run_command(project, "git", &["init"])?;
    run_command(project, "git", &["add", "-A"])?;
    run_command(project, "git", &["commit", "-m", "initial: lazuli new"])?;
    Ok(())
}

fn run_go_mod_tidy(project: &Path) -> Result<()> {
    run_command(project, "go", &["mod", "tidy"])
}

/// Locate the Lazuli Go runtime checkout on this machine so the
/// scaffolded project can `use` it from `go.work`. We never publish
/// `lazuli.dev/runtime` to a real module proxy; the runtime is
/// always resolved as a local workspace replacement.
///
/// Resolution order:
/// 1. `LAZULI_RUNTIME_PATH` env var (escape hatch for non-standard
///    layouts and CI).
/// 2. Ancestors of the running `lazuli` binary — when developing
///    from this repo, the binary lives at
///    `<repo>/target/{debug,release}/lazuli(.exe)`, and the runtime
///    sits at `<repo>/runtime/go/`.
///
/// Returns `None` if no runtime checkout is found; the scaffold then
/// leaves `go.work` as-is and users wire the path manually.
fn locate_lazuli_runtime_dir() -> Option<PathBuf> {
    if let Ok(env_path) = std::env::var("LAZULI_RUNTIME_PATH") {
        let candidate = PathBuf::from(env_path);
        if is_lazuli_runtime_dir(&candidate) {
            return Some(candidate);
        }
    }
    let exe = std::env::current_exe().ok()?;
    for ancestor in exe.ancestors() {
        let candidate = ancestor.join("runtime").join("go");
        if is_lazuli_runtime_dir(&candidate) {
            return Some(candidate);
        }
    }
    None
}

/// A directory qualifies as the Lazuli runtime when it contains a
/// `go.mod` whose `module` line is exactly `lazuli.dev/runtime`. This
/// guards against picking up an unrelated `runtime/go/` directory in
/// some other project that happens to sit above the binary.
fn is_lazuli_runtime_dir(candidate: &Path) -> bool {
    let go_mod = candidate.join("go.mod");
    let Ok(contents) = fs::read_to_string(&go_mod) else {
        return false;
    };
    contents
        .lines()
        .any(|line| line.trim() == "module lazuli.dev/runtime")
}

/// Append `use <runtime_dir>` to the scaffold's `go.work`. The
/// scaffold ships a `go.work` with `.` and `./dist/go`; this adds the
/// local runtime as a third entry so `go mod tidy`/`go build` resolve
/// `lazuli.dev/runtime` without hitting the network.
///
/// We use an absolute path so the workspace works regardless of where
/// the project lives relative to the runtime checkout.
fn inject_runtime_into_go_work(project: &Path, runtime_dir: &Path) -> Result<()> {
    let go_work_path = project.join("go.work");
    let original = fs::read_to_string(&go_work_path)
        .with_context(|| format!("failed to read {}", go_work_path.display()))?;

    // Use forward slashes in `go.work` for cross-platform readability;
    // the go toolchain accepts both on Windows.
    let absolute = if runtime_dir.is_absolute() {
        runtime_dir.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| Path::new(".").to_path_buf())
            .join(runtime_dir)
    };
    let absolute_str = absolute.to_string_lossy().replace('\\', "/");

    // Idempotency: if the user already wired the runtime in go.work,
    // don't write again.
    if original.contains(&absolute_str) {
        return Ok(());
    }

    // Find the closing `)` of the `use ( ... )` block and inject our
    // line just before it. Falls back to appending a fresh block when
    // the template format ever drifts.
    let updated = if let Some(close_idx) = original.find(")") {
        let (head, tail) = original.split_at(close_idx);
        format!("{head}    {absolute_str}\n{tail}")
    } else {
        format!("{original}\nuse {absolute_str}\n")
    };

    fs::write(&go_work_path, updated)
        .with_context(|| format!("failed to write {}", go_work_path.display()))?;
    Ok(())
}

fn run_doctor_sanity_check(project: &Path) -> Result<()> {
    doctor::doctor_command(project, SecurityProfile::Strict, false, false)
}

fn run_command(project: &Path, program: &str, args: &[&str]) -> Result<()> {
    let status = Command::new(program)
        .args(args)
        .current_dir(project)
        .status()
        .with_context(|| format!("failed to start `{}`", command_display(program, args)))?;
    if !status.success() {
        bail!(
            "`{}` exited with status {}",
            command_display(program, args),
            status
        );
    }
    Ok(())
}

fn command_display(program: &str, args: &[&str]) -> String {
    std::iter::once(program)
        .chain(args.iter().copied())
        .collect::<Vec<_>>()
        .join(" ")
}

fn app_template(app_name: &str) -> String {
    format!("app {app_name}\n  urls\n    dev: \"http://localhost:3000\"\n")
}

fn readme_template(app_name: &str) -> String {
    format!(
        "# {app_name}\n\nGenerated with `lazuli new`.\n\nSee the Lazuli docs: https://github.com/lazuli-lang/lazuli/tree/main/docs\n"
    )
}

fn write_scaffold_file(path: &Path, contents: &str) -> Result<()> {
    fs::write(path, contents).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

fn pascal_case_project_name(project: &Path) -> Result<String> {
    let Some(name) = project.file_name().and_then(|name| name.to_str()) else {
        bail!("project path must end in a valid UTF-8 project name");
    };

    let app_name = pascal_case(name);
    if app_name.is_empty() {
        bail!("project name must contain at least one ASCII alphanumeric character");
    }

    Ok(app_name)
}

fn pascal_case(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for word in value.split(|c: char| !c.is_ascii_alphanumeric()) {
        if word.is_empty() {
            continue;
        }
        if out.is_empty() && word.chars().next().is_some_and(|ch| ch.is_ascii_digit()) {
            out.push_str("App");
        }
        if matches!(
            word.to_ascii_lowercase().as_str(),
            "id" | "url" | "uri" | "html" | "json" | "sql" | "ttl"
        ) {
            out.push_str(&word.to_ascii_uppercase());
            continue;
        }
        let mut chars = word.chars();
        if let Some(first) = chars.next() {
            out.push(first.to_ascii_uppercase());
        }
        out.push_str(chars.as_str());
    }

    out
}

fn lsp_command() -> Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("failed to start Lazuli LSP runtime")?;
    runtime.block_on(lazuli_lsp::serve_stdio());
    Ok(())
}

fn compile_to_ir(input: &Path) -> Result<lazuli_ir::Module> {
    build_module_from_path(input).context("failed to compile .lzi file")
}

fn write_generated_file(root: &Path, relative: &str, contents: &str) -> Result<()> {
    let path = root.join(relative);

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory {}", parent.display()))?;
    }

    fs::write(&path, contents).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

fn write_go_work_preserving_entries(project_root: &Path, generated_contents: &str) -> Result<()> {
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
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    use clap::Parser;
    use tempfile::TempDir;

    use super::{
        Cli, Commands, DesignCommand, DesignExportTarget, DesignImportFormat, ExpandSet,
        GenerateKind, MigrateCommand, REGISTRY_TEMPLATE, app_template, default_module_name,
        add_missing_go_work_use_entries, emit_feature_sdk_ts, expand_canonical_source,
        inspect_canonical_source, inspect_json_value, new_command, parse_expand_set, pascal_case,
        pascal_case_project_name, render_inspect_symbol_lazuli, scaffold_bare,
        scaffold_from_template, templates, write_go_work_preserving_entries,
    };

    // NOTE: tests for `query_ident` / `strip_query_verb_prefix` (the
    // verb-prefix dedup added alongside the Hostpoint bug fix) cannot
    // live here because the `lazuli_cli` test binary currently fails to
    // compile on this branch's base (pre-existing `doctor::lzx::ir_stub`
    // field mismatches, unrelated to this change — see `cargo test -p
    // lazuli_cli` baseline). The behaviour is covered by the matching
    // tests in `lazuli_codegen_ts::lzx::tests` (the helper logic is
    // identical and was factored to mirror the CLI's local copy).

    #[test]
    fn go_work_preserve_adds_dist_go_without_dropping_runtime() {
        let original = "go 1.26.0\n\nuse (\n\t.\n\tc:/Users/lucas/lazuli/runtime/go\n)\n";
        let generated = "go 1.26.0\n\nuse (\n\t.\n\t./dist/go\n)\n";
        let updated = add_missing_go_work_use_entries(
            original,
            &super::extract_go_work_use_entries(generated),
        );

        assert!(updated.contains("\t.\n"));
        assert!(updated.contains("\t./dist/go\n"));
        assert!(updated.contains("\tc:/Users/lucas/lazuli/runtime/go\n"));
        assert_eq!(updated.matches("./dist/go").count(), 1);
    }

    #[test]
    fn go_work_preserve_creates_missing_file_from_generated_contents() {
        let root = TempDir::new().unwrap();
        let generated = "go 1.26.0\n\nuse (\n\t.\n\t./dist/go\n)\n";

        write_go_work_preserving_entries(root.path(), generated).unwrap();

        let written = fs::read_to_string(root.path().join("go.work")).unwrap();
        assert_eq!(written, generated);
    }

    #[test]
    fn migrate_action_up_parses_target_flag() {
        let cli = Cli::try_parse_from([
            "lazuli",
            "migrate",
            "up",
            "--target",
            "20260513_001_account_user",
            "--yes",
        ])
        .unwrap();

        let Commands::Migrate {
            sub: MigrateCommand::Up { target, yes: true },
        } = cli.command
        else {
            panic!("expected migrate up command");
        };
        assert_eq!(target.as_deref(), Some("20260513_001_account_user"));
    }

    #[test]
    fn migrate_dsl_parses_from_to_and_dry_run() {
        let cli = Cli::try_parse_from([
            "lazuli",
            "migrate",
            "dsl",
            "--from",
            "v0.11",
            "--to",
            "v0.12",
            "--dry-run",
        ])
        .unwrap();
        let Commands::Migrate {
            sub:
                MigrateCommand::Dsl {
                    from,
                    to,
                    dry_run,
                    path,
                },
        } = cli.command
        else {
            panic!("expected migrate dsl command");
        };
        assert_eq!(from, "v0.11");
        assert_eq!(to, "v0.12");
        assert!(dry_run);
        assert!(path.is_none());
    }

    #[test]
    fn migrate_dsl_bootstrap_recipe_rewrites_real_source_end_to_end() {
        // End-to-end: stand up a tempdir project with the bootstrap
        // recipe (mirrored from
        // `migrations/recipes/v0.11-to-v0.12/00-rename-validates-resource.md`)
        // plus a real-shaped .lzi file using the legacy
        // `validates resource @validator.X` form. After
        // `run_migrate_dsl`, the file must (a) reflect the modern
        // form (b) parse cleanly via
        // `lazuli_syntax::parse_feature_skeletons` (c) survive a
        // second migrate pass as a no-op.
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root: PathBuf = std::env::temp_dir().join(format!(
            "lazuli-migrate-dsl-e2e-{}-{suffix}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        let recipe_dir = root.join("migrations/recipes/v0.11-to-v0.12");
        fs::create_dir_all(&recipe_dir).unwrap();
        let bootstrap = "---\n\
                         name: rename-validates-resource-keyword\n\
                         applies_to: .lzi\n\
                         match: |\n\
                         \x20\x20${indent:ws}validates resource @validator.${ref}\n\
                         replace: |\n\
                         \x20\x20${indent}validates @validator.${ref}\n\
                         description: Tier-4 cleanup.\n\
                         ---\n";
        fs::write(
            recipe_dir.join("00-rename-validates-resource.md"),
            bootstrap,
        )
        .unwrap();

        let feature_dir = root.join("features/customer");
        fs::create_dir_all(&feature_dir).unwrap();
        let original = "feature customer\n\
                        \x20\x20resource Customer\n\
                        \x20\x20\x20\x20name: Text\n\
                        \x20\x20\x20\x20validates resource @validator.row_check\n";
        let lzi_path = feature_dir.join("customer.lzi");
        fs::write(&lzi_path, original).unwrap();

        let report = crate::migrate::dsl::run_migrate_dsl(&root, "v0.11", "v0.12", false)
            .expect("migrate dsl");
        assert_eq!(report.changed.len(), 1, "report = {report:?}");
        assert!(report.rolled_back.is_empty(), "report = {report:?}");
        let after = fs::read_to_string(&lzi_path).unwrap();
        assert!(after.contains("validates @validator.row_check"));
        assert!(!after.contains("validates resource"));

        // Survives a sanity reparse via the canonical feature-skeleton parser.
        lazuli_syntax::parse_feature_skeletons(&after).expect("reparse rewritten .lzi");

        // Second pass is a no-op: no legacy form left to match.
        let report2 = crate::migrate::dsl::run_migrate_dsl(&root, "v0.11", "v0.12", false)
            .expect("second migrate dsl");
        assert!(report2.changed.is_empty());

        let _ = fs::remove_dir_all(&root);
    }


    #[test]
    fn positive_enum_emits_const_and_type_alias() {
        let (feature, module) = enum_sdk_fixture(false, false);
        let output = emit_feature_sdk_ts(&feature, &module);

        assert!(
            output.contains("export const ITEM_TYPE_VALUES = [\"doc\", \"decision\"] as const;")
        );
        assert!(output.contains("export type ItemType = typeof ITEM_TYPE_VALUES[number];"));
    }

    #[test]
    fn enum_metadata_options_golden_emits_typed_literal() {
        let (mut feature, mut module) = enum_sdk_fixture(false, false);
        let item_type = feature
            .enums
            .iter_mut()
            .find(|decl| decl.name == "ItemType")
            .expect("ItemType enum");
        item_type.variants[0].label_key = Some("item_doc".to_owned());
        item_type.variants[0].icon_key = Some("file-text".to_owned());
        item_type.variants[1].label_key = Some("item_decision".to_owned());
        item_type.variants[1].hint_key = Some("item_decision_hint".to_owned());
        module.features = vec![feature.clone()];

        let output = emit_feature_sdk_ts(&feature, &module);

        assert!(output.contains("export const ITEM_TYPE_VALUES = [\"doc\", \"decision\"] as const;"));
        assert!(output.contains("export type ItemType = typeof ITEM_TYPE_VALUES[number];"));
        assert!(output.contains(
            "export const ITEM_TYPE_OPTIONS: ReadonlyArray<{\n  value: ItemType;\n  labelKey: string;\n  hintKey?: string;\n  iconKey?: string;\n}> = ["
        ));
        assert!(output.contains(
            "  { value: \"doc\", labelKey: \"item_doc\", iconKey: \"file-text\" },"
        ));
        assert!(output.contains(
            "  { value: \"decision\", labelKey: \"item_decision\", hintKey: \"item_decision_hint\" },"
        ));
    }

    #[test]
    fn enum_without_metadata_golden_omits_options() {
        let (feature, module) = enum_sdk_fixture(false, false);
        let output = emit_feature_sdk_ts(&feature, &module);

        assert!(output.contains("export const ITEM_TYPE_VALUES"));
        assert!(!output.contains("ITEM_TYPE_OPTIONS"));
    }

    #[test]
    fn positive_enum_field_uses_lifted_type() {
        let (feature, module) = enum_sdk_fixture(false, false);
        let output = emit_feature_sdk_ts(&feature, &module);

        assert!(output.contains("  type: ItemType;"));
        assert!(!output.contains("  type: unknown;"));
    }

    #[test]
    fn positive_list_of_text_emits_array() {
        let (feature, module) = enum_sdk_fixture(false, false);
        let output = emit_feature_sdk_ts(&feature, &module);

        assert!(output.contains("  tags: string[];"));
    }

    #[test]
    fn positive_list_of_enum_emits_typed_array() {
        let (feature, module) = enum_sdk_fixture(false, false);
        let output = emit_feature_sdk_ts(&feature, &module);

        assert!(output.contains("  categories: ItemType[];"));
    }

    #[test]
    fn negative_unreferenced_enum_not_emitted() {
        let (feature, module) = enum_sdk_fixture(true, false);
        let output = emit_feature_sdk_ts(&feature, &module);

        assert!(!output.contains("UNUSED_VALUES"));
        assert!(!output.contains("export type Unused"));
    }

    #[test]
    fn user_defined_tagged_enum_field_still_lifts_to_typed_alias() {
        // Regression for review bug #3 (2026-05-15): fields like
        // `tier: CustomerTier = free` arrive as
        // `TypeRef::UserDefined({name: "ItemType"})` instead of
        // `EnumRef(...)` because the analyzer's resolve pass doesn't
        // always promote them. Before the fix, `ts_type_for_type_ref`
        // checked records but not enums under that arm and emitted
        // `tier: unknown` — making the SDK lose enum typing.
        let (mut feature, mut module) = enum_sdk_fixture(false, false);
        // Replace the EnumRef-tagged `type` field with a UserDefined-
        // tagged one. Everything else identical.
        let resource = feature.resources.first_mut().expect("fixture resource");
        let type_field = resource
            .fields
            .iter_mut()
            .find(|f| f.name == "type")
            .expect("type field");
        type_field.type_ref = lazuli_ir::TypeRef::UserDefined(local_qn("ItemType"));
        // Module must mirror the feature's resource for the lookup.
        module.features = vec![feature.clone()];

        let output = emit_feature_sdk_ts(&feature, &module);

        assert!(
            output.contains("  type: ItemType;"),
            "UserDefined-tagged enum field must resolve to the typed alias; got:\n{output}"
        );
        assert!(
            !output.contains("  type: unknown;"),
            "UserDefined-tagged enum field must not fall through to `unknown`; got:\n{output}"
        );
        assert!(
            output.contains("export type ItemType = typeof ITEM_TYPE_VALUES[number];"),
            "alias must still be emitted at the top of the file when only a UserDefined ref drives it; got:\n{output}"
        );
    }

    #[test]
    fn command_sdk_emits_policy_rate_limit_audit_metadata() {
        // Regression for review bug #7 (2026-05-15): the TS SDK
        // previously emitted only `invalidates:` on `defineCommand`,
        // losing the Go-side Policy/RateLimit/Audit. Clients had to
        // call a separate metadata RPC (which didn't exist) to drive
        // policy-aware affordances or rate-limit-aware backoff.
        let (mut feature, mut module) = enum_sdk_fixture(false, false);
        feature.policies = lazuli_ir::Policies {
            categories: vec![lazuli_ir::PolicyCategory {
                name: "update".to_owned(),
                atoms: vec!["@role.admin".to_owned(), "@role.sales".to_owned()],
                previous_names: vec![],
                when_denied: None,
                when_denied_route: None,
            }],
            fields: vec![],
            span_ref: None,
        };
        feature.commands.push(lazuli_ir::Command {
            name: "update_item".to_owned(),
            public_contract: None,
            kind: lazuli_ir::CommandKind::Update,
            route: vec![],
            input: lazuli_ir::CommandInput::Typed(vec![]),
            target: None,
            lets: vec![],
            effect: lazuli_ir::CommandEffect::None,
            policy: lazuli_ir::PolicyRef::Atom("policy.update".to_owned()),
            policy_expr: None,
            policy_when_denied: None,
            emits: vec![],
            rate_limit: Some(lazuli_ir::RateLimitSpec::from_default(
                "30 per hour per user".to_owned(),
            )),
            audit: Some(lazuli_ir::AuditSpec {
                subjects: vec![],
                emit_to: None,
                data_subject: None,
                record_before: false,
                record_after: false,
                retain_for: None,
            }),
            approval: None,
            invalidates: vec![],
            external_calls: vec![],
            timeout: None,
            retry: None,
            idempotency: None,
            write_window: None,
            deprecated: None,
            handler: None,
            tests: None,
            triggers: vec![],
            synthesized_from_cap_file: None,
            previous_names: vec![],
            span_ref: None,
            owner_scope_sql: None,
        });
        module.features = vec![feature.clone()];

        let output = emit_feature_sdk_ts(&feature, &module);

        assert!(
            output.contains("policy: { name: \"@policy.update\", atoms: ["),
            "policy name must qualify with @policy. prefix; got:\n{output}"
        );
        assert!(
            output.contains("{ namespace: \"role\", name: \"admin\" }"),
            "policy atoms must resolve via feature.policies dictionary; got:\n{output}"
        );
        assert!(
            output.contains("{ namespace: \"role\", name: \"sales\" }"),
            "all atoms from the matching category must be emitted; got:\n{output}"
        );
        assert!(
            output.contains("rateLimit: \"30 per hour per user\""),
            "rateLimit must surface to the TS SDK; got:\n{output}"
        );
        assert!(
            output.contains("audit: \"default\""),
            "empty-subject AuditSpec must lower to the \"default\" sentinel; got:\n{output}"
        );
    }

    #[test]
    fn command_sdk_omits_metadata_when_absent() {
        // Counterpoint: when the DSL omits a piece of metadata the SDK
        // must omit the property entirely rather than emit it as
        // `undefined` (TS `exactOptionalPropertyTypes` discipline).
        let (mut feature, mut module) = enum_sdk_fixture(false, false);
        feature.commands.push(lazuli_ir::Command {
            name: "bare".to_owned(),
            public_contract: None,
            kind: lazuli_ir::CommandKind::Update,
            route: vec![],
            input: lazuli_ir::CommandInput::Typed(vec![]),
            target: None,
            lets: vec![],
            effect: lazuli_ir::CommandEffect::None,
            policy: lazuli_ir::PolicyRef::None,
            policy_expr: None,
            policy_when_denied: None,
            emits: vec![],
            rate_limit: None,
            audit: None,
            approval: None,
            invalidates: vec![],
            external_calls: vec![],
            timeout: None,
            retry: None,
            idempotency: None,
            write_window: None,
            deprecated: None,
            handler: None,
            tests: None,
            triggers: vec![],
            synthesized_from_cap_file: None,
            previous_names: vec![],
            span_ref: None,
            owner_scope_sql: None,
        });
        module.features = vec![feature.clone()];

        let output = emit_feature_sdk_ts(&feature, &module);

        assert!(!output.contains("policy:"), "expected no policy line; got:\n{output}");
        assert!(!output.contains("rateLimit:"), "expected no rateLimit line; got:\n{output}");
        assert!(!output.contains("audit:"), "expected no audit line; got:\n{output}");
        // invalidates is always emitted even when empty — that's the
        // existing contract that this test does not change.
        assert!(output.contains("invalidates: []"));
    }

    #[test]
    fn cap_file_request_upload_emits_command_spec_for_react_hook() {
        // Wave C.2 upload hooks call request_*_upload through
        // useLazuliCommand because minting a signed PUT URL is an
        // imperative upload step, not a cacheable read. The get-url
        // command remains query-shaped so the hook can expose photoUri
        // from TanStack Query state.
        let source = r#"feature host
  defaults
    tenancy org

  uses org
  uses account

  policies
    host_only: @scope.authenticated, @role.host

  domain
    resource Host
      org: Org required
      user: User required unique
      profile_photo: @cap.File(max_size:5mb,accept:image/jpeg,visibility:signed,signed_ttl:1h) optional
"#;
        let parsed = lazuli_syntax::parse_feature_skeletons(source).expect("feature parses");
        let feature =
            lazuli_analyzer::lower_feature_skeleton(&parsed[0]).expect("feature lowers");
        let module = lazuli_ir::Module {
            workspace: None,
            contracts: vec![],
            app: None,
            registry: None,
            profiles: vec![],
            design: None,
            rbac: None,
            features: vec![feature.clone()],
        };

        let output = emit_feature_sdk_ts(&feature, &module);

        assert!(
            output.contains(
                "export const requestHostProfilePhotoUpload = defineCommand<RequestHostProfilePhotoUploadInput, ProfilePhotoUploadIntent>(\"host.request_profile_photo_upload\", {"
            ),
            "request upload must remain a CommandSpec for useLazuliCommand; got:\n{output}"
        );
        assert!(
            output.contains(
                "export const getHostProfilePhotoURL = defineQuery<GetHostProfilePhotoURLInput, ProfilePhotoDisplayUrl>(\"host.get_profile_photo_url\");"
            ),
            "get-url stays query-shaped for photoUri cache state; got:\n{output}"
        );
    }

    #[test]
    fn unresolved_bare_enum_name_recovers_to_typed_alias() {
        // Regression for the deeper fallback in `ts_type_for_type_ref`:
        // when the analyzer leaves a field as
        // `TypeRef::Unresolved("ItemType")` (no `@` prefix), the emitter
        // should still recover by walking the module's enum catalog
        // rather than emitting `unknown`. Without this branch, partial
        // analyzer failures would silently destroy the TS SDK's type
        // information.
        let (mut feature, mut module) = enum_sdk_fixture(false, false);
        let resource = feature.resources.first_mut().expect("fixture resource");
        let type_field = resource
            .fields
            .iter_mut()
            .find(|f| f.name == "type")
            .expect("type field");
        type_field.type_ref = lazuli_ir::TypeRef::Unresolved("ItemType".to_owned());
        module.features = vec![feature.clone()];

        let output = emit_feature_sdk_ts(&feature, &module);

        assert!(
            output.contains("  type: ItemType;"),
            "Unresolved-but-known-enum must self-heal to the typed alias; got:\n{output}"
        );
        assert!(!output.contains("  type: unknown;"));
    }

    #[test]
    fn dedup_enum_referenced_twice_emits_once() {
        let (feature, module) = enum_sdk_fixture(false, true);
        let output = emit_feature_sdk_ts(&feature, &module);

        assert_eq!(occurrences(&output, "export const ITEM_TYPE_VALUES"), 1);
        assert_eq!(occurrences(&output, "export type ItemType"), 1);
    }

    #[test]
    fn query_view_sdk_uses_declared_returns_type() {
        let (mut feature, mut module) = enum_sdk_fixture(false, false);
        feature.name = "host".to_owned();
        feature.records.push(lazuli_ir::Record {
            name: "HostHomeRow".to_owned(),
            public_contract: None,
            fields: vec![field(
                "id",
                lazuli_ir::TypeRef::Builtin(lazuli_ir::BuiltinType::Id),
            )],
            discriminator_field: None,
            span_ref: None,
        });
        feature.queries.push(lazuli_ir::Query::Sql(lazuli_ir::SqlQuery {
            name: "host_home_view".to_owned(),
            sql_kind: lazuli_ir::SqlQueryKind::View,
            public_contract: None,
            params: vec![lazuli_ir::TypedSlot {
                name: "user_id".to_owned(),
                type_ref: lazuli_ir::TypeRef::Builtin(lazuli_ir::BuiltinType::Id),
                required: true,
                constraints: lazuli_ir::FieldConstraints::default(),
            }],
            scope: Vec::new(),
            scope_override: false,
            returns: lazuli_ir::TypeRef::Many(Box::new(lazuli_ir::TypeRef::UserDefined(
                local_qn("HostHomeRow"),
            ))),
            sql_path: "app/features/host/queries/host_home_view.sql".to_owned(),
            cache: None,
            policy: lazuli_ir::PolicyRef::None,
            policy_expr: None,
            policy_when_denied: None,
            previous_names: Vec::new(),
            span_ref: None,
        }));
        module.features = vec![feature.clone()];

        let output = emit_feature_sdk_ts(&feature, &module);

        assert!(
            output.contains(
                "export const listHostHomeViewHosts = defineQuery<{ user_id: ID }, HostHomeRow[]>(\"host.host_home_view\");"
            ),
            "query.view SDK should use the declared typed returns shape; got:\n{output}"
        );
    }

    fn enum_sdk_fixture(
        include_unused_enum: bool,
        include_second_resource: bool,
    ) -> (lazuli_ir::Feature, lazuli_ir::Module) {
        let mut enums = vec![lazuli_ir::EnumDecl {
            name: "ItemType".to_owned(),
            public_contract: None,
            variants: vec![
                lazuli_ir::EnumVariant {
                    name: "Doc".to_owned(),
                    storage_value: None,
                    label_key: None,
                    hint_key: None,
                    icon_key: None,
                    previous_names: vec![],
                },
                lazuli_ir::EnumVariant {
                    name: "Decision".to_owned(),
                    storage_value: Some(lazuli_ir::StorageValue::String("decision".to_owned())),
                    label_key: None,
                    hint_key: None,
                    icon_key: None,
                    previous_names: vec![],
                },
            ],
            previous_names: vec![],
            span_ref: None,
        }];
        if include_unused_enum {
            enums.push(lazuli_ir::EnumDecl {
                name: "Unused".to_owned(),
                public_contract: None,
                variants: vec![lazuli_ir::EnumVariant {
                    name: "Legacy".to_owned(),
                    storage_value: None,
                    label_key: None,
                    hint_key: None,
                    icon_key: None,
                    previous_names: vec![],
                }],
                previous_names: vec![],
                span_ref: None,
            });
        }

        let mut resources = vec![resource(
            "Item",
            vec![
                field("type", lazuli_ir::TypeRef::EnumRef(local_qn("ItemType"))),
                field(
                    "tags",
                    lazuli_ir::TypeRef::Many(Box::new(lazuli_ir::TypeRef::Builtin(
                        lazuli_ir::BuiltinType::Text,
                    ))),
                ),
                field(
                    "categories",
                    lazuli_ir::TypeRef::Many(Box::new(lazuli_ir::TypeRef::EnumRef(local_qn(
                        "ItemType",
                    )))),
                ),
            ],
        )];
        if include_second_resource {
            resources.push(resource(
                "Note",
                vec![field(
                    "type",
                    lazuli_ir::TypeRef::EnumRef(local_qn("ItemType")),
                )],
            ));
        }

        let feature = lazuli_ir::Feature {
            name: "item".to_owned(),
            purpose: None,
            non_goals: vec![],
            context_path: None,
            defaults: lazuli_ir::Defaults::default(),
            uses: vec![],
            uses_spans: Vec::new(),
            uses_versions: Vec::new(),
            requirements: vec![],
            enums,
            resources,
            events: vec![],
            rules: vec![],
            policies: lazuli_ir::Policies::default(),
            errors: None,
            commands: vec![],
            apis: vec![],
            records: vec![],
            queries: vec![],
            resume_routers: vec![],
            workflows: vec![],
            jobs: vec![],
            webhooks: vec![],
            notifications: vec![],
            event_groups: vec![],
            tenant_migrations: vec![],
            translation: None,
            pollers: vec![],
            auth: None,
            surfaces: vec![],
            extensions: vec![],
            escape_routes: vec![],
            agents: vec![],
            reports: vec![],
            channels: vec![],
            caches: vec![],
            aggregates: vec![],
            mcp_servers: vec![],
            previous_names: vec![],
            synth_origins: std::collections::BTreeMap::new(),
            span_ref: None,
        };
        let module = lazuli_ir::Module {
            workspace: None,
            contracts: vec![],
            app: None,
            registry: None,
            profiles: vec![],
            design: None,
            rbac: None,
            features: vec![feature.clone()],
        };
        (feature, module)
    }

    fn resource(name: &str, fields: Vec<lazuli_ir::Field>) -> lazuli_ir::Resource {
        lazuli_ir::Resource {
            name: name.to_owned(),
            public_contract: None,
            tenancy: None,
            soft_delete: false,
            timestamps: None,
            fields,
            constraints: vec![],
            validate: None,
            validates: vec![],
            retention: None,
            previous_names: vec![],
            span_ref: None,
            lifecycle: None,
            invariants: vec![],
            lock: None,
            composite_key: None,
            conventions: vec![],
        }
    }

    fn field(name: &str, type_ref: lazuli_ir::TypeRef) -> lazuli_ir::Field {
        lazuli_ir::Field {
            name: name.to_owned(),
            type_ref,
            required: true,
            unique: false,
            slug: false,
            default: None,
            derived_from: None,
            constraints: lazuli_ir::FieldConstraints::default(),
            full_text: false,
            previous_names: vec![],
            pii: None,
            owner_axis: None,
            span_ref: None,
        }
    }

    #[test]
    fn plugin_semantic_type_emits_ts_alias_and_field_reference() {
        // B3 — `@semantic.BrazilianCPF` lowers to a SemanticPluginType
        // with carrier = Text. The SDK emitter writes
        // `export type BrazilianCPF = string;` at the file head and
        // references it in every consuming interface. See
        // `docs/proposals/semantic-types-plugin-locales.md` §Codegen.
        let mut feature = lazuli_ir::Feature {
            name: "host".to_owned(),
            purpose: None,
            non_goals: vec![],
            context_path: None,
            defaults: lazuli_ir::Defaults::default(),
            uses: vec![],
            uses_spans: Vec::new(),
            uses_versions: Vec::new(),
            requirements: vec![],
            enums: vec![],
            resources: vec![],
            events: vec![],
            rules: vec![],
            policies: lazuli_ir::Policies::default(),
            errors: None,
            commands: vec![],
            apis: vec![],
            records: vec![],
            queries: vec![],
            resume_routers: vec![],
            workflows: vec![],
            jobs: vec![],
            webhooks: vec![],
            notifications: vec![],
            event_groups: vec![],
            tenant_migrations: vec![],
            translation: None,
            pollers: vec![],
            auth: None,
            surfaces: vec![],
            extensions: vec![],
            escape_routes: vec![],
            agents: vec![],
            reports: vec![],
            channels: vec![],
            caches: vec![],
            aggregates: vec![],
            mcp_servers: vec![],
            previous_names: vec![],
            synth_origins: std::collections::BTreeMap::new(),
            span_ref: None,
        };
        feature.resources.push(resource(
            "Host",
            vec![field(
                "cpf",
                lazuli_ir::TypeRef::Builtin(lazuli_ir::BuiltinType::SemanticPluginType {
                    plugin: "@plugin/scalars-br".to_owned(),
                    name: "BrazilianCPF".to_owned(),
                    carrier: Box::new(lazuli_ir::BuiltinType::Text),
                    validator: "ValidateCPF".to_owned(),
                }),
            )],
        ));
        let module = lazuli_ir::Module {
            workspace: None,
            contracts: vec![],
            app: None,
            registry: None,
            profiles: vec![],
            design: None,
            rbac: None,
            features: vec![feature.clone()],
        };
        let out = emit_feature_sdk_ts(&feature, &module);
        assert!(
            out.contains("export type BrazilianCPF = string;"),
            "expected brand alias, got:\n{out}"
        );
        assert!(
            out.contains("cpf: BrazilianCPF;"),
            "expected typed field, got:\n{out}"
        );
    }

    fn local_qn(name: &str) -> lazuli_ir::QualifiedName {
        lazuli_ir::QualifiedName {
            feature: None,
            name: name.to_owned(),
        }
    }

    fn occurrences(haystack: &str, needle: &str) -> usize {
        haystack.match_indices(needle).count()
    }

    #[test]
    fn wave2_cli_dispatch_parses_new_surfaces() {
        let cli = Cli::try_parse_from(["lazuli", "generate", "feature", "billing"]).unwrap();
        let Commands::Generate {
            kind: GenerateKind::Feature,
            input,
            ..
        } = cli.command
        else {
            panic!("expected generate feature command");
        };
        assert_eq!(input, PathBuf::from("billing"));

        let cli =
            Cli::try_parse_from(["lazuli", "new", "demo", "--frontends", "web,mobile"]).unwrap();
        let Commands::New {
            frontends: Some(frontends),
            ..
        } = cli.command
        else {
            panic!("expected new command with frontends");
        };
        assert_eq!(frontends, "web,mobile");

        let cli =
            Cli::try_parse_from(["lazuli", "new", "--frontends", "web", "--in-place"]).unwrap();
        let Commands::New {
            project_name: None,
            frontends: Some(frontends),
            in_place: true,
            ..
        } = cli.command
        else {
            panic!("expected in-place new command without project name");
        };
        assert_eq!(frontends, "web");

        let cli = Cli::try_parse_from([
            "lazuli",
            "design",
            "import",
            "--from",
            "tokens.figma.json",
            "--format",
            "figma",
            "--overwrite",
        ])
        .unwrap();
        let Commands::Design {
            sub:
                DesignCommand::Import {
                    format: DesignImportFormat::Figma,
                    overwrite: true,
                    ..
                },
        } = cli.command
        else {
            panic!("expected design import command");
        };

        let cli = Cli::try_parse_from([
            "lazuli",
            "design",
            "export",
            "--target",
            "style-dictionary",
            "--out",
            "tokens.sd.json",
        ])
        .unwrap();
        let Commands::Design {
            sub:
                DesignCommand::Export {
                    target: DesignExportTarget::StyleDictionary,
                    ..
                },
        } = cli.command
        else {
            panic!("expected design export command");
        };

        let cli = Cli::try_parse_from(["lazuli", "design", "diff", "--against", "tokens.sd.json"])
            .unwrap();
        let Commands::Design {
            sub: DesignCommand::Diff { against },
        } = cli.command
        else {
            panic!("expected design diff command");
        };
        assert_eq!(against, PathBuf::from("tokens.sd.json"));
    }

    #[test]
    fn in_place_appends_manifest_block() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        fs::write(
            root.join("Lazurite.toml"),
            "[lazuli]\nversion = \"0.1.0\"\n",
        )
        .unwrap();

        new_command(
            Some(root),
            "default",
            false,
            true,
            None,
            Some("web".to_string()),
            true,
        )
        .unwrap();

        let manifest = fs::read_to_string(root.join("Lazurite.toml")).unwrap();
        assert!(manifest.contains("[lazuli]"));
        assert!(manifest.contains("[frontends.web]"));
        assert!(manifest.contains("target = \"tanstack-vite\""));
        assert!(manifest.contains("source = \"app/web\""));
    }

    #[test]
    fn in_place_preserves_existing_files() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        fs::write(
            root.join("Lazurite.toml"),
            "[lazuli]\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        fs::create_dir_all(root.join("app/web")).unwrap();
        fs::write(
            root.join("app/web/tailwind.config.ts"),
            "// custom tailwind\n",
        )
        .unwrap();

        new_command(
            Some(root),
            "default",
            false,
            true,
            None,
            Some("web".to_string()),
            true,
        )
        .unwrap();

        assert_eq!(
            fs::read_to_string(root.join("app/web/tailwind.config.ts")).unwrap(),
            "// custom tailwind\n"
        );
    }

    #[test]
    fn in_place_writes_missing_files() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        fs::write(
            root.join("Lazurite.toml"),
            "[lazuli]\nversion = \"0.1.0\"\n",
        )
        .unwrap();

        new_command(
            Some(root),
            "default",
            false,
            true,
            None,
            Some("web".to_string()),
            true,
        )
        .unwrap();

        assert!(root.join("app/web/index.html").is_file());
        assert!(root.join("app/web/main.tsx").is_file());
        assert!(root.join("app/web/shell/root.tsx").is_file());
        assert!(root.join("app/web/shell/layout.tsx").is_file());
        assert!(
            root.join("app/web/theme/theme_provider.tsx")
                .is_file()
        );
        assert!(root.join("app/web/theme/globals.css").is_file());
        assert!(root.join("app/web/tailwind.config.ts").is_file());
        assert!(root.join("app/web/tsconfig.json").is_file());
        assert!(root.join("app/web/vite.config.ts").is_file());
    }

    #[test]
    fn in_place_without_manifest_errors() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();

        let err = new_command(
            Some(root),
            "default",
            false,
            true,
            None,
            Some("web".to_string()),
            true,
        )
        .unwrap_err();

        assert!(
            err.to_string().contains("no Lazurite project in")
                && err
                    .to_string()
                    .contains("run without --in-place to scaffold a new project"),
            "{err:#}"
        );
    }

    #[test]
    fn in_place_merges_package_json() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        fs::write(
            root.join("Lazurite.toml"),
            "[lazuli]\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        fs::create_dir_all(root.join("app/web")).unwrap();
        fs::write(
            root.join("app/web/package.json"),
            r#"{
  "name": "custom-app",
  "dependencies": {
    "left-pad": "1.3.0",
    "react": "18.0.0"
  },
  "devDependencies": {
    "custom-dev-tool": "0.1.0"
  }
}
"#,
        )
        .unwrap();

        new_command(
            Some(root),
            "default",
            false,
            true,
            None,
            Some("web".to_string()),
            true,
        )
        .unwrap();

        let package_json: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(root.join("app/web/package.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(package_json["name"], "custom-app");
        assert_eq!(package_json["dependencies"]["left-pad"], "1.3.0");
        assert_eq!(package_json["dependencies"]["react"], "18.0.0");
        assert_eq!(package_json["devDependencies"]["custom-dev-tool"], "0.1.0");
        assert!(package_json["dependencies"]["@tanstack/react-query"].is_string());
        assert!(package_json["dependencies"]["@lazuli/runtime"].is_string());
        assert!(package_json["devDependencies"]["vite"].is_string());
    }

    #[test]
    fn pascal_case_converts_project_names() {
        assert_eq!(pascal_case("my-app"), "MyApp");
        assert_eq!(pascal_case("acme_crm"), "AcmeCrm");
        assert_eq!(pascal_case("123-api"), "App123Api");
    }

    #[test]
    fn pascal_case_project_name_handles_kebab_and_snake() {
        assert_eq!(
            pascal_case_project_name(Path::new("my-app")).unwrap(),
            "MyApp"
        );
        assert_eq!(
            pascal_case_project_name(Path::new("acme_crm")).unwrap(),
            "AcmeCrm"
        );
    }

    #[test]
    fn default_module_name_derives_from_project_name() {
        assert_eq!(default_module_name(Path::new("my-app")), "lazuli/my-app");
        assert_eq!(
            default_module_name(Path::new("acme_crm")),
            "lazuli/acme-crm"
        );
        assert_eq!(default_module_name(Path::new("AcmeCRM")), "lazuli/acme-crm");
    }

    #[test]
    fn scaffold_bare_writes_minimal_files() {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("lazuli-bare-test-{}-{suffix}", std::process::id()));
        let _ = fs::remove_dir_all(&root);

        let bare = root.join("bare-app");
        scaffold_bare(&bare, "BareApp").unwrap();
        assert_eq!(
            fs::read_to_string(bare.join("app.lzi")).unwrap(),
            app_template("BareApp")
        );
        assert_eq!(
            fs::read_to_string(bare.join("registry.lzi")).unwrap(),
            REGISTRY_TEMPLATE
        );
        assert!(bare.join("README.md").is_file());
        assert!(bare.join(".gitignore").is_file());
        assert!(bare.join("features").join(".gitkeep").is_file());
        assert!(!bare.join("Lazurite.toml").exists());
        assert!(!bare.join("features").join("account").exists());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn scaffold_from_template_substitutes_placeholders() {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "lazuli-template-substitute-test-{}-{suffix}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);

        scaffold_from_template(
            &templates::DEFAULT_TEMPLATE,
            &root,
            "MyApp",
            "github.com/me/myapp",
        )
        .unwrap();
        assert!(
            fs::read_to_string(root.join("app/app.lzi"))
                .unwrap()
                .contains("app MyApp")
        );
        assert!(
            fs::read_to_string(root.join("go.mod"))
                .unwrap()
                .contains("module github.com/me/myapp")
        );
        assert!(
            fs::read_to_string(root.join("README.md"))
                .unwrap()
                .contains("# MyApp")
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn scaffold_from_template_strips_tmpl_extension() {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "lazuli-template-extension-test-{}-{suffix}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);

        scaffold_from_template(
            &templates::DEFAULT_TEMPLATE,
            &root,
            "MyApp",
            "lazuli/my-app",
        )
        .unwrap();
        assert!(root.join("app/app.lzi").is_file());
        assert!(!root.join("app/app.lzi.tmpl").exists());
        assert!(root.join("app/features/account/account.lzi").is_file());
        assert!(!root.join("app/features/account/account.lzi.tmpl").exists());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    #[ignore = "smoke test for the complete embedded Lazurite scaffold tree"]
    fn scaffold_from_template_smoke_tree_matches_expected() {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "lazuli-template-smoke-test-{}-{suffix}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);

        scaffold_from_template(
            &templates::DEFAULT_TEMPLATE,
            &root,
            "MyApp",
            "lazuli/my-app",
        )
        .unwrap();
        // Handler starter `.go` files are not scaffolded — the codegen
        // handler-stub emitter (`crates/lazuli_codegen_go/src/emitter/
        // handlers.rs`) lays them down in `dist/go/<feature>/<name>.go`
        // on first `lazuli generate go`. The scaffold owns `.lzi` /
        // `.lzx` / `.tmpl` (notification templates) / config; user Go
        // handlers materialise via the codegen path.
        for relative in [
            ".gitignore",
            "README.md",
            "app/app.lzi",
            "app/design.lzi",
            "go.mod",
            "go.work",
            "Lazurite.toml",
            "app/registry.lzi",
            "app/features/account/account.lzi",
            "app/features/account/templates/welcome.en-US",
            "app/features/account/templates/welcome.pt-BR",
            "i18n/common.en-US.json",
            "scripts/seed.sh",
            ".env.example",
            "docker-compose.yml",
            "scripts/bootstrap-storage.sh",
        ] {
            assert!(root.join(relative).is_file(), "missing {relative}");
        }

        // The bootstrap-storage script substitutes `{{app_slug}}` as a
        // bash-default fallback; the `.tmpl` suffix is stripped.
        let bootstrap = fs::read_to_string(root.join("scripts/bootstrap-storage.sh"))
            .expect("read bootstrap-storage.sh");
        assert!(
            bootstrap.contains(":-my_app"),
            "bootstrap-storage.sh should embed app_slug as a default: {bootstrap}"
        );
        let env_example = fs::read_to_string(root.join(".env.example"))
            .expect("read .env.example");
        assert!(
            env_example.contains("S3_ENDPOINT="),
            ".env.example should declare S3_ENDPOINT"
        );
        let compose = fs::read_to_string(root.join("docker-compose.yml"))
            .expect("read docker-compose.yml");
        assert!(
            compose.contains("MINIO_ROOT_USER_FILE: \"\""),
            "docker-compose.yml should clear MinIO _FILE defaults"
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn inspect_expand_rewrites_local_sugars() {
        let source = r#"
feature customer
  purpose "Customers"

  domain
    resource Customer
      name: Text required
      email: @semantic.Email @pii.contact required
      api_key: @cap.Encrypted(key:@key.tenant) optional

    record CustomerLtv
      customer_id: ID
      amount: @semantic.Money

    query.lookup by_id by id: ID

    query.list list
      params
        name: Text optional

      filters
        name when params.name

      paginate 50

    event_group customer_* on Customer
      payload
        customer_id = id
        org_id = org.id

      event created
        email: @semantic.Email

  command create
    input name, email
    policy @policy.create
    creates Customer from input

  command rename
    route id: ID
    input name
    policy @policy.update
    updates Customer
      name = input.name

  workflow lifecycle on Customer.status
    policy @policy.update

    activate: lead -> active requires @policy.delete emits customer_activated
"#;

        let expanded = expand_canonical_source(source);

        assert!(expanded.contains("    query.lookup by_id\n      params\n        id: ID"));
        assert!(expanded.contains("    event customer_created\n      customer_id: ID\n      org_id: ID\n      email: @semantic.Email"));
        assert!(
            expanded.contains(
                "    creates Customer\n      name = input.name\n      email = input.email"
            )
        );
        assert!(
            expanded.contains("    target query.by_id(id: route.id)\n    policy @policy.update")
        );
        assert!(expanded.contains(
            "    activate: lead -> active\n      requires @policy.delete\n      emits customer_activated"
        ));
        assert!(!expanded.contains("event_group customer_* on Customer"));
        assert!(!expanded.contains("from input"));
    }

    #[test]
    fn inspect_json_reports_selected_expansions_with_origin() {
        let source = r#"
feature customer
  purpose "Customers"

  requires integration gateway: PaymentGateway

  refs
    core: @role, @policy, @semantic, @cap, @pii, @key

  defaults
    tenancy org

  domain
    resource Customer
      name: Text required
      email: @semantic.Email @pii.contact required
      api_key: @cap.Encrypted(key:@key.tenant) optional

    record CustomerLtv
      customer_id: ID
      amount: @semantic.Money

    query.lookup by_id by id: ID

    query.list list
      params
        name: Text optional

      filters
        name when params.name

      paginate 50

    event_group customer_* on Customer
      payload
        customer_id = id

      event created
        email: @semantic.Email @pii.contact

  policies
    update: @role.admin

  command rename
    route id: ID
    input name
    policy @policy.update
    idempotency by route.id, input.name
    retry 2 backoff exponential
    calls gateway.rename_customer
      customer_id = route.id
      name = input.name
    timeout "5s"
    updates Customer
      name = input.name
    emits customer_created
"#;
        let mut expansions = ExpandSet::default();
        expansions.events = true;
        expansions.targets = true;
        expansions.policies = true;
        expansions.defaults = true;
        expansions.refs = true;
        expansions.summary = true;
        expansions.locators = true;
        expansions.dependencies = true;
        expansions.security = true;
        expansions.tests = true;

        let report = inspect_canonical_source(source, Path::new("customer.lzi"), expansions);
        let json = serde_json::to_string(&report).unwrap();

        assert!(json.contains("\"schema\":\"lazuli.inspect.v0\""));
        assert!(json.contains("\"requirements\""));
        assert!(json.contains("\"kind\":\"integration\""));
        assert!(json.contains("\"name\":\"gateway\""));
        assert!(json.contains("\"contract\":\"PaymentGateway\""));
        assert!(json.contains("\"external_calls\""));
        assert!(json.contains("\"subject\":\"customer.command.rename\""));
        assert!(json.contains("\"slot\":\"gateway\""));
        assert!(json.contains("\"operation\":\"rename_customer\""));
        assert!(json.contains("\"timeout\":\"5s\""));
        assert!(json.contains("\"retry\":\"2 backoff exponential\""));
        assert!(json.contains("\"idempotency\":\"route.id, input.name\""));
        assert!(json.contains("\"origin\":\"event_group:customer_*\""));
        assert!(json.contains("\"refs\""));
        assert!(json.contains("\"summary\""));
        assert!(json.contains("\"resources\":[\"Customer\"]"));
        assert!(json.contains("\"records\":[\"CustomerLtv\"]"));
        assert!(json.contains("\"provides\""));
        assert!(json.contains("\"types\":[\"Customer\",\"CustomerLtv\"]"));
        assert!(!json.contains("\"missing\""));
        assert!(
            json.contains("\"origin\":\"inferred from local route id and query.lookup by_id\"")
        );
        assert!(json.contains("\"origin\":\"explicit\""));
        assert!(json.contains("\"origin\":\"defaults\""));
        assert!(json.contains("\"name\":\"query_order\""));
        assert!(json.contains("\"name\":\"query_filter_index\""));
        assert!(json.contains("\"value\":\"org, name\""));
        assert!(json.contains("\"origin\":\"language default\""));
        assert!(json.contains("\"locators\""));
        assert!(json.contains("\"name\":\"route.id\""));
        assert!(json.contains("\"name\":\"target\""));
        assert!(json.contains("\"dependencies\""));
        assert!(json.contains("\"kind\":\"emits_event\""));
        assert!(json.contains("\"security\""));
        assert!(json.contains("\"markers\":[\"@pii.contact\""));
        assert!(json.contains("@cap.Encrypted(key:@key.tenant)"));
        assert!(json.contains("\"tests\""));
        assert!(json.contains("\"assertion\":\"permits @role.admin\""));
        assert!(json.contains("\"origin\":\"generated from command policy @policy.update\""));
    }

    #[test]
    fn inspect_json_reports_app_manifest() {
        let source = r#"
app AcmeCRM
  title "Acme CRM"

  uses
    customer

  packs
    customer_import from registry.packs.customer_import

  bindings
    customer.gateway = integrations.crm

  targets
    backend go
    web react

  environments
    local
    production

  urls
    api production "https://api.acme.example"

  env
    server DATABASE_URL: Secret required
    group mailer
      server MAILER_API_KEY: Secret required in production

  integrations
    crm: CRMProvider
      adapter @adapter.crm
      environments production
      credentials platform
        webhook_secret env.CRM_WEBHOOK_SECRET

  capabilities
    database postgres

  architecture
    mode modular_monolith
    service_ready true

  services
    service crm
      owns customer
      exposes
        query customer.query.list

  communication
    internal sync rpc
    propagate actor, tenant

  runtime
    unit api
      serves queries, commands
      healthcheck "/healthz"

  deploy
    migrations before_deploy
    rollback on_failed_healthcheck
"#;

        let report = inspect_canonical_source(source, Path::new("app.lzi"), ExpandSet::default());
        let json = serde_json::to_string(&report).unwrap();

        assert!(json.contains("\"app\""));
        assert!(json.contains("\"name\":\"AcmeCRM\""));
        assert!(json.contains("\"packs\""));
        assert!(json.contains("\"registry.packs.customer_import\""));
        assert!(json.contains("\"bindings\""));
        assert!(json.contains("\"target_feature\":\"customer\""));
        assert!(json.contains("\"source\":\"integrations.crm\""));
        assert!(json.contains("\"environments\":[\"local\",\"production\"]"));
        assert!(json.contains("\"url\":\"https://api.acme.example\""));
        assert!(json.contains("\"DATABASE_URL\""));
        assert!(json.contains("\"group\":\"mailer\""));
        assert!(json.contains("\"MAILER_API_KEY\""));
        assert!(json.contains("\"environments\":[\"production\"]"));
        assert!(json.contains("\"integrations\""));
        assert!(json.contains("\"kind\":\"CRMProvider\""));
        assert!(json.contains("\"adapter_provenance\":\"local\""));
        assert!(json.contains("\"webhook_secret\""));
        assert!(json.contains("\"architecture\""));
        assert!(json.contains("\"mode\":\"modular_monolith\""));
        assert!(json.contains("\"services\""));
        assert!(json.contains("\"communication\""));
        assert!(json.contains("\"runtime\""));
        assert!(json.contains("\"migrations\":\"before_deploy\""));
    }

    #[test]
    fn inspect_expand_caches_projects_feature_level_profiles() {
        // CL.C.3 — `--expand=caches` surfaces every feature-level
        // `cache <name>` profile typed end-to-end (key + ttl literal +
        // optional namespace/tags/SWR/coalesce/sliding). The query's
        // inline `cache` slot keeps its own projection.
        let source = r#"
feature catalog
  cache product_view
    key "product:{product_id}"
    ttl 5m
    namespace catalog
    tags product, listing
    stale_while_revalidate 30s
    coalesce true
    sliding true

  domain
    resource Product
      id: ID required

    query.list list
      cache product_view
"#;
        let mut expansions = ExpandSet::default();
        expansions.caches = true;
        let report = inspect_canonical_source(source, Path::new("catalog.lzi"), expansions);
        let json = serde_json::to_string(&report).unwrap();

        // Expand label surfaces in the report header.
        assert!(
            json.contains("\"expand\":[\"caches\"]"),
            "expected expand label, got {json}"
        );
        // Profile shows up in the `caches` projection.
        assert!(
            json.contains("\"caches\":["),
            "expected caches array, got {json}"
        );
        assert!(
            json.contains("\"name\":\"product_view\""),
            "expected profile name, got {json}"
        );
        assert!(
            json.contains("\"namespace\":\"catalog\""),
            "expected namespace, got {json}"
        );
        assert!(json.contains("\"product\""), "expected tags, got {json}");
        assert!(json.contains("\"listing\""), "expected tags, got {json}");
        assert!(
            json.contains("\"coalesce\":true"),
            "expected coalesce, got {json}"
        );
        assert!(
            json.contains("\"sliding\":true"),
            "expected sliding, got {json}"
        );
    }

    #[test]
    fn inspect_emits_manifest_when_present() {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "lazuli-inspect-manifest-test-{}-{suffix}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();

        let app_path = root.join("app.lzi");
        fs::write(
            &app_path,
            r#"
app Marketplace
  title "Marketplace"
"#,
        )
        .unwrap();
        fs::write(
            root.join("Lazurite.toml"),
            r#"
[project]
name = "marketplace"
module = "github.com/acme/marketplace"
schema = 1

[lazuli]
runtime = "0.1.0"

[plugins]
"@plugin/example/payment-gateway" = { module = "github.com/lazuli-lang/lazuli-plugin-example-payment", version = "v0.2.0" }

[generate.go]
out = "dist/go"
submodule = true
emit_main = true

[frontends.mobile]
target = "expo"
out = "dist/ts-mobile"
audiences = ["buyer", "seller"]

[migrations]
generated = "dist/go/migrations"
manual = "migrations"
strategy = "auto"
"#,
        )
        .unwrap();

        let source = fs::read_to_string(&app_path).unwrap();
        let json =
            inspect_json_value(&source, &app_path, &root, ExpandSet::default(), &[]).unwrap();

        assert_eq!(json["manifest"]["origin"], "Lazurite.toml");
        assert_eq!(json["manifest"]["project"]["name"], "marketplace");
        assert_eq!(
            json["manifest"]["plugins"][0]["ref"],
            "@plugin/example/payment-gateway"
        );
        assert_eq!(json["manifest"]["plugins"][0]["source"], "remote");
        assert_eq!(json["manifest"]["frontends"][0]["name"], "mobile");
        assert_eq!(json["manifest"]["frontends"][0]["target"], "expo");
        assert_eq!(json["manifest"]["migrations"]["strategy"], "auto");
        assert!(!json["ir"].is_null());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn inspect_json_reports_profiles() {
        let source = r#"
profile local
  urls
    web "http://localhost:3000"
  bindings
    customer_import.crm = integrations.crm
  integrations
    crm environment sandbox
    crm adapter @adapter.fake_crm
  deploy
    topology monolith
"#;

        let report =
            inspect_canonical_source(source, Path::new("profiles.lzi"), ExpandSet::default());
        let json = serde_json::to_string(&report).unwrap();

        assert!(json.contains("\"profiles\""));
        assert!(json.contains("\"name\":\"local\""));
        assert!(json.contains("\"target\":\"web\""));
        assert!(json.contains("\"environment\":\"sandbox\""));
        assert!(json.contains("\"adapter\":\"@adapter.fake_crm\""));
        assert!(json.contains("\"adapter_provenance\":\"local\""));
        assert!(json.contains("\"topology\":\"monolith\""));
    }

    #[test]
    fn inspect_json_reports_registry_manifest() {
        let source = r#"
registry
  env
    group mercadopago
      server MERCADOPAGO_ACCESS_TOKEN: Secret required in production
  capabilities
    payment_gateway mercadopago
  packs
    payments from @runtime/payments
      version "0.1.0"
      provides feature payments
      requires integration gateway: PaymentGateway
  integrations
    mercadopago: PaymentGateway
      adapter @runtime/mercadopago
      credentials platform
        access_token env.MERCADOPAGO_ACCESS_TOKEN
"#;

        let report =
            inspect_canonical_source(source, Path::new("registry.lzi"), ExpandSet::default());
        let json = serde_json::to_string(&report).unwrap();

        assert!(json.contains("\"registry\""));
        assert!(json.contains("\"group\":\"mercadopago\""));
        assert!(json.contains("\"packs\""));
        assert!(json.contains("\"@runtime/payments\""));
        assert!(json.contains("\"provides\""));
        assert!(json.contains("\"contract\":\"PaymentGateway\""));
        assert!(json.contains("\"kind\":\"PaymentGateway\""));
        assert!(json.contains("\"adapter_provenance\":\"runtime\""));
        assert!(json.contains("\"access_token\""));
    }

    #[test]
    fn inspect_expand_webhook_events_projects_registry_events() {
        let source = r#"
registry
  webhook_event customer.created
    payload
      customer_id: ID
      email: @semantic.Email
    version 2
    previous_version 1
"#;

        let report = inspect_canonical_source(
            source,
            Path::new("registry.lzi"),
            parse_expand_set("webhook_events").unwrap(),
        );
        let json = serde_json::to_value(&report).unwrap();
        let event = &json["webhook_events"][0];

        assert_eq!(json["expand"][0], "webhook_events");
        assert_eq!(event["name"], "customer.created");
        assert_eq!(event["version"], 2);
        assert_eq!(event["previous_version"], 1);
        assert_eq!(event["payload"][1]["type_text"], "@semantic.Email");
    }

    #[test]
    fn inspect_expand_flags_are_explicit() {
        let expansions = parse_expand_set("events,targets,locators,dependencies,security").unwrap();

        assert!(expansions.events);
        assert!(expansions.targets);
        assert!(expansions.locators);
        assert!(expansions.dependencies);
        assert!(expansions.security);
        assert!(!expansions.tests);
        assert!(parse_expand_set("crud").is_err());
    }

    // CL.C.4 — `--expand=aggregates` projection test (spec wave-c-cl4).
    #[test]
    fn inspect_expand_aggregates_projects_root_contains_invariants() {
        let expansions = parse_expand_set("aggregates").unwrap();
        assert!(expansions.aggregates);

        let source = "
feature billing
  resource Order
    total: Integer required

  resource OrderLine
    amount: Integer required

  aggregate OrderBoundary
    root Order
    contains OrderLine
    invariants
      invariant total_non_negative
        when total >= 0
        message \"order total cannot be negative\"
";
        let report = inspect_canonical_source(
            source,
            Path::new("features/billing/billing.lzi"),
            expansions,
        );
        let json = serde_json::to_string(&report).unwrap();
        assert!(
            json.contains("\"aggregates\":["),
            "expected aggregates projection in JSON: {json}"
        );
        assert!(
            json.contains("\"name\":\"OrderBoundary\""),
            "aggregate name should surface: {json}"
        );
        assert!(
            json.contains("\"root\":\"Order\""),
            "root should surface verbatim: {json}"
        );
        assert!(
            json.contains("\"contains\":[\"OrderLine\"]"),
            "contains list should surface: {json}"
        );
        assert!(
            json.contains("\"name\":\"total_non_negative\""),
            "invariant name should surface: {json}"
        );
        assert!(
            json.contains("\"when\":\"total >= 0\""),
            "predicate text should round-trip: {json}"
        );
        assert!(
            json.contains("\"when_kind\":\"closed\""),
            "closed predicate kind should surface: {json}"
        );
    }

    // -------------------------------------------------------------------------
    // Phase L Tier 4b/4cd — per-axis inspect projections (synth Wave 1 cell 01).
    // Mirrors the aggregates/caches template. Each test exercises one
    // `--expand=<axis>` flag in isolation and asserts the lifted IR slice
    // surfaces verbatim in the JSON projection.
    // -------------------------------------------------------------------------

    #[test]
    fn inspect_expand_commands_projects_lifted_commands_with_rate_limit_and_audit() {
        let expansions = parse_expand_set("commands").unwrap();
        assert!(expansions.commands);

        let source = r#"
feature billing
  domain
    event_group audit_stream on Order

  resource Order
    total: Integer required

  command pay
    route id: ID
    input
      amount: Integer required
    policy @policy.create
    rate_limit "30 per hour per ip"
    audit actor, target.id, input.amount
      emit_to audit_stream
    creates Order
      total = input.amount
    emits order_paid
    invalidates
      query.list
"#;
        let report = inspect_canonical_source(
            source,
            Path::new("features/billing/billing.lzi"),
            expansions,
        );
        let json = serde_json::to_string(&report).unwrap();
        assert!(
            json.contains("\"commands\":["),
            "expected commands projection in JSON: {json}"
        );
        assert!(
            json.contains("\"name\":\"pay\""),
            "command name should surface: {json}"
        );
        assert!(
            json.contains("\"rate_limit\":{\"default\":\"30 per hour per ip\""),
            "rate_limit verbatim: {json}"
        );
        assert!(
            json.contains("\"audit\""),
            "audit spec should surface: {json}"
        );
        assert!(
            json.contains("\"emit_to\":\"audit_stream\""),
            "audit emit_to should surface: {json}"
        );
        assert!(
            json.contains("\"invalidates\""),
            "invalidates list should surface: {json}"
        );
    }

    #[test]
    fn inspect_expand_apis_alias_accepts_api_and_apis_tokens() {
        // Both tokens must populate the same boolean.
        let expansions_plural = parse_expand_set("apis").unwrap();
        assert!(expansions_plural.apis);
        let expansions_singular = parse_expand_set("api").unwrap();
        assert!(expansions_singular.apis);

        let source = r#"
feature billing
  api export
    method GET
    path "/api/billing/export"
    output Text
    policy @scope.public
    handler "./api/export.go"
"#;
        let report = inspect_canonical_source(
            source,
            Path::new("features/billing/billing.lzi"),
            expansions_plural,
        );
        let json = serde_json::to_string(&report).unwrap();
        assert!(
            json.contains("\"apis\":["),
            "expected apis projection in JSON: {json}"
        );
        assert!(
            json.contains("\"name\":\"export\""),
            "api name should surface: {json}"
        );
        assert!(
            json.contains("\"path\":\"/api/billing/export\""),
            "api path should surface: {json}"
        );
        assert!(
            json.contains("\"path\":\"./api/export.go\""),
            "api handler path should surface: {json}"
        );
    }

    #[test]
    fn inspect_expand_resources_projects_lifted_resources() {
        let expansions = parse_expand_set("resources").unwrap();
        assert!(expansions.resources);

        let source = r#"
feature billing
  resource Order
    customer_id: ID required
    total: Integer required
    is_high_value: Boolean derived from total > 1000
    retention 7y then anonymize
"#;
        let report = inspect_canonical_source(
            source,
            Path::new("features/billing/billing.lzi"),
            expansions,
        );
        let json = serde_json::to_string(&report).unwrap();
        assert!(
            json.contains("\"resources\":["),
            "expected resources projection in JSON: {json}"
        );
        assert!(
            json.contains("\"name\":\"Order\""),
            "resource name should surface: {json}"
        );
        assert!(
            json.contains("\"retention\""),
            "retention slot should surface: {json}"
        );
        assert!(
            json.contains("\"derived_from\""),
            "derived_from slot should surface: {json}"
        );
    }

    #[test]
    fn inspect_expand_queries_projects_lifted_queries() {
        let expansions = parse_expand_set("queries").unwrap();
        assert!(expansions.queries);

        let source = r#"
feature billing
  domain
    query.lookup by_id by id: ID

    query.list list
      params
        status: Text optional
"#;
        let report = inspect_canonical_source(
            source,
            Path::new("features/billing/billing.lzi"),
            expansions,
        );
        let json = serde_json::to_string(&report).unwrap();
        assert!(
            json.contains("\"queries\":["),
            "expected queries projection in JSON: {json}"
        );
        assert!(
            json.contains("\"by_id\""),
            "lookup query name should surface: {json}"
        );
    }

    #[test]
    fn inspect_expand_records_projects_lifted_records() {
        let expansions = parse_expand_set("records").unwrap();
        assert!(expansions.records);

        let source = r#"
feature billing
  enum InvoiceStatus
    draft
    issued
    paid

  record InvoiceSummary
    status: InvoiceStatus required discriminator
    amount: Integer required
"#;
        let report = inspect_canonical_source(
            source,
            Path::new("features/billing/billing.lzi"),
            expansions,
        );
        let json = serde_json::to_string(&report).unwrap();
        assert!(
            json.contains("\"records\":["),
            "expected records projection in JSON: {json}"
        );
        assert!(
            json.contains("\"name\":\"InvoiceSummary\""),
            "record name should surface: {json}"
        );
        assert!(
            json.contains("\"discriminator_field\":\"status\""),
            "discriminator_field should surface: {json}"
        );
    }

    // -------------------------------------------------------------------------
    // Cut A — inspect projections (§7.3 snapshot tests)
    // -------------------------------------------------------------------------

    #[test]
    fn inspect_expand_tools_flag_parses() {
        let expansions = parse_expand_set("tools").unwrap();
        assert!(expansions.tools);
        assert!(!expansions.summary);
    }

    #[test]
    fn inspect_expand_tenant_migrations_alias_projects_ir() {
        let source = r#"
feature customer
  defaults
    tenancy org

  domain
    query.lookup by_id by id: ID

  tenant_migration backfill_lifecycle_stage
    target query.by_id
    axis org
    idempotency envelope.tenant_id
    handler "./migrations/backfill_lifecycle_stage.go"
"#;
        let expansions = parse_expand_set("tenant_migrations").unwrap();
        let report = inspect_canonical_source(source, Path::new("customer.lzi"), expansions);
        let projected = report.features[0]
            .tenant_migrations
            .as_ref()
            .expect("tenant migrations projection");
        assert_eq!(projected[0].name, "backfill_lifecycle_stage");
        assert_eq!(projected[0].target.axis, "org");
        assert!(projected[0].target.operation.is_some());
    }

    #[test]
    fn inspect_summary_includes_agent_tools_evals_output_kind() {
        let source = r#"
feature customer
  agent summarize
    input
      customer_id: ID required
    policy @policy.read
    output discriminator Intent
    model @llm.classifier
    temperature 0
    seed 1
    prompt "./p.md"
    tools
      customer.query.lookup.by_id
      @tool.web_search
    evals
      case mentions_status
        requires output contains "active"
"#;
        let report =
            inspect_canonical_source(source, Path::new("customer.lzi"), ExpandSet::default());
        let json = serde_json::to_string(&report).unwrap();

        // Agents are emitted regardless of expansion (always-on field).
        assert!(json.contains("\"name\":\"summarize\""));
        // tools[] now picks up indent-6 entries (canonical block form).
        assert!(
            json.contains("\"tools\":[\"customer.query.lookup.by_id\",\"@tool.web_search\"]"),
            "expected tools list in agent: {json}"
        );
        // evals[] carries the case names.
        assert!(
            json.contains("\"evals\":[\"mentions_status\"]"),
            "expected evals list in agent: {json}"
        );
        // output_kind + output_discriminator surface the discriminator
        // form.
        assert!(
            json.contains("\"output_kind\":\"discriminated_enum\""),
            "expected output_kind discriminated_enum: {json}"
        );
        assert!(
            json.contains("\"output_discriminator\":\"Intent\""),
            "expected output_discriminator Intent: {json}"
        );
        // eval_determinism is `pinned` because temperature 0 + seed 1.
        assert!(
            json.contains("\"eval_determinism\":\"pinned\""),
            "expected eval_determinism pinned: {json}"
        );
    }

    #[test]
    fn inspect_summary_marks_nondeterministic_eval_block() {
        let source = r#"
feature customer
  agent flaky
    input
      message: Text required
    policy @policy.read
    output stream Text
    model @llm.default
    temperature 0.7
    prompt "./p.md"
    evals
      case smoke
        requires output contains "ok"
"#;
        let report =
            inspect_canonical_source(source, Path::new("customer.lzi"), ExpandSet::default());
        let json = serde_json::to_string(&report).unwrap();
        assert!(
            json.contains("\"eval_determinism\":\"nondeterministic\""),
            "expected eval_determinism nondeterministic: {json}"
        );
        assert!(
            json.contains("\"output_kind\":\"stream\""),
            "expected output_kind stream: {json}"
        );
    }

    #[test]
    fn inspect_tools_projection_emits_per_agent_dispatch_graph() {
        let source = r#"
feature customer
  agent triage
    input
      message: Text required
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
    tools
      query.lookup.by_id
      customer.command.archive
      @tool.web_search
"#;
        let mut expansions = ExpandSet::default();
        expansions.tools = true;
        let report = inspect_canonical_source(source, Path::new("customer.lzi"), expansions);
        let json = serde_json::to_string(&report).unwrap();

        // The new --expand=tools projection populates `features[].tools`.
        assert!(
            json.contains("\"agent\":\"triage\""),
            "expected agent entry: {json}"
        );
        // Local query.lookup categorised correctly.
        assert!(
            json.contains("\"reference\":\"query.lookup.by_id\",\"kind\":\"query.lookup\",\"scope\":\"local\",\"derived_effect\":\"read\""),
            "expected local query.lookup binding: {json}"
        );
        // Cross-feature command writes.
        assert!(
            json.contains("\"reference\":\"customer.command.archive\",\"kind\":\"command\",\"scope\":\"cross_feature\",\"derived_effect\":\"write\""),
            "expected cross-feature command binding: {json}"
        );
        // Adapter tool with unknown effect (registry resolves in doctor).
        assert!(
            json.contains("\"reference\":\"@tool.web_search\",\"kind\":\"adapter\",\"scope\":\"adapter\",\"derived_effect\":\"unknown\""),
            "expected adapter binding: {json}"
        );
    }

    #[test]
    fn inspect_expand_events_includes_built_in_trace_events() {
        let source = r#"
feature customer
  agent summarize
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
"#;
        let mut expansions = ExpandSet::default();
        expansions.events = true;
        let report = inspect_canonical_source(source, Path::new("customer.lzi"), expansions);
        let json = serde_json::to_string(&report).unwrap();
        assert!(
            json.contains("\"built_in_trace_events\":[{\"name\":\"agent_run\""),
            "expected built_in_trace_events with agent_run: {json}"
        );
        assert!(
            json.contains("\"fires_per\":\"agent_dispatch\""),
            "expected fires_per agent_dispatch: {json}"
        );
        assert!(
            json.contains("\"name\":\"tokens_total\",\"type\":\"Integer\""),
            "expected canonical payload field tokens_total: {json}"
        );
    }

    #[test]
    fn inspect_built_in_trace_events_omitted_without_events_expand() {
        let source = r#"
feature customer
  agent summarize
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
"#;
        let report =
            inspect_canonical_source(source, Path::new("customer.lzi"), ExpandSet::default());
        let json = serde_json::to_string(&report).unwrap();
        assert!(
            !json.contains("built_in_trace_events"),
            "built_in_trace_events must be omitted without --expand=events: {json}"
        );
    }

    #[test]
    fn inspect_expand_expose_flag_parses() {
        let expansions = parse_expand_set("expose").unwrap();
        assert!(expansions.expose);
        assert!(!expansions.summary);
    }

    #[test]
    fn inspect_summary_includes_agent_expose_http() {
        let source = r#"
feature customer
  agent summarize
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
    expose http
      method POST
      path "/api/customers/:id/summary"
      route id: Customer.ID
"#;
        let report =
            inspect_canonical_source(source, Path::new("customer.lzi"), ExpandSet::default());
        let json = serde_json::to_string(&report).unwrap();
        assert!(
            json.contains("\"expose_http\":{\"method\":\"POST\""),
            "expected expose_http always-on summary: {json}"
        );
        assert!(json.contains("\"path\":\"/api/customers/:id/summary\""));
    }

    #[test]
    fn inspect_expose_projection_emits_unified_route_table() {
        let source = r#"
feature customer
  agent summarize
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
    expose http
      method POST
      path "/api/customers/:id/summary"
      route id: Customer.ID

  api list_customers
    method GET
    path "/api/customers"
    handler "./api/list.go"
"#;
        let mut expansions = ExpandSet::default();
        expansions.expose = true;
        let report = inspect_canonical_source(source, Path::new("customer.lzi"), expansions);
        let json = serde_json::to_string(&report).unwrap();

        assert!(
            json.contains("\"kind\":\"agent\",\"origin\":\"customer.agent.summarize\""),
            "expected agent expose entry: {json}"
        );
        assert!(
            json.contains("\"kind\":\"api\",\"origin\":\"customer.api.list_customers\""),
            "expected api expose entry: {json}"
        );
    }

    #[test]
    fn inspect_expose_projection_omitted_without_expand() {
        let source = r#"
feature customer
  agent summarize
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
    expose http
      method POST
      path "/api/x"
"#;
        let report =
            inspect_canonical_source(source, Path::new("customer.lzi"), ExpandSet::default());
        let json = serde_json::to_string(&report).unwrap();
        assert!(
            !json.contains("\"origin\":\"customer.agent.summarize\""),
            "expose projection must be omitted without --expand=expose: {json}"
        );
    }

    #[test]
    fn inspect_tools_projection_omitted_without_expand() {
        let source = r#"
feature customer
  agent triage
    input
      message: Text required
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
    tools
      query.lookup.by_id
"#;
        let report =
            inspect_canonical_source(source, Path::new("customer.lzi"), ExpandSet::default());
        let json = serde_json::to_string(&report).unwrap();

        // Without --expand=tools the new projection is omitted (skipped
        // by `Option::is_none`). The agent's plain tools list is still
        // emitted as part of the always-on agents block.
        assert!(
            !json.contains("\"reference\":\"query.lookup.by_id\""),
            "tools projection should not appear without --expand=tools: {json}"
        );
        assert!(
            json.contains("\"tools\":[\"query.lookup.by_id\"]"),
            "agent.tools list should still be present: {json}"
        );
    }

    // -------------------------------------------------------------------------
    // Phase L — `--expand=auth` projection coverage
    // -------------------------------------------------------------------------

    #[test]
    fn inspect_auth_projection_emits_full_block() {
        let source = r#"
feature customer_auth
  auth
    identity Customer.email

    password
      algorithm argon2id
      hash @fn.hash_customer_password
      verify @fn.verify_customer_password
      rate_limit "5 per 10 minutes"

    oauth google
      adapter @adapter.google_oauth

    mfa totp
      enroll @fn.enroll_customer_totp
      verify @validator.verify_customer_totp

    sessions
      resource CustomerSession
      ttl "7 days"
      refresh false
"#;
        let mut expansions = ExpandSet::default();
        expansions.auth = true;
        let report = inspect_canonical_source(source, Path::new("customer_auth.lzi"), expansions);
        let json = serde_json::to_value(&report).unwrap();
        let auth = &json["features"][0]["auth"];
        assert!(!auth.is_null(), "auth projection should be present: {json}");
        assert_eq!(auth["origin"]["feature"], "customer_auth");
        assert_eq!(auth["identity"]["field"], "Customer.email");
        assert_eq!(auth["identity"]["resource"], "Customer");
        assert_eq!(auth["identity"]["origin"]["feature"], "customer_auth");
        assert_eq!(auth["password"]["algorithm"], "argon2id");
        assert_eq!(auth["password"]["hash"], "@fn.hash_customer_password");
        assert_eq!(auth["password"]["origin"]["feature"], "customer_auth");
        assert_eq!(auth["mfa"]["method"], "totp");
        assert_eq!(auth["mfa"]["enroll"], "@fn.enroll_customer_totp");
        assert_eq!(auth["mfa"]["verify"], "@validator.verify_customer_totp");
        assert_eq!(auth["mfa"]["origin"]["feature"], "customer_auth");
        assert_eq!(auth["sessions"]["ttl"], "7 days");
        assert_eq!(auth["sessions"]["refresh"], false);
        assert_eq!(auth["sessions"]["origin"]["feature"], "customer_auth");
        assert_eq!(auth["oauth"][0]["provider"], "google");
        assert_eq!(auth["oauth"][0]["origin"]["feature"], "customer_auth");
    }

    #[test]
    fn inspect_auth_projection_omitted_without_expand() {
        let source = r#"
feature customer_auth
  auth
    identity Customer.email
"#;
        let report =
            inspect_canonical_source(source, Path::new("customer_auth.lzi"), ExpandSet::default());
        let json = serde_json::to_string(&report).unwrap();
        assert!(
            !json.contains("\"auth\":{"),
            "auth projection must be absent without --expand=auth: {json}"
        );
    }

    // -------------------------------------------------------------------------
    // Phase L Tier 2 — `--expand=storage` projection coverage
    // -------------------------------------------------------------------------

    #[test]
    fn inspect_storage_projection_emits_resource_field_capability() {
        let source = r#"
feature customer_import
  domain
    resource CustomerImportBatch
      file: @cap.File(max_size:25mb,accept:text/csv) required
      uploaded_by: User required
"#;
        let mut expansions = ExpandSet::default();
        expansions.storage = true;
        let report = inspect_canonical_source(source, Path::new("customer_import.lzi"), expansions);
        let json = serde_json::to_value(&report).unwrap();
        let storage = &json["features"][0]["storage"];
        assert!(
            !storage.is_null(),
            "storage projection should be present: {json}"
        );
        let field = &storage["fields"][0];
        assert_eq!(field["resource"], "CustomerImportBatch");
        assert_eq!(field["field"], "file");
        assert_eq!(
            field["file_capability"]["max_size"]["bytes"],
            25 * 1024 * 1024
        );
        assert_eq!(field["file_capability"]["max_size"]["literal"], "25mb");
        assert_eq!(field["file_capability"]["accept"][0]["family"], "text");
        assert_eq!(field["file_capability"]["accept"][0]["subtype"], "csv");
    }

    #[test]
    fn inspect_storage_projection_emits_api_output_capability() {
        let source = r#"
feature customer
  api customer_export
    method GET
    path "/api/customers/export"
    output @cap.File(max_size:100mb,accept:text/csv,visibility:signed,signed_ttl:1h)
    policy @policy.global_read
    handler "./api/export.go"
"#;
        let mut expansions = ExpandSet::default();
        expansions.storage = true;
        let report = inspect_canonical_source(source, Path::new("customer.lzi"), expansions);
        let json = serde_json::to_value(&report).unwrap();
        let output = &json["features"][0]["storage"]["api_outputs"][0];
        assert_eq!(output["api"], "customer_export");
        assert_eq!(output["file_capability"]["max_size"]["literal"], "100mb");
        assert_eq!(output["file_capability"]["visibility"], "signed");
        assert_eq!(output["file_capability"]["signed_ttl"], "1h");
    }

    #[test]
    fn inspect_storage_projection_omitted_without_expand() {
        let source = r#"
feature customer_import
  domain
    resource CustomerImportBatch
      file: @cap.File(max_size:25mb,accept:text/csv) required
"#;
        let report = inspect_canonical_source(
            source,
            Path::new("customer_import.lzi"),
            ExpandSet::default(),
        );
        let json = serde_json::to_string(&report).unwrap();
        assert!(
            !json.contains("\"storage\":{"),
            "storage projection must be absent without --expand=storage: {json}"
        );
    }

    #[test]
    fn inspect_storage_projection_absent_when_feature_has_no_cap_file() {
        let source = r#"
feature customer
  domain
    resource Customer
      name: Text required
"#;
        let mut expansions = ExpandSet::default();
        expansions.storage = true;
        let report = inspect_canonical_source(source, Path::new("customer.lzi"), expansions);
        let json = serde_json::to_value(&report).unwrap();
        // No @cap.File authored → field omitted entirely.
        assert!(json["features"][0]["storage"].is_null());
    }

    #[test]
    fn inspect_auth_projection_absent_when_feature_has_no_auth() {
        let source = r#"
feature customer
  agent simple
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
"#;
        let mut expansions = ExpandSet::default();
        expansions.auth = true;
        let report = inspect_canonical_source(source, Path::new("customer.lzi"), expansions);
        let json = serde_json::to_value(&report).unwrap();
        // No auth block authored → field omitted (None serialises away).
        assert!(json["features"][0]["auth"].is_null());
    }

    // -------------------------------------------------------------------------
    // Roadmap §1.2 — `--expand=http` projection coverage. The unified
    // `http` slot at the report root surfaces cookie + proxy + limits
    // with `origin` metadata only when the flag is set. The typed
    // blocks still serialize on `app` either way.
    // -------------------------------------------------------------------------

    #[test]
    fn inspect_expand_http_flag_parses() {
        let expansions = parse_expand_set("http").unwrap();
        assert!(expansions.http);
        assert!(!expansions.summary);
    }

    #[test]
    fn inspect_http_projection_surfaces_cookie_proxy_limits_with_flag() {
        let source = r#"
app MyApp
  cookie
    default
      signed true
      secure true
      http_only true
      same_site strict
      max_age "7d"
    session
      same_site lax

  proxy
    trusted 10.0.0.0/8, 172.16.0.0/12
    real_ip_header X-Forwarded-For
    forwarded_proto_header X-Forwarded-Proto

  limits
    body_size "10mb"
    header_size "16kb"
    timeout "30s"
"#;
        let mut expansions = ExpandSet::default();
        expansions.http = true;
        let report = inspect_canonical_source(source, Path::new("app.lzi"), expansions);
        let json = serde_json::to_value(&report).unwrap();
        let http = &json["http"];
        assert!(!http.is_null(), "http projection should be present: {json}");
        assert_eq!(http["origin"]["app"], "MyApp");
        // Cookie block.
        assert_eq!(http["cookie"]["profiles"][0]["name"], "default");
        assert_eq!(http["cookie"]["profiles"][0]["signed"], true);
        assert_eq!(http["cookie"]["profiles"][0]["same_site"], "strict");
        assert_eq!(http["cookie"]["profiles"][0]["max_age"], "7d");
        assert_eq!(http["cookie"]["profiles"][1]["name"], "session");
        assert_eq!(http["cookie"]["profiles"][1]["same_site"], "lax");
        // Proxy block.
        assert_eq!(http["proxy"]["trusted"][0], "10.0.0.0/8");
        assert_eq!(http["proxy"]["trusted"][1], "172.16.0.0/12");
        assert_eq!(http["proxy"]["real_ip_header"], "X-Forwarded-For");
        assert_eq!(http["proxy"]["forwarded_proto_header"], "X-Forwarded-Proto");
        // Limits block.
        assert_eq!(http["limits"]["body_size"], "10mb");
        assert_eq!(http["limits"]["header_size"], "16kb");
        assert_eq!(http["limits"]["timeout"], "30s");
        // Per-block origin envelope.
        assert_eq!(http["cookie"]["origin"]["app"], "MyApp");
        assert_eq!(http["proxy"]["origin"]["app"], "MyApp");
        assert_eq!(http["limits"]["origin"]["app"], "MyApp");
    }

    #[test]
    fn inspect_http_projection_omitted_without_expand() {
        let source = r#"
app MyApp
  cookie
    default
      same_site strict

  limits
    body_size "10mb"
"#;
        let report = inspect_canonical_source(source, Path::new("app.lzi"), ExpandSet::default());
        let json = serde_json::to_string(&report).unwrap();
        // The unified `http` slot at the report root is absent without
        // the flag — Option<Value>::None skips the serde key.
        assert!(
            !json.contains("\"http\":{"),
            "http projection must be absent without --expand=http: {json}"
        );
        // But the typed blocks still serialize on `app`.
        assert!(
            json.contains("\"cookie\":"),
            "cookie still surfaces on AppManifest: {json}"
        );
        assert!(
            json.contains("\"limits\":"),
            "limits still surfaces on AppManifest: {json}"
        );
    }

    // -------------------------------------------------------------------------
    // `--format=lazuli` for `lazuli inspect <symbol>` (next-checklist
    // follow-up from lsp-symbol-origin v0.2; closes the deferred item).
    // -------------------------------------------------------------------------

    #[test]
    fn render_inspect_symbol_lazuli_found_emits_human_readable_one_liner() {
        let output = serde_json::json!({
            "symbol": "Customer",
            "feature": "account",
            "defined_in": {
                "source": "file",
                "file": "features/account/account.lzi",
                "line": 42,
                "column": 3,
                "kind": "resource",
            },
            "imported_via": null,
            "type": "resource",
            "previous_names": [],
        });
        let rendered = render_inspect_symbol_lazuli("Customer", &output);
        assert!(
            rendered.contains("Customer"),
            "rendered should name the symbol:\n{rendered}"
        );
        assert!(
            rendered.contains("account"),
            "rendered should name the feature:\n{rendered}"
        );
        assert!(
            rendered.contains("features/account/account.lzi:42"),
            "rendered should anchor the source location:\n{rendered}"
        );
        assert!(
            rendered.contains("(resource)"),
            "rendered should name the symbol kind:\n{rendered}"
        );
    }

    #[test]
    fn render_inspect_symbol_lazuli_with_previous_names() {
        let output = serde_json::json!({
            "symbol": "Customer",
            "feature": "account",
            "defined_in": {
                "source": "file",
                "file": "x.lzi",
                "line": 10,
                "column": 1,
                "kind": "resource",
            },
            "imported_via": null,
            "type": "resource",
            "previous_names": ["Client", "User"],
        });
        let rendered = render_inspect_symbol_lazuli("Customer", &output);
        assert!(
            rendered.contains("previously:"),
            "rendered should announce previously: trailer:\n{rendered}"
        );
        assert!(
            rendered.contains("Client") && rendered.contains("User"),
            "rendered should list both previous names:\n{rendered}"
        );
    }

    #[test]
    fn render_inspect_symbol_lazuli_not_found_emits_code_and_message() {
        let output = serde_json::json!({
            "error": {
                "code": "SYMBOL_NOT_FOUND",
                "message": "no declaration named `Foo` in any feature of this project",
            }
        });
        let rendered = render_inspect_symbol_lazuli("Foo", &output);
        assert!(
            rendered.starts_with("SYMBOL_NOT_FOUND:"),
            "rendered should lead with the error code:\n{rendered}"
        );
        assert!(
            rendered.contains("Foo"),
            "rendered should echo the missing symbol:\n{rendered}"
        );
    }

    #[test]
    fn render_inspect_symbol_lazuli_ambiguous_lists_candidates() {
        let output = serde_json::json!({
            "error": {
                "code": "AMBIGUOUS_SYMBOL",
                "message": "`Customer` is declared in multiple features",
                "candidates": ["account.Customer", "billing.Customer"],
            }
        });
        let rendered = render_inspect_symbol_lazuli("Customer", &output);
        assert!(
            rendered.contains("AMBIGUOUS_SYMBOL"),
            "rendered should lead with the error code:\n{rendered}"
        );
        assert!(
            rendered.contains("- account.Customer"),
            "rendered should list candidate as bullet:\n{rendered}"
        );
        assert!(
            rendered.contains("- billing.Customer"),
            "rendered should list every candidate:\n{rendered}"
        );
    }
}
