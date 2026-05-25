use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::fmt;
use std::path::Path;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Manifest {
    pub project: Project,
    pub lazuli: LazuliPin,
    pub lazurite: Option<Lazurite>,
    #[serde(default)]
    pub plugins: BTreeMap<String, Plugin>,
    #[serde(default)]
    pub generate: Generate,
    #[serde(default)]
    pub frontends: BTreeMap<String, Frontend>,
    pub migrations: Option<Migrations>,
    pub seeds: Option<Seeds>,
    pub dev: Option<DevOverrides>,
    /// Wave 0.5 + Wave 6 — optional `[doctor]` block. Carries severity
    /// overrides per category (Wave 0.5) and per-layer coverage thresholds
    /// (Wave 6). Absent on most projects today; when present
    /// `DOCTOR-OVERRIDE-NEEDS-REASON-001` enforces `reason = "..."` on every
    /// severity override entry. See
    /// `docs/proposals/tdd-bdd-first-2026-05-23.md` §Wave 0.5 + Wave 6.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doctor: Option<Doctor>,
    /// Sibling T0-T5 — `[testing]` section consumed by `lazuli test`.
    /// Optional; when absent the runner falls back to conventional
    /// discovery. Schema per
    /// `docs/proposals/lazuli-test-runner-2026-05-24.md` §3.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub testing: Option<Testing>,
}

/// Wave 0.5 + Wave 6 + Wave 3 (rails-style) — `[doctor]` block in `Lazurite.toml`.
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct Doctor {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub test_discipline: Option<TestDisciplineDoctor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coverage: Option<CoverageSection>,
    /// W3 (rails-style-refactor) — `[doctor.internal_hygiene]` block.
    /// Governs `INTERNAL-*` rules that audit the framework's own Rust
    /// source under `lazuli doctor --self`. Mirrors test_discipline shape.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub internal_hygiene: Option<InternalHygieneDoctor>,
}

/// W3 — `[doctor.internal_hygiene]` block.
///
/// Configures the four `INTERNAL-*` rules that audit the framework's
/// Rust source. Under `preset = "tdd-iron-hand"`, every rule fires at
/// `Error` regardless of profile — editorial veto for the framework's
/// own CI. Per-rule overrides via `severity_override` must carry
/// `reason` per `DOCTOR-OVERRIDE-NEEDS-REASON-001`.
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct InternalHygieneDoctor {
    /// Preset name. Parsed by
    /// `lazuli_doctor::internal_hygiene::preset::InternalHygienePreset::parse`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
    /// Per-rule severity overrides keyed by canonical code.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub severity_override: BTreeMap<String, SeverityOverride>,
}

/// Wave 0.5 + Wave 1.5 — `[doctor.test_discipline]` block.
///
/// Wave 1.5 (rails-style-refactor) adds the optional `preset` shortcut.
/// Mirrors `[doctor.coverage].preset` mechanism: a single line sets the
/// severity posture for every TEST-* / DOCTOR-* / MIGRATION-* / RUNTIME-*
/// rule. Values: `tdd-iron-hand` (all error), `tdd-strict` (all warning),
/// `tdd-mature` (per-rule defaults), `off` (all info). Per-rule overrides
/// in `severity_override` still win — preset is the baseline.
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct TestDisciplineDoctor {
    /// Wave 1.5 — preset name. Parsed by
    /// `lazuli_doctor::test_discipline::preset::TestDisciplinePreset::parse`.
    /// `None` means "no preset; defer to profile-derived defaults +
    /// per-rule overrides only".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub severity_override: BTreeMap<String, SeverityOverride>,
}

/// Wave 0.5 — `[doctor.<category>].severity_override.<RULE-CODE>` entry.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct SeverityOverride {
    pub severity: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Wave 6 — `[doctor.coverage]` schema.
///
/// Frente 1 (2026-05-24) adds the optional `preset` shortcut so
/// pilots can opt into the `tdd-strict` / `tdd-mature` / `off`
/// opinionated layer-threshold sets without authoring all six
/// `[doctor.coverage.<layer>]` sub-blocks. Per-layer sub-blocks
/// still override the preset; see
/// `docs/canonical-semantics.md#coverage-presets`.
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct CoverageSection {
    /// Coverage preset name. One of `tdd-strict`, `tdd-mature`,
    /// `off`. Unknown values surface as a doctor error so unknown
    /// presets don't silently degrade into vacuous-pass behavior.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
    #[serde(flatten)]
    pub per_layer: BTreeMap<String, LayerThresholdConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aggregate_method: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy)]
pub struct LayerThresholdConfig {
    pub block_under: u32,
    pub warn_under: u32,
}

/// Sibling T0-T5 — `[testing]` block consumed by `lazuli test`.
#[derive(Debug, Default, Deserialize, Serialize, Clone)]
pub struct Testing {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_layers: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub go: Option<TestingGo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub playwright: Option<TestingPlaywright>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ts: Option<TestingTs>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spec: Option<TestingSpec>,
}

#[derive(Debug, Default, Deserialize, Serialize, Clone)]
pub struct TestingGo {
    #[serde(default)]
    pub flags: Vec<String>,
    #[serde(default)]
    pub coverage: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coverage_out: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package_pattern: Option<String>,
}

