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
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Lazurite {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub template: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub template_version: Option<String>,
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
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Frontend {
    pub target: FrontendTarget,
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

pub fn load(project_root: &Path) -> Result<Option<Manifest>, ManifestError> {
    let path = project_root.join("lazurite.toml");
    if !path.exists() {
        return Ok(None);
    }

    let contents = std::fs::read_to_string(&path)?;
    let manifest: Manifest = toml::from_str(&contents)?;
    manifest.validate()?;
    Ok(Some(manifest))
}

impl Manifest {
    pub fn validate(&self) -> Result<(), ManifestError> {
        if self.project.schema != 1 {
            return Err(ManifestError::UnsupportedSchema(self.project.schema));
        }

        for key in self.plugins.keys() {
            if !key.starts_with("@plugin/") {
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
            origin: "lazurite.toml",
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
                write!(f, "unsupported lazurite.toml schema version {schema}")
            }
            ManifestError::InvalidPluginNamespace(key) => {
                write!(f, "plugin key `{key}` must start with `@plugin/`")
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
name = "hostpoint"
module = "github.com/acme/hostpoint"
schema = 1

[lazuli]
runtime = "0.1.0"

[frontends.mobile]
target = "expo"
out = "dist/ts-mobile"
audiences = ["traveler", "host"]

[frontends.web-host]
target = "tanstack-vite"
out = "dist/ts-web-host"
audiences = ["host"]

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
        assert_eq!(manifest.frontends["web-host"].out, "dist/ts-web-host");
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
}
