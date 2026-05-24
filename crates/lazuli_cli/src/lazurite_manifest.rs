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
    /// Wave 0.5 + Wave 6 — `[doctor]` section. `None` when the project
    /// did not declare doctor overrides; defaults are derived from the
    /// `--security-profile` flag.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doctor: Option<Doctor>,
}

/// Wave 0.5 (forward-compat) + Wave 6 (`coverage`) `[doctor]` section.
/// Most sub-tables are placeholders Wave 0.5 will hydrate; Wave 6 only
/// owns `coverage` here.
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct Doctor {
    /// `profile = "strict"` etc. Optional; flag wins when both set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    /// Wave 6 — `[doctor.coverage]` per-layer thresholds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coverage: Option<CoverageSection>,
}

/// Wave 6.5 — `[doctor.coverage]` schema. Per-layer entries live as
/// top-level keys (`spec_predicate = { block_under = 50, warn_under
/// = 80 }`); the `aggregate_method` sibling toggles aggregate
/// emission. Unknown layer names are ignored (forward-compat: a
/// future layer added to the catalog will not break old projects).
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct CoverageSection {
    /// Per-layer overrides keyed by canonical layer name
    /// (`spec_predicate`, `spec_actor_matrix`,
    /// `spec_transition_state`, `view_extensibility`,
    /// `view_e2e_pair`, `handler_go`).
    #[serde(flatten)]
    pub per_layer: BTreeMap<String, LayerThresholdConfig>,
    /// Optional aggregate-method disclosure. When set, doctor emits
    /// the `aggregate` field on the coverage report. Values:
    /// `"weighted-by-construct-count"`, `"weighted-by-LOC"`,
    /// `"unweighted-mean"`. Other strings are accepted and surfaced
    /// verbatim — Lazuli does not police aggregate methods because
    /// the gate uses per-layer thresholds only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aggregate_method: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy)]
pub struct LayerThresholdConfig {
    /// CI MUST fail when the layer's coverage is strictly below
    /// `block_under`.
    pub block_under: u32,
    /// CI warns when coverage is strictly below `warn_under` but at
    /// or above `block_under`.
    pub warn_under: u32,
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
    pub generated: String,
    pub manual: String,
    pub strategy: MigrationStrategy,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "kebab-case")]
pub enum MigrationStrategy {
    Auto,
    Manual,
    CheckOnly,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Seeds {
    pub dir: String,
    pub auto: bool,
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

    /// Wave 6 — `[doctor.coverage]` parses per-layer thresholds + the
    /// optional aggregate-method disclosure. Unknown layer names are
    /// preserved verbatim so additions to the layer catalog don't
    /// break old projects.
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