#[derive(Debug, Default, Deserialize, Serialize, Clone)]
pub struct TestingPlaywright {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workers: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discovery_root: Option<String>,
    #[serde(default)]
    pub flags: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TestingTs {
    /// Frente 1 — defaults to `"vitest"` when omitted. Pilots that
    /// follow the canonical scaffold need only `[testing.ts]` to opt
    /// in without restating the runner choice. Use `runner = "jest"`
    /// to switch.
    #[serde(default = "default_ts_runner")]
    pub runner: String,
    #[serde(default)]
    pub flags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discovery_root: Option<String>,
    #[serde(default)]
    pub coverage: bool,
}

#[derive(Debug, Default, Deserialize, Serialize, Clone)]
pub struct TestingSpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Project {
    pub name: String,
    pub module: String,
    pub schema: u32,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct LazuliPin {
    pub runtime: String,
    /// Optional path to the Lazuli source checkout, used by codegen
    /// to emit portable `dist/lazurite.vite.mjs` aliases (and other
    /// dev-time runtime path resolution). Absolute or relative to
    /// the project root. When `None`, codegen assumes
    /// `@lazuli/runtime` is installed as an npm package (no alias
    /// needed); when `Some`, codegen emits aliases pointing at the
    /// source tree via `import.meta.url`-relative paths so the
    /// generated file works regardless of where the project is
    /// checked out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Lazurite {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub template: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub template_version: Option<String>,
    /// Optional subdir (relative to project root) holding `app.lzi`,
    /// `design.lzi`, and `registry.lzi`. Defaults to the project root.
    /// Set to `"app"` to group Lazuli sources under an `app/` subdir
    /// alongside the shell/UI code already conventional there.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_dir: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(untagged)]
pub enum Plugin {
    Remote { module: String, version: String },
    Local { path: String },
}

#[derive(Debug, Default, Deserialize, Serialize, Clone)]
pub struct Generate {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub go: Option<GenerateGo>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct GenerateGo {
    #[serde(default = "default_go_out")]
    pub out: String,
    #[serde(default = "default_true")]
    pub gofmt: bool,
    #[serde(default = "default_true")]
    pub strict: bool,
    #[serde(default = "default_true")]
    pub emit_main: bool,
    #[serde(default = "default_true")]
    pub submodule: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dev_replace: Option<String>,
}

/// Frente 1 — canonical defaults for `[generate.go]`. Applied
/// transparently when the block is absent from `Lazurite.toml`, so
/// pilots can omit boilerplate that matches the canonical layout.
impl Default for GenerateGo {
    fn default() -> Self {
        Self {
            out: default_go_out(),
            gofmt: true,
            strict: true,
            emit_main: true,
            submodule: true,
            dev_replace: None,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Frontend {
    pub target: FrontendTarget,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    pub out: String,
    pub audiences: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "kebab-case")]
pub enum FrontendTarget {
    Expo,
    TanstackVite,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Migrations {
    #[serde(default = "default_migrations_generated")]
    pub generated: String,
    #[serde(default = "default_migrations_manual")]
    pub manual: String,
    #[serde(default)]
    pub strategy: MigrationStrategy,
}

/// Frente 1 — canonical defaults for `[migrations]`. Applied
/// transparently when the block is absent.
impl Default for Migrations {
    fn default() -> Self {
        Self {
            generated: default_migrations_generated(),
            manual: default_migrations_manual(),
            strategy: MigrationStrategy::default(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
#[serde(rename_all = "kebab-case")]
pub enum MigrationStrategy {
    #[default]
    Auto,
    Manual,
    CheckOnly,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Seeds {
    #[serde(default = "default_seeds_dir")]
    pub dir: String,
    #[serde(default)]
    pub auto: bool,
}

/// Frente 1 — canonical defaults for `[seeds]`. Applied transparently
/// when the block is absent.
impl Default for Seeds {
    fn default() -> Self {
        Self {
            dir: default_seeds_dir(),
            auto: false,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DevOverrides {
    #[serde(default)]
    pub plugin_paths: BTreeMap<String, String>,
}

#[derive(Debug, Serialize)]
pub struct InspectManifest<'a> {
    pub origin: &'static str,
    pub project: &'a Project,
    pub lazuli: &'a LazuliPin,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lazurite: Option<&'a Lazurite>,
    pub plugins: Vec<InspectPlugin<'a>>,
    pub generate: &'a Generate,
    pub frontends: Vec<InspectFrontend<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub migrations: Option<&'a Migrations>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seeds: Option<&'a Seeds>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dev: Option<&'a DevOverrides>,
}

#[derive(Debug, Serialize)]
pub struct InspectPlugin<'a> {
    #[serde(rename = "ref")]
    pub plugin_ref: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub module: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<&'a str>,
    pub source: &'static str,
}

#[derive(Debug, Serialize)]
pub struct InspectFrontend<'a> {
    pub name: &'a str,
    pub target: &'a FrontendTarget,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<&'a str>,
    pub out: &'a str,
    pub audiences: &'a [String],
}

#[derive(Debug)]
pub enum ManifestError {
    Io(std::io::Error),
    Toml(toml::de::Error),
    UnsupportedSchema(u32),
    InvalidPluginNamespace(String),
    FrontendOutCollision(String, String),
}

/// Canonical manifest filename. Capitalized following the Cargo
/// convention (`Cargo.toml`, `Cargo.lock`) — the capital signals
/// "this is THE manifest file" in a file tree. The lowercase
/// `lazurite.toml` form is also accepted during the migration window;
/// new projects emit the capitalized form.
pub const MANIFEST_FILENAME: &str = "Lazurite.toml";

/// Legacy lowercase filename. Recognized for back-compat with projects
/// scaffolded before the rename (2026-05-15). When found, `load()`
/// reads it transparently; future `lazuli upgrade` step will rename.
pub const LEGACY_MANIFEST_FILENAME: &str = "lazurite.toml";

pub fn load(project_root: &Path) -> Result<Option<Manifest>, ManifestError> {
    // Prefer the canonical capitalized name; fall back to the legacy
    // lowercase form. On filesystems that are case-insensitive (NTFS,
    // APFS default) both `exists()` calls return true for a single
    // file — that's fine, we always read the canonical path first.
    let canonical = project_root.join(MANIFEST_FILENAME);
    let legacy = project_root.join(LEGACY_MANIFEST_FILENAME);
    let path = if canonical.exists() {
        canonical
    } else if legacy.exists() {
        legacy
    } else {
        return Ok(None);
    };

    let contents = std::fs::read_to_string(&path)?;
    let manifest: Manifest = toml::from_str(&contents)?;
    manifest.validate()?;
    Ok(Some(manifest))
}

/// Resolve `<project_root>/<app_dir>/<file>`, where `<app_dir>` comes
/// from `Lazurite.toml`'s `[lazurite] app_dir` field. Falls back to
/// `<project_root>/<file>` when no manifest exists or no `app_dir` is
/// set — preserving the original convention where Lazuli sources lived
/// at the project root.
///
/// Used by source-loaders (generate, inspect, doctor) that need to find
/// `app.lzi`, `design.lzi`, or `registry.lzi` without each callsite
/// re-implementing manifest-aware resolution.
pub fn resolve_in_app_dir(project_root: &Path, file: &str) -> std::path::PathBuf {
    match load(project_root).ok().flatten() {
        Some(manifest) => manifest.app_root(project_root).join(file),
        None => project_root.join(file),
    }
}

impl Manifest {
    pub fn lazuli_runtime_version(&self) -> Option<&str> {
        Some(self.lazuli.runtime.as_str())
    }

    /// Resolved path to the directory containing `app.lzi`, `design.lzi`,
    /// and `registry.lzi`. Returns `project_root.join(app_dir)` when
    /// `[lazurite] app_dir` is set, otherwise `project_root` itself
    /// (backwards-compatible default).
    pub fn app_root(&self, project_root: &Path) -> std::path::PathBuf {
        match self.lazurite.as_ref().and_then(|l| l.app_dir.as_deref()) {
            Some(subdir) => project_root.join(subdir),
            None => project_root.to_path_buf(),
        }
    }

    /// Frente 1 — resolve `[generate.go]` with canonical defaults
    /// applied when the block is omitted. Pilots that follow the
    /// canonical layout can skip the section entirely.
    pub fn generate_go_or_default(&self) -> GenerateGo {
        self.generate.go.clone().unwrap_or_default()
    }

    /// Frente 1 — resolve `[migrations]` with canonical defaults
    /// applied when the block is omitted.
    pub fn migrations_or_default(&self) -> Migrations {
        self.migrations.clone().unwrap_or_default()
    }

    /// Frente 1 — resolve `[seeds]` with canonical defaults applied
    /// when the block is omitted.
    pub fn seeds_or_default(&self) -> Seeds {
        self.seeds.clone().unwrap_or_default()
    }

    /// Frente 1 — resolve effective `[testing.ts]` config. Authored
    /// `[testing.ts]` (or missing fields filled with layout-derived
    /// canonical defaults) > pure layout defaults > `None`.
    ///
    /// Layout-derived defaults (when `detect_frontend_layout` resolves):
    /// - `runner = "vitest"`
    /// - `config  = "<layout>/vite.config.ts"`
    /// - `discovery_root = "<layout>/src"`
    ///
    /// Returns `None` only when the project is neither in the canonical
    /// layout nor declares `[testing.ts]` (back-compat).
    pub fn testing_ts_resolved(&self, project_root: &Path) -> Option<TestingTs> {
        let authored = self.testing.as_ref().and_then(|t| t.ts.as_ref());
        let layout = self.detect_frontend_layout(project_root);
        match (authored, layout) {
            (Some(cfg), layout) => {
                let mut cfg = cfg.clone();
                if cfg.config.is_none() {
                    if let Some(l) = layout.as_ref() {
                        cfg.config = Some(format!("{l}/vite.config.ts"));
                    }
                }
                if cfg.discovery_root.is_none() {
                    if let Some(l) = layout.as_ref() {
                        cfg.discovery_root = Some(format!("{l}/src"));
                    }
                }
                Some(cfg)
            }
            (None, Some(l)) => Some(TestingTs {
                runner: default_ts_runner(),
                flags: Vec::new(),
                config: Some(format!("{l}/vite.config.ts")),
                discovery_root: Some(format!("{l}/src")),
                coverage: false,
            }),
            (None, None) => None,
        }
    }

    /// Frente 1 — resolve effective `[testing.playwright]` config.
    /// Same precedence as `testing_ts_resolved`. Layout-derived
    /// defaults:
    /// - `config = "<layout>/playwright.config.ts"`
    /// - `discovery_root = "<layout>/e2e"`
    /// - `workers = Some(4)`
    pub fn testing_playwright_resolved(&self, project_root: &Path) -> Option<TestingPlaywright> {
        let authored = self.testing.as_ref().and_then(|t| t.playwright.as_ref());
        let layout = self.detect_frontend_layout(project_root);
        match (authored, layout) {
            (Some(cfg), layout) => {
                let mut cfg = cfg.clone();
                if cfg.config.is_none() {
                    if let Some(l) = layout.as_ref() {
                        cfg.config = Some(format!("{l}/playwright.config.ts"));
                    }
                }
                if cfg.discovery_root.is_none() {
                    if let Some(l) = layout.as_ref() {
                        cfg.discovery_root = Some(format!("{l}/e2e"));
                    }
                }
                if cfg.workers.is_none() {
                    cfg.workers = Some(4);
                }
                Some(cfg)
            }
            (None, Some(l)) => Some(TestingPlaywright {
                config: Some(format!("{l}/playwright.config.ts")),
                workers: Some(4),
                project: None,
                discovery_root: Some(format!("{l}/e2e")),
                flags: Vec::new(),
            }),
            (None, None) => None,
        }
    }

    /// Frente 1 — `[testing] default_layers` with canonical default
    /// `["handler_go", "view_extensibility"]` applied when the field
    /// (or the entire `[testing]` block) is missing.
    pub fn testing_default_layers(&self) -> Vec<String> {
        self.testing
            .as_ref()
            .and_then(|t| t.default_layers.clone())
            .unwrap_or_else(|| vec!["handler_go".to_string(), "view_extensibility".to_string()])
    }

    /// Frente 1 — detect the canonical frontend layout. Returns
    /// `Some("app/web")` when the singular scaffold layout is in use,
    /// `Some("app/clients/<name>")` when the multi-client layout is
    /// in use (and exactly one client exists), `None` otherwise.
    ///
    /// Used to apply default `[testing.ts]` / `[testing.playwright]`
    /// config + discovery_root paths so pilots on the canonical layout
    /// don't need to author either block.
    pub fn detect_frontend_layout(&self, project_root: &Path) -> Option<String> {
        let singular = project_root.join("app").join("web");
        if singular.is_dir() {
            return Some("app/web".to_string());
        }
        let clients = project_root.join("app").join("clients");
        if !clients.is_dir() {
            return None;
        }
        // Only auto-detect when exactly one client dir exists; with
        // multiple clients the pilot must spell out which to use.
        let mut entries: Vec<String> = std::fs::read_dir(&clients)
            .ok()?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .filter_map(|e| e.file_name().into_string().ok())
            .collect();
        entries.sort();
        if entries.len() == 1 {
            return Some(format!("app/clients/{}", entries[0]));
        }
        None
    }

    pub fn validate(&self) -> Result<(), ManifestError> {
        if self.project.schema != 1 {
            return Err(ManifestError::UnsupportedSchema(self.project.schema));
        }

        for key in self.plugins.keys() {
            if !key.starts_with("@lazuli/plugin-") {
                return Err(ManifestError::InvalidPluginNamespace(key.clone()));
            }
        }

        let mut seen_outs = HashSet::new();
        for (name, frontend) in &self.frontends {
            if !seen_outs.insert(frontend.out.as_str()) {
                return Err(ManifestError::FrontendOutCollision(
                    name.clone(),
                    frontend.out.clone(),
                ));
            }
        }

        Ok(())
    }

    pub fn inspect_view(&self) -> InspectManifest<'_> {
        InspectManifest {
            origin: MANIFEST_FILENAME,
            project: &self.project,
            lazuli: &self.lazuli,
            lazurite: self.lazurite.as_ref(),
            plugins: self
                .plugins
                .iter()
                .map(|(plugin_ref, plugin)| match plugin {
                    Plugin::Remote { module, version } => InspectPlugin {
                        plugin_ref,
                        module: Some(module),
                        version: Some(version),
                        path: None,
                        source: "remote",
                    },
                    Plugin::Local { path } => InspectPlugin {
                        plugin_ref,
                        module: None,
                        version: None,
                        path: Some(path),
                        source: "local",
                    },
                })
                .collect(),
            generate: &self.generate,
            frontends: self
                .frontends
                .iter()
                .map(|(name, frontend)| InspectFrontend {
                    name,
                    target: &frontend.target,
                    source: frontend.source.as_deref(),
                    out: &frontend.out,
                    audiences: &frontend.audiences,
                })
                .collect(),
            migrations: self.migrations.as_ref(),
            seeds: self.seeds.as_ref(),
            dev: self.dev.as_ref(),
        }
    }
}

impl fmt::Display for ManifestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ManifestError::Io(err) => write!(f, "{err}"),
            ManifestError::Toml(err) => write!(f, "{err}"),
            ManifestError::UnsupportedSchema(schema) => {
                write!(f, "unsupported Lazurite.toml schema version {schema}")
            }
            ManifestError::InvalidPluginNamespace(key) => {
                write!(f, "plugin key `{key}` must start with `@lazuli/plugin-`")
            }
            ManifestError::FrontendOutCollision(name, out) => {
                write!(f, "frontend `{name}` reuses generated output path `{out}`")
            }
        }
    }
}

impl std::error::Error for ManifestError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ManifestError::Io(err) => Some(err),
            ManifestError::Toml(err) => Some(err),
            _ => None,
        }
    }
}

impl From<std::io::Error> for ManifestError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<toml::de::Error> for ManifestError {
    fn from(value: toml::de::Error) -> Self {
        Self::Toml(value)
    }
}

fn default_true() -> bool {
    true
}

fn default_go_out() -> String {
    "dist/go".to_string()
}

fn default_migrations_generated() -> String {
    "dist/go/migrations".to_string()
}

fn default_migrations_manual() -> String {
    "migrations".to_string()
}

fn default_seeds_dir() -> String {
    "seeds".to_string()
}

/// Frente 1 — `[testing.ts] runner` defaults to `"vitest"` since
/// that's the canonical scaffold choice. Pilots opting into Jest
/// must set `runner = "jest"` explicitly.
fn default_ts_runner() -> String {
    "vitest".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_manifest(contents: &str) -> Result<Manifest, ManifestError> {
        let manifest: Manifest = toml::from_str(contents)?;
        manifest.validate()?;
        Ok(manifest)
    }

    #[test]
    fn parse_minimum_manifest() {
        let manifest = parse_manifest(
            r#"
[project]
name = "myapp"
module = "github.com/myorg/myapp"
schema = 1

[lazuli]
runtime = "0.1.0"
"#,
        )
        .unwrap();

        assert_eq!(manifest.project.name, "myapp");
        assert_eq!(manifest.lazuli.runtime, "0.1.0");
        assert!(manifest.lazurite.is_none());
        assert!(manifest.plugins.is_empty());
        assert!(manifest.generate.go.is_none());
        assert!(manifest.frontends.is_empty());
    }

    #[test]
    fn parse_with_frontends() {
        let manifest = parse_manifest(
            r#"
[project]
name = "marketplace"
module = "github.com/acme/marketplace"
schema = 1

[lazuli]
runtime = "0.1.0"

[frontends.mobile]
target = "expo"
out = "dist/ts-mobile"
audiences = ["buyer", "seller"]

[frontends.web-seller]
target = "tanstack-vite"
out = "dist/ts-web-seller"
audiences = ["seller"]

[frontends.admin]
target = "tanstack-vite"
out = "dist/ts-admin"
audiences = ["admin"]
"#,
        )
        .unwrap();

        assert_eq!(manifest.frontends.len(), 3);
        assert!(matches!(
            manifest.frontends["mobile"].target,
            FrontendTarget::Expo
        ));
        assert_eq!(manifest.frontends["web-seller"].out, "dist/ts-web-seller");
    }

    #[test]
    fn reject_unknown_frontend_target() {
        let err = parse_manifest(
            r#"
[project]
name = "myapp"
module = "github.com/myorg/myapp"
schema = 1

[lazuli]
runtime = "0.1.0"

[frontends.mobile]
target = "react-native"
out = "dist/ts-mobile"
audiences = ["traveler"]
"#,
        )
        .unwrap_err();

        assert!(matches!(err, ManifestError::Toml(_)));
    }

    #[test]
    fn reject_non_plugin_namespace() {
        let err = parse_manifest(
            r#"
[project]
name = "myapp"
module = "github.com/myorg/myapp"
schema = 1

[lazuli]
runtime = "0.1.0"

[plugins]
"@runtime/foo" = { module = "github.com/acme/foo", version = "v0.1.0" }
"#,
        )
        .unwrap_err();

        assert!(matches!(err, ManifestError::InvalidPluginNamespace(key) if key == "@runtime/foo"));
    }

    #[test]
    fn reject_unsupported_schema() {
        let err = parse_manifest(
            r#"
[project]
name = "myapp"
module = "github.com/myorg/myapp"
schema = 2

[lazuli]
runtime = "0.1.0"
"#,
        )
        .unwrap_err();

        assert!(matches!(err, ManifestError::UnsupportedSchema(2)));
    }

    #[test]
    fn reject_frontend_out_collision() {
        let err = parse_manifest(
            r#"
[project]
name = "myapp"
module = "github.com/myorg/myapp"
schema = 1

[lazuli]
runtime = "0.1.0"

[frontends.mobile]
target = "expo"
out = "dist/ts"
audiences = ["traveler"]

[frontends.web]
target = "tanstack-vite"
out = "dist/ts"
audiences = ["host"]
"#,
        )
        .unwrap_err();

        assert!(
            matches!(err, ManifestError::FrontendOutCollision(name, out) if name == "web" && out == "dist/ts")
        );
    }

    /// Wave 0.5 — `[doctor.test_discipline]` parses with no
    /// per-rule overrides authored. Most projects will start here.
    #[test]
    fn parse_empty_doctor_block() {
        let manifest = parse_manifest(
            r#"
[project]
name = "myapp"
module = "github.com/myorg/myapp"
schema = 1

[lazuli]
runtime = "0.1.0"

[doctor]
profile = "strict"

[doctor.test_discipline]
"#,
        )
        .unwrap();

        let doctor = manifest.doctor.expect("doctor block parsed");
        assert_eq!(doctor.profile.as_deref(), Some("strict"));
        let td = doctor
            .test_discipline
            .expect("test_discipline block parsed");
        assert!(td.severity_override.is_empty());
    }

    /// Wave 0.5 — per-rule severity overrides with `reason` lift
    /// cleanly. Whether the `reason` is blank or missing is a
    /// `DOCTOR-OVERRIDE-NEEDS-REASON-001` concern, not a parse error.
    #[test]
    fn parse_doctor_severity_override_with_reason() {
        let manifest = parse_manifest(
            r#"
[project]
name = "myapp"
module = "github.com/myorg/myapp"
schema = 1

[lazuli]
runtime = "0.1.0"

[doctor.test_discipline.severity_override.TEST-MISSING-AUTHORED-001]
severity = "warning"
reason = "legacy billing feature; refactor scheduled Q3"

[doctor.test_discipline.severity_override.TEST-PREDICATE-UNCOVERED-001]
severity = "info"
"#,
        )
        .unwrap();

        let td = manifest
            .doctor
            .and_then(|d| d.test_discipline)
            .expect("test_discipline parsed");
        let with_reason = &td.severity_override["TEST-MISSING-AUTHORED-001"];
        assert_eq!(with_reason.severity, "warning");
        assert_eq!(
            with_reason.reason.as_deref(),
            Some("legacy billing feature; refactor scheduled Q3")
        );
        let without_reason = &td.severity_override["TEST-PREDICATE-UNCOVERED-001"];
        assert_eq!(without_reason.severity, "info");
        assert!(without_reason.reason.is_none());
    }

    /// Wave 6 — `[doctor.coverage]` parses per-layer thresholds + the
    /// optional aggregate-method disclosure.
    #[test]
    fn parse_doctor_coverage_section() {
        let manifest = parse_manifest(
            r#"
[project]
name = "myapp"
module = "github.com/myorg/myapp"
schema = 1

[lazuli]
runtime = "0.1.0"

[doctor.coverage]
spec_predicate     = { block_under = 50, warn_under = 80 }
spec_actor_matrix  = { block_under = 70, warn_under = 90 }
aggregate_method   = "weighted-by-construct-count"
"#,
        )
        .unwrap();

        let doctor = manifest.doctor.expect("doctor section present");
        let coverage = doctor.coverage.expect("coverage section present");
        assert_eq!(
            coverage.aggregate_method.as_deref(),
            Some("weighted-by-construct-count")
        );
        let sp = coverage
            .per_layer
            .get("spec_predicate")
            .expect("spec_predicate entry");
        assert_eq!(sp.block_under, 50);
        assert_eq!(sp.warn_under, 80);
        let sa = coverage
            .per_layer
            .get("spec_actor_matrix")
            .expect("spec_actor_matrix entry");
        assert_eq!(sa.block_under, 70);
        assert_eq!(sa.warn_under, 90);
    }

    /// Frente 1 — `[doctor.coverage] preset = "<name>"` parses.
    #[test]
    fn parse_doctor_coverage_with_preset() {
        let manifest = parse_manifest(
            r#"
[project]
name = "myapp"
module = "github.com/myorg/myapp"
schema = 1

[lazuli]
runtime = "0.1.0"

[doctor.coverage]
preset = "tdd-strict"
"#,
        )
        .unwrap();

        let coverage = manifest
            .doctor
            .and_then(|d| d.coverage)
            .expect("coverage section");
        assert_eq!(coverage.preset.as_deref(), Some("tdd-strict"));
        assert!(coverage.per_layer.is_empty());
    }

    /// Frente 1 — preset + per-layer overrides coexist; preset is
    /// captured as a string and individual layers as their own
    /// `LayerThresholdConfig` entries.
    #[test]
    fn parse_doctor_coverage_preset_with_overrides() {
        let manifest = parse_manifest(
            r#"
[project]
name = "myapp"
module = "github.com/myorg/myapp"
schema = 1

[lazuli]
runtime = "0.1.0"

[doctor.coverage]
preset = "tdd-strict"

[doctor.coverage.handler_go]
block_under = 70
warn_under = 80
"#,
        )
        .unwrap();

        let coverage = manifest
            .doctor
            .and_then(|d| d.coverage)
            .expect("coverage section");
        assert_eq!(coverage.preset.as_deref(), Some("tdd-strict"));
        let handler = coverage
            .per_layer
            .get("handler_go")
            .expect("handler_go entry");
        assert_eq!(handler.block_under, 70);
        assert_eq!(handler.warn_under, 80);
    }

    /// Frente 1 — `[generate.go]` defaults apply when block is absent.
    #[test]
    fn generate_go_or_default_when_block_absent() {
        let manifest = parse_manifest(
            r#"
[project]
name = "myapp"
module = "github.com/myorg/myapp"
schema = 1

[lazuli]
runtime = "0.1.0"
"#,
        )
        .unwrap();

        let go = manifest.generate_go_or_default();
        assert_eq!(go.out, "dist/go");
        assert!(go.gofmt);
        assert!(go.strict);
        assert!(go.emit_main);
        assert!(go.submodule);
    }

    /// Frente 1 — partial `[generate.go]` block fills missing fields
    /// with canonical defaults.
    #[test]
    fn generate_go_partial_block_fills_defaults() {
        let manifest = parse_manifest(
            r#"
[project]
name = "myapp"
module = "github.com/myorg/myapp"
schema = 1

[lazuli]
runtime = "0.1.0"

[generate.go]
out = "build/server"
"#,
        )
        .unwrap();

        let go = manifest.generate_go_or_default();
        assert_eq!(go.out, "build/server");
        // Other fields default to the canonical values.
        assert!(go.gofmt);
        assert!(go.strict);
        assert!(go.emit_main);
        assert!(go.submodule);
    }

    /// Frente 1 — `[migrations]` defaults apply when block is absent.
    #[test]
    fn migrations_or_default_when_block_absent() {
        let manifest = parse_manifest(
            r#"
[project]
name = "myapp"
module = "github.com/myorg/myapp"
schema = 1

[lazuli]
runtime = "0.1.0"
"#,
        )
        .unwrap();

        let migrations = manifest.migrations_or_default();
        assert_eq!(migrations.generated, "dist/go/migrations");
        assert_eq!(migrations.manual, "migrations");
        assert!(matches!(migrations.strategy, MigrationStrategy::Auto));
    }

    /// Frente 1 — partial `[migrations]` block fills missing fields
    /// with canonical defaults.
    #[test]
    fn migrations_partial_block_fills_defaults() {
        let manifest = parse_manifest(
            r#"
[project]
name = "myapp"
module = "github.com/myorg/myapp"
schema = 1

[lazuli]
runtime = "0.1.0"

[migrations]
strategy = "manual"
"#,
        )
        .unwrap();

        let migrations = manifest.migrations_or_default();
        assert_eq!(migrations.generated, "dist/go/migrations");
        assert_eq!(migrations.manual, "migrations");
        assert!(matches!(migrations.strategy, MigrationStrategy::Manual));
    }

    /// Frente 1 — `[testing] default_layers` defaults to
    /// `["handler_go", "view_extensibility"]` when missing.
    #[test]
    fn testing_default_layers_when_block_absent() {
        let manifest = parse_manifest(
            r#"
[project]
name = "myapp"
module = "github.com/myorg/myapp"
schema = 1

[lazuli]
runtime = "0.1.0"
"#,
        )
        .unwrap();

        let layers = manifest.testing_default_layers();
        assert_eq!(layers, vec!["handler_go", "view_extensibility"]);
    }

    /// Frente 1 — authored `default_layers` wins over the canonical
    /// default.
    #[test]
    fn testing_default_layers_authored_wins() {
        let manifest = parse_manifest(
            r#"
[project]
name = "myapp"
module = "github.com/myorg/myapp"
schema = 1

[lazuli]
runtime = "0.1.0"

[testing]
default_layers = ["spec_predicate"]
"#,
        )
        .unwrap();

        let layers = manifest.testing_default_layers();
        assert_eq!(layers, vec!["spec_predicate"]);
    }

    /// Frente 1 — `[testing.ts]` runner defaults to `"vitest"`.
    #[test]
    fn testing_ts_runner_defaults_to_vitest() {
        let manifest = parse_manifest(
            r#"
[project]
name = "myapp"
module = "github.com/myorg/myapp"
schema = 1

[lazuli]
runtime = "0.1.0"

[testing.ts]
"#,
        )
        .unwrap();

        let ts = manifest
            .testing
            .as_ref()
            .and_then(|t| t.ts.as_ref())
            .expect("ts block");
        assert_eq!(ts.runner, "vitest");
    }

    /// Frente 1 — layout detection: `app/web/` → singular.
    #[test]
    fn detect_frontend_layout_singular() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("app").join("web")).unwrap();
        let manifest = parse_manifest(
            r#"
[project]
name = "myapp"
module = "github.com/myorg/myapp"
schema = 1

[lazuli]
runtime = "0.1.0"
"#,
        )
        .unwrap();
        assert_eq!(
            manifest.detect_frontend_layout(tmp.path()),
            Some("app/web".to_string())
        );
    }

    /// Frente 1 — layout detection: `app/clients/<sole>/` → plural.
    #[test]
    fn detect_frontend_layout_single_client() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("app").join("clients").join("hostpoint-app"))
            .unwrap();
        let manifest = parse_manifest(
            r#"
[project]
name = "myapp"
module = "github.com/myorg/myapp"
schema = 1

[lazuli]
runtime = "0.1.0"
"#,
        )
        .unwrap();
        assert_eq!(
            manifest.detect_frontend_layout(tmp.path()),
            Some("app/clients/hostpoint-app".to_string())
        );
    }

