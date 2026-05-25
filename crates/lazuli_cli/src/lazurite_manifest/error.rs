//! `ManifestError` — the typed error envelope returned by every
//! manifest entry point (`load`, `validate`, parse). Carries enough
//! context for the diagnostic emitter to point authors at the
//! offending file/line.

use std::fmt;

#[derive(Debug)]
pub enum ManifestError {
    Io(std::io::Error),
    Toml(toml::de::Error),
    UnsupportedSchema(u32),
    InvalidPluginNamespace(String),
    FrontendOutCollision(String, String),
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
