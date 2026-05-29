//! `ManifestError` — the typed error envelope returned by every
//! manifest entry point (`load`, `validate`, parse). Carries enough
//! context for the diagnostic emitter to point authors at the
//! offending file/line.

use std::fmt;

/// Typed error envelope returned by every manifest entry point.
///
/// Concrete I/O / TOML errors keep their source via `Error::source`,
/// while the structured variants name the closed catalog of
/// manifest-shape problems we surface with author-facing context.
#[derive(Debug)]
pub enum ManifestError {
    /// File read / write failure.
    Io(std::io::Error),
    /// TOML deserialization failure.
    Toml(toml::de::Error),
    /// `[lazurite] schema = N` outside the supported range.
    UnsupportedSchema(u32),
    /// A plugin key did not start with the required `@lazuli/plugin-`
    /// prefix.
    InvalidPluginNamespace(String),
    /// Two `[frontends.<name>]` blocks resolved to the same `out`
    /// directory, which would cause codegen to clobber itself.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsupported_schema_renders_with_version() {
        let err = ManifestError::UnsupportedSchema(99);
        assert!(format!("{err}").contains("99"));
    }

    #[test]
    fn invalid_plugin_namespace_renders_key() {
        let err = ManifestError::InvalidPluginNamespace("foo".into());
        assert!(format!("{err}").contains("foo"));
    }
}
