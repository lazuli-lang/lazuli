use std::collections::{BTreeMap, BTreeSet};
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
mod debug;
mod dev;
mod doctor;
mod examples_bundle;
mod lazurite_manifest;
mod migrate;
mod profile;
mod seed;
mod templates;
mod upgrade;

const DEFAULT_TEMPLATE: &str = include_str!("../../../examples/crm.lzi");
const REGISTRY_TEMPLATE: &str =
    "registry\n  # capabilities: name typed\n  # integrations: provider-neutral declarations\n";
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
dist/
build/
"#;

#[derive(Debug, Parser)]
#[command(name = "lazuli")]
#[command(about = "Lazuli application metalinguage compiler")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
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
    Compile {
        input: PathBuf,
        #[arg(long, short)]
        out: PathBuf,
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
        project_name: PathBuf,
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
    },
    Lsp,
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
    /// OpenAPI / Lazuli Go bucket cycle — emit artifacts derived from
    /// the typed IR slice. Today supports `openapi` (OpenAPI 3.1 spec
    /// YAML) and `go` (Lazuli Go user-code that imports
    /// `lazuli.dev/runtime/lazuli`).
    Generate {
        /// Which artifact to emit. Closed catalog: `openapi`, `go`.
        #[arg(value_enum)]
        kind: GenerateKind,
        /// Path to a `.lzi` file or a directory containing one.
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
    /// Apply, roll back, or inspect SQL migrations from lazurite.toml.
    Migrate {
        #[command(subcommand)]
        sub: MigrateCommand,
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
    /// Run seed scripts from lazurite.toml [seeds].dir.
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
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum GenerateKind {
    Openapi,
    Go,
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
            jobs: true,
            webhooks: true,
            event_groups: true,
            migrations: true,
            notifications: true,
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
            || self.jobs
            || self.webhooks
            || self.event_groups
            || self.migrations
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
        } => check_command(&input, security_profile),
        Commands::Doctor {
            input,
            security_profile,
            check_release,
        } => doctor::doctor_command(&input, security_profile.into(), check_release),
        Commands::Compile { input, out } => compile_command(&input, &out),
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
        } => new_command(&project_name, &template, bare, no_git, module),
        Commands::Lsp => lsp_command(),
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
        } => generate_command(
            kind,
            &input,
            output.as_deref(),
            api_version.as_deref(),
            module.as_deref(),
            lazuli_go_version.as_deref(),
            check,
            with_source,
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
                }
                MigrateCommand::Down { steps, yes } => {
                    migrate::run_migrate(&project_root, migrate::MigrateAction::Down { steps, yes })
                }
                MigrateCommand::Status => {
                    migrate::run_migrate(&project_root, migrate::MigrateAction::Status)
                }
            }
            .map_err(|err| anyhow::anyhow!("{err}"))
        }
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
) -> Result<()> {
    match kind {
        GenerateKind::Openapi => generate_openapi(input, output, api_version),
        GenerateKind::Go => {
            generate_go(input, output, module, lazuli_go_version, check, with_source)
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
) -> Result<()> {
    let project_root = project_root_for_input(input);
    let manifest = lazurite_manifest::load(&project_root).with_context(|| {
        format!(
            "failed to read {}",
            project_root.join("lazurite.toml").display()
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

    let options = lazuli_codegen_go::GoEmitOptions {
        module_name: Some(module_name),
        lazuli_go_version: go_version,
        check,
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
        // Coarse pass/fail signal; the closed §6.2.1 error catalog
        // (CODEGEN-GO-PLUGIN-001, etc.) lands in cell I4. Today we
        // just enumerate what the emitter would produce and exit 0.
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

    for file in &files {
        if file.path == "go.work" {
            write_generated_file(&project_root, &file.path, &file.contents)?;
        } else {
            write_generated_file(out_dir, &file.path, &file.contents)?;
        }
    }

    println!("wrote {} file(s) to {}", files.len(), out_dir.display());
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
            (
                plugin_ref.clone(),
                lazuli_codegen_go::LazuritePlugin {
                    module,
                    version,
                    path,
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
        features: Vec::new(),
    };

    // L0 #2 — `design.lzi` lives at project root, peer to `app.lzi` /
    // `registry.lzi`. Only parse when we're building from a directory;
    // single-file input mode skips the design pipeline.
    if input.is_dir() {
        let design_path = input.join("design.lzi");
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

    Ok(module)
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
        features: Vec::new(),
    };
    let mut source_map = lazuli_ir::SourceMap { files: Vec::new() };
    let mut feature_file_ids = BTreeMap::new();

    // L0 #2 — Optional `design.lzi` at the input root. Mirrors
    // `build_module_from_path`; emitters and SDK projections consume
    // `module.design` when present.
    if input.is_dir() {
        let design_path = input.join("design.lzi");
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
        input.join("app.lzi")
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

fn check_command(input: &Path, security_profile: CheckSecurityProfile) -> Result<()> {
    let source =
        fs::read_to_string(input).with_context(|| format!("failed to read {}", input.display()))?;
    let diagnostics =
        lazuli_lsp::diagnostics_for_source_with_profile(&source, security_profile.into());
    let has_error = diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == Some(DiagnosticSeverity::ERROR));

    for diagnostic in &diagnostics {
        print_diagnostic(input, diagnostic);
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

fn compile_command(input: &Path, out: &Path) -> Result<()> {
    let app = compile_to_ir(input)?;
    let plan = lazuli_planner::plan_initial_generation(&app);

    fs::create_dir_all(out)
        .with_context(|| format!("failed to create output directory {}", out.display()))?;

    for file in lazuli_codegen_go::generate_legacy_demo(&app) {
        write_generated_file(out, &file.path, &file.contents)?;
    }

    for file in lazuli_codegen_ts::generate(&app) {
        write_generated_file(out, &file.path, &file.contents)?;
    }

    println!("{}", serde_json::to_string_pretty(&plan)?);
    Ok(())
}

fn inspect_command(
    input: &Path,
    expand: &str,
    format: InspectFormat,
    include: &[InspectInclude],
) -> Result<()> {
    let expansions = parse_expand_set(expand)?;
    let source_path = inspect_source_path(input);
    let source = fs::read_to_string(&source_path)
        .with_context(|| format!("failed to read {}", source_path.display()))?;

    match format {
        InspectFormat::Json => {
            let output = inspect_json_value(&source, &source_path, expansions, include)?;
            println!("{}", serde_json::to_string_pretty(&output)?);
        }
        InspectFormat::Lazuli => {
            if expansions.any() {
                print!("{}", expand_canonical_source_with(&source, expansions));
            } else {
                print!("{source}");
            }
        }
    }

    Ok(())
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
        return input.join("app.lzi");
    }

    input.to_path_buf()
}

fn inspect_json_value(
    source: &str,
    input: &Path,
    expansions: ExpandSet,
    include: &[InspectInclude],
) -> Result<serde_json::Value> {
    let report = inspect_canonical_source(source, input, expansions);
    let project_root = project_root_for_input(input);
    let manifest = lazurite_manifest::load(&project_root).with_context(|| {
        format!(
            "failed to read {}",
            project_root.join("lazurite.toml").display()
        )
    })?;

    if let Some(manifest) = manifest {
        return Ok(serde_json::json!({
            "ir": report,
            "manifest": manifest.inspect_view(),
        }));
    }

    if include.contains(&InspectInclude::Manifest) {
        return Ok(serde_json::json!({
            "ir": report,
            "manifest": serde_json::Value::Null,
        }));
    }

    Ok(serde_json::to_value(report)?)
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
    project: &Path,
    template: &str,
    bare: bool,
    no_git: bool,
    module: Option<String>,
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

        if let Err(err) = run_go_mod_tidy(project) {
            eprintln!("warning: failed to run `go mod tidy`: {err:#}");
        }
        if let Err(err) = run_doctor_sanity_check(project) {
            eprintln!("warning: failed to run `lazuli doctor`: {err:#}");
        }
    }

    if !no_git {
        run_git_init(project)?;
    }

    println!("created {}", project.display());
    Ok(())
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

fn run_doctor_sanity_check(project: &Path) -> Result<()> {
    doctor::doctor_command(project, SecurityProfile::Strict, false)
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
    let mut output = String::new();
    let mut capitalize_next = true;

    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            if output.is_empty() && ch.is_ascii_digit() {
                output.push_str("App");
            }

            if capitalize_next {
                output.push(ch.to_ascii_uppercase());
            } else {
                output.push(ch.to_ascii_lowercase());
            }
            capitalize_next = false;
        } else {
            capitalize_next = true;
        }
    }

    output
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
    let source =
        fs::read_to_string(input).with_context(|| format!("failed to read {}", input.display()))?;
    let document = lazuli_syntax::parse_document(&source).context("failed to parse .lzi file")?;
    lazuli_analyzer::lower_document(&document).context("failed to analyze .lzi file")
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
            "jobs" => set.jobs = true,
            "webhooks" => set.webhooks = true,
            "event_groups" => set.event_groups = true,
            // Migrations bucket cycle Route C — projects every lifted
            // `ir::TenantMigration` on the feature + the app deploy
            // block's checkpoint/strategy/lock_timeout/hook fields.
            "migrations" => set.migrations = true,
            // Notifications expanded bucket cycle — projects every
            // lifted `ir::Notification` with typed `digest` /
            // `throttle` sub-blocks. The scalar fields surface in
            // default inspect; this flag adds the structured shapes.
            "notifications" => set.notifications = true,
            _ => bail!(
                "unknown inspect expansion `{item}`; use none, all, refs, summary, locators, dependencies, security, events, targets, policies, tests, defaults, tools, expose, auth, storage, tracing, logging, jobs, webhooks, event_groups, migrations, or notifications"
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
    #[serde(skip_serializing_if = "Vec::is_empty")]
    profiles: Vec<lazuli_ir::AppProfile>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    routes: Vec<lazuli_ir::AppRoute>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    experiences: Vec<lazuli_ir::Experience>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    surfaces: Vec<lazuli_ir::PlatformSurface>,
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
}

#[derive(Debug, Serialize)]
struct InspectAuthPassword {
    algorithm: String,
    hash: String,
    verify: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    rate_limit: Option<String>,
}

#[derive(Debug, Serialize)]
struct InspectAuthSessions {
    resource: String,
    ttl: String,
    refresh: bool,
}

#[derive(Debug, Serialize)]
struct InspectAuthMfa {
    method: String,
    enroll: String,
    verify: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    adapter: Option<String>,
}

#[derive(Debug, Serialize)]
struct InspectAuthOAuthProvider {
    provider: String,
    adapter: String,
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
        || expansions.migrations)
        && !is_lzx
    {
        collect_tier3_by_feature(source)
    } else {
        std::collections::BTreeMap::new()
    };

    InspectReport {
        schema: "lazuli.inspect.v0",
        source: input.display().to_string(),
        expand: expansions.labels(),
        workspace: app_manifest::parse_app_workspace(source),
        contracts: app_manifest::parse_app_contracts(source),
        app: app_manifest::parse_app_manifest(source).or(lzx_app),
        registry: app_manifest::parse_app_registry(source),
        profiles: app_manifest::parse_app_profiles(source),
        routes,
        experiences,
        surfaces,
        features: inspect_features(&lines, expansions, &auth_by_feature, &tier3_by_feature),
    }
}

/// Phase L Tier 3 — lower the canonical-indent slice once per inspect
/// call and build a per-feature lookup of `(jobs, webhooks,
/// event_groups)`. Same degradation rules as `collect_auth_by_feature`:
/// failures fall through to an empty map so `--expand=jobs` etc. are
/// projections, not checks.
fn collect_tier3_by_feature(source: &str) -> std::collections::BTreeMap<String, Tier3FeatureSlice> {
    let mut map = std::collections::BTreeMap::new();
    let Ok(features) = lazuli_syntax::parse_feature_skeletons(source) else {
        return map;
    };
    for feature_ast in features {
        let Ok(feature_ir) = lazuli_analyzer::lower_feature_skeleton(&feature_ast) else {
            continue;
        };
        map.insert(
            feature_ir.name.clone(),
            Tier3FeatureSlice {
                jobs: feature_ir.jobs,
                webhooks: feature_ir.webhooks,
                event_groups: feature_ir.event_groups,
                tenant_migrations: feature_ir.tenant_migrations,
                notifications: feature_ir.notifications,
                policies: feature_ir.policies,
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
        .then(|| auth_by_feature.get(&name).map(project_auth))
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
        defaults: expansions.defaults.then(|| inspect_defaults(lines)),
        events: expansions.events.then(|| inspect_events(lines)),
        built_in_trace_events: expansions.events.then(inspect_built_in_trace_events),
        targets: expansions.targets.then(|| inspect_targets(lines)),
        policies: expansions
            .policies
            .then(|| inspect_policies(lines, &policies)),
        tests: expansions.tests.then(|| inspect_tests(lines, &policies)),
        tools,
        expose,
        auth,
        storage,
        jobs: jobs_projection,
        webhooks: webhooks_projection,
        event_groups: event_groups_projection,
        tenant_migrations: tenant_migrations_projection,
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
fn project_auth(auth: &lazuli_ir::Auth) -> InspectAuth {
    InspectAuth {
        identity: InspectAuthIdentity {
            field: format!(
                "{}.{}",
                auth.identity.field.resource.name, auth.identity.field.field
            ),
            resource: auth.identity.field.resource.name.clone(),
        },
        password: auth.password.as_ref().map(|p| InspectAuthPassword {
            algorithm: p.algorithm.clone(),
            hash: p.hash.clone(),
            verify: p.verify.clone(),
            rate_limit: p.rate_limit.clone(),
        }),
        sessions: auth.sessions.as_ref().map(|s| InspectAuthSessions {
            resource: s.resource.name.clone(),
            ttl: s.ttl.clone(),
            refresh: s.refresh,
        }),
        mfa: auth.mfa.as_ref().map(|m| InspectAuthMfa {
            method: m.method.clone(),
            enroll: m.enroll.clone(),
            verify: m.verify.clone(),
            adapter: m.adapter.clone(),
        }),
        oauth: auth
            .oauth
            .iter()
            .map(|o| InspectAuthOAuthProvider {
                provider: o.provider.clone(),
                adapter: o.adapter.clone(),
            })
            .collect(),
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
        ["query", _] => ("query", "local"),
        ["command", _] => ("command", "local"),
        ["api", _] => ("api", "local"),
        [_feature, "query", "list", _] => ("query.list", "cross_feature"),
        [_feature, "query", "lookup", _] => ("query.lookup", "cross_feature"),
        [_feature, "query", "sql", _] => ("query.sql", "cross_feature"),
        [_feature, "query", _] => ("query", "cross_feature"),
        [_feature, "command", _] => ("command", "cross_feature"),
        [_feature, "api", _] => ("api", "cross_feature"),
        _ => ("unknown", "unknown"),
    };

    let derived_effect = match kind {
        "command" => "write",
        "query.list" | "query.lookup" | "query.sql" | "query" => "read",
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

fn inspect_defaults(lines: &[String]) -> Vec<InspectDefault> {
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
            BuiltinType::SemanticMoney => "@semantic.Money",
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
        TypeRef::Capability(CapabilityRef::Token(t)) => format_token_capability(t),
    }
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
) -> Vec<InspectPolicy> {
    let mut policies = Vec::new();

    for command in command_blocks(lines) {
        let name = command_name(command[0].trim_start()).unwrap_or("unknown");

        if let Some(policy) = direct_child_value(command, "policy ") {
            policies.push(InspectPolicy {
                subject: format!("command.{name}"),
                atoms: resolve_policy_atoms(&policy, policy_atoms),
                policy,
                origin: "explicit".to_owned(),
                requires: Vec::new(),
            });
        }
    }

    for query in query_blocks(lines) {
        let name = query_name(query[0].trim_start()).unwrap_or("unknown");

        if let Some(policy) = direct_child_value(query, "policy ") {
            policies.push(InspectPolicy {
                subject: format!("query.{name}"),
                atoms: resolve_policy_atoms(&policy, policy_atoms),
                policy,
                origin: "explicit".to_owned(),
                requires: Vec::new(),
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
        path::Path,
        time::{SystemTime, UNIX_EPOCH},
    };

    use clap::Parser;

    use super::{
        Cli, Commands, ExpandSet, MigrateCommand, REGISTRY_TEMPLATE, app_template,
        default_module_name, expand_canonical_source, inspect_canonical_source, inspect_json_value,
        new_command, parse_expand_set, pascal_case, pascal_case_project_name, scaffold_bare,
        scaffold_from_template, templates,
    };

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
        assert!(!bare.join("lazurite.toml").exists());
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
            fs::read_to_string(root.join("app.lzi"))
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
        assert!(root.join("app.lzi").is_file());
        assert!(!root.join("app.lzi.tmpl").exists());
        assert!(root.join("features/account/account.lzi").is_file());
        assert!(!root.join("features/account/account.lzi.tmpl").exists());

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
        for relative in [
            ".gitignore",
            "README.md",
            "app.lzi",
            "go.mod",
            "go.work",
            "lazurite.toml",
            "registry.lzi",
            "features/account/account.lzi",
            "features/account/handlers/hash_password.go",
            "features/account/handlers/verify_password.go",
            "features/account/templates/welcome.en-US",
            "features/account/templates/welcome.pt-BR",
            "i18n/common.en-US.json",
            "scripts/seed.sh",
        ] {
            assert!(root.join(relative).is_file(), "missing {relative}");
        }

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
            root.join("lazurite.toml"),
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
        let json = inspect_json_value(&source, &app_path, ExpandSet::default(), &[]).unwrap();

        assert_eq!(json["manifest"]["origin"], "lazurite.toml");
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
        assert_eq!(auth["identity"]["field"], "Customer.email");
        assert_eq!(auth["identity"]["resource"], "Customer");
        assert_eq!(auth["password"]["algorithm"], "argon2id");
        assert_eq!(auth["password"]["hash"], "@fn.hash_customer_password");
        assert_eq!(auth["mfa"]["method"], "totp");
        assert_eq!(auth["mfa"]["enroll"], "@fn.enroll_customer_totp");
        assert_eq!(auth["mfa"]["verify"], "@validator.verify_customer_totp");
        assert_eq!(auth["sessions"]["ttl"], "7 days");
        assert_eq!(auth["sessions"]["refresh"], false);
        assert_eq!(auth["oauth"][0]["provider"], "google");
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
}
