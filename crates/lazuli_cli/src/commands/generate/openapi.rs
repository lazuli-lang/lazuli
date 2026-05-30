//! `lazuli generate openapi` — emit an OpenAPI 3.x YAML spec derived
//! from the typed IR.
//!
//! The IR walk produces a `lazuli_openapi::EmitOptions` and delegates
//! to `lazuli_openapi::emit`, which owns the actual schema generation
//! (path table from `api` blocks, schema table from resources +
//! records, security from `policy` decorators). This module is the
//! thin sink — write to `--out` if provided, otherwise stdout.
//!
//! `--api-version` overrides the `info.version` field; absent that,
//! the emitter falls back to the project's manifest-pinned API
//! version. `strict_typed_only` stays false here because `generate
//! openapi` is the public-spec emit and tolerates incomplete typing —
//! `lazuli doctor` is the gate for typing completeness.
//!
//! Cross-refs:
//! - `lazuli_openapi::emit` / `EmitOptions` — the actual emit.
//! - `crate::build_module_from_path` — the compile entry.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

use crate::build_module_from_path;

/// Handler for `GenerateKind::Openapi`.
///
/// Compiles `input` to IR, runs `lazuli_openapi::emit`, and writes the
/// YAML to `output` (creating the parent dir) or stdout. `api_version`
/// overrides the `info.version` field; `strict_typed_only` stays false
/// because doctor — not this emitter — gates typing completeness.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_cli::commands::generate::openapi::generate_openapi;
///
/// // generate_openapi(Path::new("app.lzi"), None, Some("1.0.0"))?;
/// ```
pub fn generate_openapi(
    input: &Path,
    output: Option<&Path>,
    api_version: Option<&str>,
) -> Result<()> {
    let module = build_module_from_path(input)?;
    let opts = lazuli_openapi::EmitOptions {
        api_version: api_version.map(|s| s.to_owned()),
        strict_typed_only: false,
    };
    let yaml = lazuli_openapi::emit(&module, opts);
    match output {
        Some(path) => {
            if let Some(parent) = path.parent()
                && !parent.as_os_str().is_empty()
            {
                fs::create_dir_all(parent)
                    .with_context(|| format!("creating output directory {}", parent.display()))?;
            }
            fs::write(path, &yaml)
                .with_context(|| format!("writing OpenAPI spec to {}", path.display()))?;
            println!("wrote {}", path.display());
        }
        None => print!("{}", yaml),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_input_errors() {
        let result = generate_openapi(Path::new("__lazuli_no_such_input.lzi"), None, None);
        assert!(result.is_err());
    }
}
