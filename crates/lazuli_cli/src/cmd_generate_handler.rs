//! `lazuli generate handler <feature>.<fn>` subcommand.
//!
//! Creates `<app_dir>/features/<feature>/handlers/<fn>.go` for a
//! referenced `@fn.<fn>` in the feature's `.lzi` source. Handlers
//! live in a dedicated `handlers/` sub-folder so the feature
//! directory stays focused on the DSL surface; the sub-folder
//! declares `package <feature>handlers`. See
//! `docs/project-structure.md` for the canonical layout and the
//! rationale (cycle from gen→handler resolved by the runtime
//! registry).

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use lazuli_analyzer::lower_feature_skeleton;
use lazuli_doctor::handler_path;
use lazuli_doctor::handler_walker::{HandlerSite, HandlerSiteKind, iter_handler_sites};
use lazuli_ir::Feature;
use lazuli_syntax::parse_feature_skeletons;

use crate::signature_aware_stub::{StubContext, render_test_stub};

/// Run `lazuli generate handler <ident>`.
/// `ident` is `<feature>.<fn_name>`.
///
/// Creates `<app_dir>/features/<feature>/handlers/<fn>.go` for an
/// `@fn.<fn>` reference declared in the feature `.lzi`. Signature
/// inference uses [`lazuli_analyzer::lower_feature_skeleton`] and the
/// handler walker so the stub already typechecks.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_cli::cmd_generate_handler::run;
///
/// // run("post.publish_post", Path::new("."))?;
/// ```
pub fn run(ident: &str, project_root: &Path) -> Result<()> {
    let (feature, fn_name) = parse_ident(ident)?;
    validate_part(&feature, "feature")?;
    validate_part(&fn_name, "fn name")?;

    let feat_root = app_root(project_root)?.join("features").join(&feature);
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

    // Canonical layout: handler at `<app_dir>/features/<feature>/handlers/<fn>.go`
    // in `package <feature>handlers`. See `docs/project-structure.md`
    // for the rationale (Tier 1 portable code; gen lives separately
    // in `dist/go/<feature>/` as package `<feature>gen`; cycle
    // resolved by the runtime registry, so the `handlers/` sub-folder
    // is viable and keeps the feature dir focused on DSL surface).
    let handlers_dir = feat_root.join("handlers");
    fs::create_dir_all(&handlers_dir)
        .with_context(|| format!("creating {}", handlers_dir.display()))?;

    let target = handlers_dir.join(format!("{}.go", fn_name));
    if target.exists() {
        return Err(anyhow!("handler already exists: {}", target.display()));
    }

    let body = render_stub(&feature, &fn_name);
    fs::write(&target, body).with_context(|| format!("writing {}", target.display()))?;

    // Wave 5 — emit the paired `_test.go` alongside the handler. The
    // pair lands at the same canonical path enforced by
    // TEST-HANDLER-MISSING-001 so doctor stays silent immediately after
    // scaffolding. Existing `_test.go` files are left untouched —
    // matches the .go safe-check above (refuse to overwrite author
    // content).
    let test_target = handler_path::resolve_test(&app_root(project_root)?, &feature, &fn_name);
    if !test_target.exists() {
        let ir_feature = parse_ir_feature(&lzi_path, &source).ok();
        let test_body = render_test_for(&ir_feature, &feature, &fn_name);
        fs::write(&test_target, test_body)
            .with_context(|| format!("writing {}", test_target.display()))?;
    }
    Ok(())
}

/// Best-effort IR lift for the feature file. Returns `None` when the
/// `.lzi` parser/analyzer can't lift the source — the generator falls
/// back to a generic stub in that case so a partially-typed `.lzi`
/// never blocks `lazuli generate handler`.
fn parse_ir_feature(lzi_path: &Path, source: &str) -> Result<Feature> {
    let skeletons = parse_feature_skeletons(source)
        .map_err(|err| anyhow!("parse {}: {err:?}", lzi_path.display()))?;
    let skeleton = skeletons
        .into_iter()
        .next()
        .ok_or_else(|| anyhow!("no feature skeleton in {}", lzi_path.display()))?;
    lower_feature_skeleton(&skeleton)
        .map_err(|err| anyhow!("lower {}: {err:?}", lzi_path.display()))
}

/// Pick the right test stub renderer. When we have a typed IR feature
/// AND a recognized handler site, use the signature-aware renderer.
/// Otherwise emit a generic table-driven stub that compiles with stdlib
/// only — same shape, fewer hints.
fn render_test_for(ir_feature: &Option<Feature>, feature_name: &str, handler_name: &str) -> String {
    if let Some(feature) = ir_feature
        && let Some(site) = find_handler_site(feature, handler_name)
    {
        return render_test_stub(&StubContext {
            feature,
            site: &site,
        });
    }
    render_generic_test_stub(feature_name, handler_name)
}

fn find_handler_site(feature: &Feature, handler_name: &str) -> Option<HandlerSite> {
    iter_handler_sites(feature).into_iter().find(|s| {
        s.handler_name == handler_name && !matches!(s.kind, HandlerSiteKind::WebhookHandler)
    })
}

