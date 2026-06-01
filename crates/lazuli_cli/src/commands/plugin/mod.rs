//! `lazuli plugin new` — offline, in-compiler plugin scaffolder (spec 0023).
//!
//! Brings plugin authoring into the binary every author already has. Today
//! authoring is a 583-line prose doc plus an out-of-repo ops pipeline; this
//! command emits a working skeleton — typed manifest (deserializes against
//! the 0021 `lazuli_manifest::PluginManifest` schema) + Go validator/adapter
//! + paired `_test.go` + `go.mod`/`README`/`CHANGELOG`/`LICENSE` — that a
//! `lazuli plugin verify` (spec 0022, sibling) would pass with zero edits.
//!
//! Deliberately DUMB (ADR Decision 1): two embedded template trees
//! (`templates/plugin/{semantic,adapter}`) walked + string-substituted,
//! mirroring `commands::new::scaffold::scaffold_from_template`. No codegen,
//! no IR introspection — the same machinery `lazuli new` uses for projects.
//!
//! The sibling `plugin verify` verb (spec 0022) dispatches from the same
//! `PluginCommand` enum; see `cli_args::PluginCommand`.

use std::path::Path;

use anyhow::{Context, Result, bail};
use include_dir::{Dir, DirEntry, include_dir};

use crate::casing::pascal_case;
use crate::cli_args::PluginKind;

/// `lazuli plugin verify` (spec 0022) — end-to-end wiring proof.
pub mod verify;

/// Embedded semantic template tree (mirrors `lazuli-plugin-scalars-br`,
/// minus the TS/npm publishing surface).
static SEMANTIC_TEMPLATE: Dir<'static> =
    include_dir!("$CARGO_MANIFEST_DIR/templates/plugin/semantic");

/// Embedded adapter template tree (mirrors `lazuli-plugin-mercadopago`,
/// self-contained + dependency-free so the skeleton compiles offline).
static ADAPTER_TEMPLATE: Dir<'static> =
    include_dir!("$CARGO_MANIFEST_DIR/templates/plugin/adapter");

/// The frozen substitution token set (ADR Decision 1). Built once per
/// scaffold and applied to every `.tmpl` file's contents and path.
struct Tokens {
    /// `<name>` arg, kebab-validated (e.g. `my-scalars`).
    name: String,
    /// `<name>` with separators stripped + lowercased — a valid Go package
    /// identifier (e.g. `myscalars`).
    go_package: String,
    /// Go module path (e.g. `lazuli.dev/plugin/my-scalars`).
    module: String,
    /// npm-style namespace (e.g. `@lazuli/plugin-my-scalars`).
    namespace: String,
    /// PascalCase of `<name>`'s last segment (e.g. `MyScalars`).
    type_name: String,
    /// Scaffold date `YYYY-MM-DD` for the CHANGELOG entry.
    date: String,
    /// Scaffold year for the LICENSE copyright line.
    date_year: String,
}

impl Tokens {
    /// Substitute every token in `input`. Used for both file contents and
    /// the `__name__` filename placeholder.
    fn apply(&self, input: &str) -> String {
        input
            .replace("{{name}}", &self.name)
            .replace("{{go_package}}", &self.go_package)
            .replace("{{module}}", &self.module)
            .replace("{{namespace}}", &self.namespace)
            .replace("{{TypeName}}", &self.type_name)
            .replace("{{date_year}}", &self.date_year)
            .replace("{{date}}", &self.date)
    }
}

/// Handler for `lazuli plugin new <name> --kind <semantic|adapter>`.
///
/// Validates `name` (kebab-case) and rejects a non-empty `out` BEFORE any
/// write, then walks the chosen template tree and writes each file with
/// token substitution. The substitution is the only transform — templates
/// are static, derived 1:1 from the two shipped reference plugins.
pub(crate) fn plugin_new_command(
    name: &str,
    kind: PluginKind,
    namespace: Option<String>,
    out: Option<&Path>,
) -> Result<()> {
    validate_kebab_name(name)?;

    let target = match out {
        Some(dir) => dir.to_path_buf(),
        None => std::path::PathBuf::from(name),
    };
    reject_non_empty_dir(&target)?;

    let tokens = build_tokens(name, namespace);
    let template = match kind {
        PluginKind::Semantic => &SEMANTIC_TEMPLATE,
        PluginKind::Adapter => &ADAPTER_TEMPLATE,
    };

    std::fs::create_dir_all(&target)
        .with_context(|| format!("failed to create directory {}", target.display()))?;
    write_template_tree(template, &target, &tokens)?;

    println!("created plugin {} ({:?}) at {}", name, kind, target.display());
    Ok(())
}

/// Build the frozen token map from the `<name>` arg + optional namespace.
fn build_tokens(name: &str, namespace: Option<String>) -> Tokens {
    let go_package = name.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>();
    let type_name = pascal_case(name.rsplit('-').next().unwrap_or(name));
    let namespace = namespace.unwrap_or_else(|| format!("@lazuli/plugin-{name}"));
    // Scaffold date — UTC, derived without pulling a date crate. The Rust
    // test env has no clock guarantee beyond `SystemTime`; format manually.
    let (date, date_year) = scaffold_date();
    Tokens {
        name: name.to_string(),
        go_package,
        module: format!("lazuli.dev/plugin/{name}"),
        namespace,
        type_name,
        date,
        date_year,
    }
}