    /// Frente 1 — multiple clients → no auto-detect (pilot must
    /// spell out the config).
    #[test]
    fn detect_frontend_layout_multiple_clients_ambiguous() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("app").join("clients").join("web")).unwrap();
        std::fs::create_dir_all(tmp.path().join("app").join("clients").join("mobile")).unwrap();
        let manifest = parse_manifest(
            r#"
[project]
name = "myapp"
module = "github.com/myorg/myapp"
schema = 1

[lazuli]
runtime = "0.1.0"
"#,
        )
        .unwrap();
        assert_eq!(manifest.detect_frontend_layout(tmp.path()), None);
    }

    /// Frente 1 — `testing_ts_resolved` returns layout-derived
    /// defaults when block is omitted.
    #[test]
    fn testing_ts_resolved_layout_defaults() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("app").join("web")).unwrap();
        let manifest = parse_manifest(
            r#"
[project]
name = "myapp"
module = "github.com/myorg/myapp"
schema = 1

[lazuli]
runtime = "0.1.0"
"#,
        )
        .unwrap();
        let ts = manifest.testing_ts_resolved(tmp.path()).expect("resolved");
        assert_eq!(ts.runner, "vitest");
        assert_eq!(ts.config.as_deref(), Some("app/web/vite.config.ts"));
        assert_eq!(ts.discovery_root.as_deref(), Some("app/web/src"));
    }

    /// Frente 1 — authored fields win over layout defaults; missing
    /// fields are filled.
    #[test]
    fn testing_ts_resolved_authored_wins_with_layout_fill() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("app").join("web")).unwrap();
        let manifest = parse_manifest(
            r#"
[project]
name = "myapp"
module = "github.com/myorg/myapp"
schema = 1

[lazuli]
runtime = "0.1.0"

[testing.ts]
discovery_root = "custom/src"
"#,
        )
        .unwrap();
        let ts = manifest.testing_ts_resolved(tmp.path()).expect("resolved");
        // discovery_root authored wins.
        assert_eq!(ts.discovery_root.as_deref(), Some("custom/src"));
        // config filled from layout.
        assert_eq!(ts.config.as_deref(), Some("app/web/vite.config.ts"));
    }

    /// Frente 1 — no layout AND no authored block → None
    /// (back-compat skip path for non-canonical projects).
    #[test]
    fn testing_ts_resolved_none_when_neither() {
        let tmp = tempfile::tempdir().unwrap();
        let manifest = parse_manifest(
            r#"
[project]
name = "myapp"
module = "github.com/myorg/myapp"
schema = 1

[lazuli]
runtime = "0.1.0"
"#,
        )
        .unwrap();
        assert!(manifest.testing_ts_resolved(tmp.path()).is_none());
    }

    /// Frente 1 — `testing_playwright_resolved` mirrors the ts
    /// behavior with playwright-specific paths.
    #[test]
    fn testing_playwright_resolved_layout_defaults() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("app").join("web")).unwrap();
        let manifest = parse_manifest(
            r#"
[project]
name = "myapp"
module = "github.com/myorg/myapp"
schema = 1

[lazuli]
runtime = "0.1.0"
"#,
        )
        .unwrap();
        let pw = manifest
            .testing_playwright_resolved(tmp.path())
            .expect("resolved");
        assert_eq!(pw.config.as_deref(), Some("app/web/playwright.config.ts"));
        assert_eq!(pw.discovery_root.as_deref(), Some("app/web/e2e"));
        assert_eq!(pw.workers, Some(4));
    }

    /// Frente 1 — `[seeds]` defaults apply when block is absent.
    #[test]
    fn seeds_or_default_when_block_absent() {
        let manifest = parse_manifest(
            r#"
[project]
name = "myapp"
module = "github.com/myorg/myapp"
schema = 1

[lazuli]
runtime = "0.1.0"
"#,
        )
        .unwrap();

        let seeds = manifest.seeds_or_default();
        assert_eq!(seeds.dir, "seeds");
        assert!(!seeds.auto);
    }

    /// Lazurite.toml rename (2026-05-15) — `load()` must accept both
    /// the canonical capitalized name and the legacy lowercase form.
    /// Cargo-style: new projects emit `Lazurite.toml`, but existing
    /// projects scaffolded before the rename keep working.
    #[test]
    fn loader_accepts_both_canonical_and_legacy_filenames() {
        use std::fs;

        let body = r#"
[project]
name = "casing-test"
module = "github.com/myorg/casing-test"
schema = 1

[lazuli]
runtime = "0.1.0"
"#;

        // Canonical capitalized form.
        let canonical = tempfile::tempdir().unwrap();
        fs::write(canonical.path().join(MANIFEST_FILENAME), body).unwrap();
        let manifest = load(canonical.path()).unwrap().expect("manifest");
        assert_eq!(manifest.project.name, "casing-test");

        // Legacy lowercase form (back-compat for existing projects).
        let legacy = tempfile::tempdir().unwrap();
        fs::write(legacy.path().join(LEGACY_MANIFEST_FILENAME), body).unwrap();
        let manifest = load(legacy.path()).unwrap().expect("manifest");
        assert_eq!(manifest.project.name, "casing-test");
    }
}