/// Fallback when the IR lift didn't surface the handler — emit a
/// minimal table-driven stub that still compiles. Authors who care
/// about boundary enumeration can re-run after fixing the `.lzi`.
fn render_generic_test_stub(feature: &str, handler_name: &str) -> String {
    let pkg = format!("{}handlers", feature);
    let fn_pascal = pascal_case(handler_name);
    format!(
        r#"package {pkg}

// Auto-generated test stub for @fn.{handler}.
// Pair file for {handler}.go. The IR lift could not infer the
// handler's signature (parser/analyzer fell back), so this stub is
// the structural minimum. Re-run `lazuli generate handler` after
// fixing the `.lzi` to get the signature-aware version.

import (
	"testing"
)

func Test{fn_pascal}(t *testing.T) {{
	tests := []struct {{
		name string
		// @TODO authored: extend with handler-specific fields (input, want, etc.)
	}}{{
		{{name: "golden path"}}, // @TODO authored: exercise the happy branch
		{{name: "error path"}},  // @TODO authored: exercise at least one error branch
	}}
	for _, tt := range tests {{
		t.Run(tt.name, func(t *testing.T) {{
			// @TODO authored: invoke {fn_pascal} with tt's fields and assert.
			_ = tt
		}})
	}}
}}
"#,
        pkg = pkg,
        handler = handler_name,
        fn_pascal = fn_pascal,
    )
}

fn app_root(project_root: &Path) -> Result<PathBuf> {
    let manifest = crate::lazurite_manifest::load(project_root)
        .map_err(|err| anyhow!("failed to load Lazurite.toml: {err}"))?;
    Ok(manifest
        .as_ref()
        .map(|manifest| manifest.app_root(project_root))
        .unwrap_or_else(|| project_root.to_path_buf()))
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
        r#"package {feature}handlers

import (
	"context"
	"errors"
)

// {fn_pascal} is the handler for @fn.{fn_name}.
//
// Reference: app/features/{feature}/{feature}.lzi (search for `@fn.{fn_name}`).
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

    fn write_app_dir_manifest(root: &Path) {
        fs::write(
            root.join("Lazurite.toml"),
            r#"[project]
name = "demo"
module = "github.com/acme/demo"
schema = 1

[lazuli]
runtime = "0.1.0"

[lazurite]
app_dir = "app"
"#,
        )
        .unwrap();
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
    fn success_emits_paired_test_file() {
        // Wave 5 — `lazuli generate handler` writes BOTH the handler
        // and the paired `_test.go` so TEST-HANDLER-MISSING-001 stays
        // silent right after scaffolding.
        let project = tempdir();
        write_lzi(
            project.path(),
            "auth",
            "feature auth\n  command login\n    handler @fn.verify_password\n",
        );

        run("auth.verify_password", project.path()).unwrap();

        let test_path = project
            .path()
            .join("features/auth/handlers/verify_password_test.go");
        assert!(test_path.exists(), "expected paired _test.go to exist");
        let body = fs::read_to_string(test_path).unwrap();
        assert!(body.contains("package authhandlers"));
        assert!(body.contains("func TestVerifyPassword("));
        assert!(body.contains("\"testing\""));
        // Stub depends only on the Go stdlib — pilots may add
        // testify/etc. later but the seed must compile bare.
        assert!(!body.contains("github.com/"));
    }

    #[test]
    fn preserves_existing_test_file() {
        // Author may have hand-written the test before running the
        // generator. The generator MUST NOT overwrite either file —
        // mirrors the `.go` safe-check.
        let project = tempdir();
        write_lzi(
            project.path(),
            "auth",
            "feature auth\n  command login\n    handler @fn.verify_password\n",
        );
        let handlers_dir = project.path().join("features/auth/handlers");
        fs::create_dir_all(&handlers_dir).unwrap();
        fs::write(
            handlers_dir.join("verify_password_test.go"),
            "package authhandlers\n// AUTHOR EDIT\n",
        )
        .unwrap();

        run("auth.verify_password", project.path()).unwrap();

        let body = fs::read_to_string(handlers_dir.join("verify_password_test.go")).unwrap();
        assert!(
            body.contains("AUTHOR EDIT"),
            "must preserve author content, got: {body}"
        );
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
        let feat_dir = project.path().join("features/auth");
        let handlers_dir = feat_dir.join("handlers");
        fs::create_dir_all(&handlers_dir).unwrap();
        fs::write(
            handlers_dir.join("verify_password.go"),
            "package authhandlers\n",
        )
        .unwrap();

        let err = run("auth.verify_password", project.path()).unwrap_err();

        assert!(err.to_string().contains("already exists"));
    }

    #[test]
    fn error_on_invalid_ident() {
        let project = tempdir();

        let err = run("badident", project.path()).unwrap_err();

        assert!(
            err.to_string()
                .contains("handler ident must be <feature>.<fn_name>")
        );
    }

    #[test]
    fn pascal_case_conversion() {
        assert_eq!(pascal_case("verify_password_v2"), "VerifyPasswordV2");
    }

    #[test]
    fn honors_app_dir_manifest() {
        let project = tempdir();
        write_app_dir_manifest(project.path());
        let feature_root = project.path().join("app/features/auth");
        fs::create_dir_all(&feature_root).unwrap();
        fs::write(
            feature_root.join("auth.lzi"),
            "feature auth\n  command login\n    handler @fn.verify_password\n",
        )
        .unwrap();

        run("auth.verify_password", project.path()).unwrap();

        assert!(feature_root.join("handlers/verify_password.go").exists());
        // Should NOT fall back to the root-relative path when the
        // manifest points at app/.
        assert!(
            !project
                .path()
                .join("features/auth/handlers/verify_password.go")
                .exists()
        );
    }
}
