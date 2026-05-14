//! `lazuli generate handler <feature>.<fn>` subcommand.
//!
//! Creates `features/<feature>/handlers/<fn>.go` for a referenced
//! `@fn.<fn>` in the feature's `.lzi` source.

use std::fs;
use std::path::Path;

use anyhow::{anyhow, Context, Result};

/// Run `lazuli generate handler <ident>`.
/// `ident` is `<feature>.<fn_name>`.
pub fn run(ident: &str, project_root: &Path) -> Result<()> {
    let (feature, fn_name) = parse_ident(ident)?;
    validate_part(&feature, "feature")?;
    validate_part(&fn_name, "fn name")?;

    let feat_root = project_root.join("features").join(&feature);
    let lzi_path = feat_root.join(format!("{}.lzi", feature));
    if !lzi_path.exists() {
        return Err(anyhow!("feature .lzi not found: {}", lzi_path.display()));
    }

    let source =
        fs::read_to_string(&lzi_path).with_context(|| format!("reading {}", lzi_path.display()))?;
    let needle = format!("@fn.{}", fn_name);
    if !source.contains(&needle) {
        return Err(anyhow!(
            "`@fn.{}` is not referenced in {}",
            fn_name,
            lzi_path.display()
        ));
    }

    let handlers_dir = feat_root.join("handlers");
    fs::create_dir_all(&handlers_dir)
        .with_context(|| format!("creating {}", handlers_dir.display()))?;

    let target = handlers_dir.join(format!("{}.go", fn_name));
    if target.exists() {
        return Err(anyhow!("handler already exists: {}", target.display()));
    }

    let body = render_stub(&feature, &fn_name);
    fs::write(&target, body).with_context(|| format!("writing {}", target.display()))?;
    Ok(())
}

fn parse_ident(ident: &str) -> Result<(String, String)> {
    let mut parts = ident.split('.');
    let feature = parts.next().unwrap_or_default();
    let fn_name = parts.next().unwrap_or_default();
    if feature.is_empty() || fn_name.is_empty() || parts.next().is_some() {
        return Err(anyhow!(
            "handler ident must be <feature>.<fn_name>; got {ident}"
        ));
    }

    Ok((feature.to_string(), fn_name.to_string()))
}

fn validate_part(name: &str, kind: &str) -> Result<()> {
    if name.len() < 2 || name.len() > 64 {
        return Err(anyhow!("{kind} must be 2-64 chars; got {}", name.len()));
    }

    let bytes = name.as_bytes();
    if !bytes[0].is_ascii_lowercase() {
        return Err(anyhow!("{kind} must start with a lowercase letter"));
    }

    for &byte in bytes {
        if !(byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_') {
            return Err(anyhow!(
                "{kind} must be snake_case (lowercase, digits, underscores)"
            ));
        }
    }

    Ok(())
}

fn render_stub(feature: &str, fn_name: &str) -> String {
    let fn_pascal = pascal_case(fn_name);
    format!(
        r#"package {feature}

import (
	"context"
	"errors"
)

// {fn_pascal} is the handler for @fn.{fn_name}.
//
// Reference: features/{feature}/{feature}.lzi (search for `@fn.{fn_name}`).
//
// Implement the body to match the expected signature. The IR generator
// will produce a typed callsite when the corresponding command/validator
// is regenerated; until then, this stub compiles against the runtime
// adapter shape and returns a TODO error so callers fail-fast at runtime
// during development.
func {fn_pascal}(ctx context.Context /* TODO: typed input */) error {{
	_ = ctx
	return errors.New("@fn.{feature}.{fn_name}: handler not implemented")
}}
"#
    )
}

fn pascal_case(snake: &str) -> String {
    let mut out = String::new();
    for part in snake.split('_') {
        let mut chars = part.chars();
        if let Some(first) = chars.next() {
            out.extend(first.to_uppercase());
            out.push_str(chars.as_str());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn tempdir() -> TempDir {
        TempDir::new().unwrap()
    }

    fn write_lzi(root: &Path, feature: &str, content: &str) {
        let feature_root = root.join("features").join(feature);
        fs::create_dir_all(&feature_root).unwrap();
        fs::write(feature_root.join(format!("{feature}.lzi")), content).unwrap();
    }

    #[test]
    fn success_creates_handler_with_correct_signature() {
        let project = tempdir();
        write_lzi(
            project.path(),
            "auth",
            "feature auth\n  command login\n    handler @fn.verify_password\n",
        );

        run("auth.verify_password", project.path()).unwrap();

        let target = project
            .path()
            .join("features/auth/handlers/verify_password.go");
        assert!(target.exists());
        let body = fs::read_to_string(target).unwrap();
        assert!(body.contains("func VerifyPassword("));
    }

    #[test]
    fn error_when_lzi_missing() {
        let project = tempdir();

        let err = run("auth.verify_password", project.path()).unwrap_err();

        assert!(err.to_string().contains("feature .lzi not found"));
    }

    #[test]
    fn error_when_fn_not_referenced() {
        let project = tempdir();
        write_lzi(project.path(), "auth", "feature auth\n  command login\n");

        let err = run("auth.verify_password", project.path()).unwrap_err();

        assert!(err.to_string().contains("not referenced"));
    }

    #[test]
    fn error_when_handler_already_exists() {
        let project = tempdir();
        write_lzi(
            project.path(),
            "auth",
            "feature auth\n  command login\n    handler @fn.verify_password\n",
        );
        let handlers_dir = project.path().join("features/auth/handlers");
        fs::create_dir_all(&handlers_dir).unwrap();
        fs::write(handlers_dir.join("verify_password.go"), "package auth\n").unwrap();

        let err = run("auth.verify_password", project.path()).unwrap_err();

        assert!(err.to_string().contains("already exists"));
    }

    #[test]
    fn error_on_invalid_ident() {
        let project = tempdir();

        let err = run("badident", project.path()).unwrap_err();

        assert!(err
            .to_string()
            .contains("handler ident must be <feature>.<fn_name>"));
    }

    #[test]
    fn pascal_case_conversion() {
        assert_eq!(pascal_case("verify_password_v2"), "VerifyPasswordV2");
    }
}