/// Walk an embedded template `Dir`, substituting tokens in both file
/// contents and the `__name__` filename placeholder, stripping the `.tmpl`
/// suffix on output. Recursive on subdirectories (e.g. `.github/workflows`).
fn write_template_tree(template: &Dir<'_>, target: &Path, tokens: &Tokens) -> Result<()> {
    for entry in template.entries() {
        match entry {
            DirEntry::File(file) => {
                let rel = rendered_relative_path(file.path(), tokens);
                let out_path = target.join(&rel);
                let contents = file.contents_utf8().with_context(|| {
                    format!("template file is not valid UTF-8: {}", file.path().display())
                })?;
                let rendered = tokens.apply(contents);
                if let Some(parent) = out_path.parent() {
                    std::fs::create_dir_all(parent).with_context(|| {
                        format!("failed to create directory {}", parent.display())
                    })?;
                }
                std::fs::write(&out_path, rendered)
                    .with_context(|| format!("failed to write {}", out_path.display()))?;
            }
            DirEntry::Dir(dir) => {
                let rel = rendered_relative_path(dir.path(), tokens);
                let out_path = target.join(&rel);
                std::fs::create_dir_all(&out_path).with_context(|| {
                    format!("failed to create directory {}", out_path.display())
                })?;
                write_template_tree(dir, target, tokens)?;
            }
        }
    }
    Ok(())
}

/// Render a template-relative path into its output form: substitute the
/// `__name__` filename token and strip a trailing `.tmpl` extension.
fn rendered_relative_path(path: &Path, tokens: &Tokens) -> std::path::PathBuf {
    let rendered = path.to_string_lossy().replace("__name__", &tokens.name);
    let mut buf = std::path::PathBuf::from(rendered);
    if buf.extension().and_then(|e| e.to_str()) == Some("tmpl") {
        buf.set_extension("");
    }
    buf
}

/// Validate the plugin name is kebab-case (lowercase a–z, 0–9, '-'; no
/// leading/trailing/double dash). Anchored error; nothing is written when
/// this fails (checked before any filesystem mutation).
fn validate_kebab_name(name: &str) -> Result<()> {
    let ok = !name.is_empty()
        && !name.starts_with('-')
        && !name.ends_with('-')
        && !name.contains("--")
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if !ok {
        bail!("plugin name must be kebab-case (lowercase a-z, 0-9, '-'): got '{name}'");
    }
    Ok(())
}

/// Bail if `dir` exists and is non-empty. Creating into an empty/absent dir
/// is fine. Checked before any write so the error path leaves no partial
/// scaffold.
fn reject_non_empty_dir(dir: &Path) -> Result<()> {
    if !dir
        .try_exists()
        .with_context(|| format!("failed to inspect {}", dir.display()))?
    {
        return Ok(());
    }
    let mut entries = std::fs::read_dir(dir)
        .with_context(|| format!("failed to read {}", dir.display()))?;
    if entries.next().is_some() {
        bail!("refusing to scaffold into non-empty directory {}", dir.display());
    }
    Ok(())
}

/// Compute `(YYYY-MM-DD, YYYY)` for the current UTC date from `SystemTime`,
/// avoiding a date-crate dependency. A civil-date conversion (Howard
/// Hinnant's algorithm) over the Unix day count.
fn scaffold_date() -> (String, String) {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = (secs / 86_400) as i64;
    let (y, m, d) = civil_from_days(days);
    (format!("{y:04}-{m:02}-{d:02}"), format!("{y:04}"))
}

/// days since 1970-01-01 → (year, month, day). Hinnant's `civil_from_days`.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kebab_validation_accepts_canonical_names() {
        assert!(validate_kebab_name("scalars-br").is_ok());
        assert!(validate_kebab_name("mercadopago").is_ok());
        assert!(validate_kebab_name("a1-b2").is_ok());
    }

    #[test]
    fn kebab_validation_rejects_bad_names() {
        assert!(validate_kebab_name("Foo_Bar").is_err());
        assert!(validate_kebab_name("UPPER").is_err());
        assert!(validate_kebab_name("-lead").is_err());
        assert!(validate_kebab_name("trail-").is_err());
        assert!(validate_kebab_name("double--dash").is_err());
        assert!(validate_kebab_name("").is_err());
    }

    #[test]
    fn tokens_derive_go_package_and_type_name() {
        let t = build_tokens("my-scalars", None);
        assert_eq!(t.go_package, "myscalars");
        assert_eq!(t.type_name, "Scalars");
        assert_eq!(t.module, "lazuli.dev/plugin/my-scalars");
        assert_eq!(t.namespace, "@lazuli/plugin-my-scalars");
    }

    #[test]
    fn namespace_override_is_honored() {
        let t = build_tokens("foo", Some("@acme/foo".to_string()));
        assert_eq!(t.namespace, "@acme/foo");
    }

    #[test]
    fn rendered_path_strips_tmpl_and_substitutes_name() {
        let t = build_tokens("widget", None);
        let p = rendered_relative_path(Path::new("__name__.go.tmpl"), &t);
        assert_eq!(p, Path::new("widget.go"));
        let p2 = rendered_relative_path(Path::new("__name___test.go.tmpl"), &t);
        assert_eq!(p2, Path::new("widget_test.go"));
    }

    #[test]
    fn civil_date_known_epochs() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        // 2026-06-01 is 20_605 days after the epoch.
        assert_eq!(civil_from_days(20_605), (2026, 6, 1));
    }
}
