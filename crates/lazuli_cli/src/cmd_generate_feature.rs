//! `lazuli generate feature <name>` subcommand.
//!
//! Creates `<app_dir>/features/<name>/` with canonical Lazurite folder structure
//! per L0 #1 §6.2. Frontend subdirs (`web/`, `mobile/`) are only created
//! when the project's `Lazurite.toml` declares matching frontends.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};

/// Run the subcommand. `project_root` is the directory containing
/// `Lazurite.toml` (or the CWD if no manifest).
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_cli::cmd_generate_feature::run;
///
/// // run("orders", Path::new("."))?;
/// ```
pub fn run(name: &str, project_root: &Path) -> Result<()> {
    validate_feature_name(name)?;

    let feature_root = app_root(project_root)?.join("features").join(name);
    if feature_root.exists() {
        return Err(anyhow!(
            "feature directory already exists: {}",
            feature_root.display()
        ));
    }

    let frontends = detect_frontends(project_root)?;

    create_backend_skeleton(&feature_root, name)?;
    if frontends.web {
        create_web_skeleton(&feature_root, name)?;
    }
    if frontends.mobile {
        create_mobile_skeleton(&feature_root, name)?;
    }

    Ok(())
}

fn app_root(project_root: &Path) -> Result<PathBuf> {
    let manifest = crate::lazurite_manifest::load(project_root)
        .map_err(|err| anyhow!("failed to load Lazurite.toml: {err}"))?;
    Ok(manifest
        .as_ref()
        .map(|manifest| manifest.app_root(project_root))
        .unwrap_or_else(|| project_root.to_path_buf()))
}

fn validate_feature_name(name: &str) -> Result<()> {
    if name.len() < 2 || name.len() > 64 {
        return Err(anyhow!(
            "feature name must be 2-64 chars; got {}",
            name.len()
        ));
    }

    let bytes = name.as_bytes();
    if !bytes[0].is_ascii_lowercase() {
        return Err(anyhow!("feature name must start with a lowercase letter"));
    }

    for &byte in bytes {
        if !(byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_') {
            return Err(anyhow!(
                "feature name must be snake_case (lowercase, digits, underscores)"
            ));
        }
    }

    Ok(())
}

struct DetectedFrontends {
    web: bool,
    mobile: bool,
}

fn detect_frontends(project_root: &Path) -> Result<DetectedFrontends> {
    let manifest_path = project_root.join("Lazurite.toml");
    if !manifest_path.exists() {
        return Ok(DetectedFrontends {
            web: false,
            mobile: false,
        });
    }

    let raw = fs::read_to_string(&manifest_path)
        .with_context(|| format!("reading {}", manifest_path.display()))?;

    let mut web = false;
    let mut mobile = false;
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("[frontends.web") {
            web = true;
        }
        if trimmed.starts_with("[frontends.mobile") {
            mobile = true;
        }
    }

    Ok(DetectedFrontends { web, mobile })
}

fn create_backend_skeleton(feature_root: &Path, name: &str) -> Result<()> {
    fs::create_dir_all(feature_root)
        .with_context(|| format!("creating {}", feature_root.display()))?;
    write_text(&feature_root.join(format!("{name}.lzi")), &lzi_stub(name))?;

    for subdir in [
        "handlers",
        "queries",
        "jobs",
        "integrations",
        "templates",
        "i18n",
    ] {
        let dir = feature_root.join(subdir);
        fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
        write_text(&dir.join(".gitkeep"), "")?;
    }

    Ok(())
}

fn create_web_skeleton(feature_root: &Path, name: &str) -> Result<()> {
    write_text(
        &feature_root.join(format!("{name}.web.lzx")),
        &web_lzx_stub(name),
    )?;

    for subdir in ["web/cells", "web/views"] {
        let dir = feature_root.join(subdir);
        fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
        write_text(&dir.join(".gitkeep"), "")?;
    }

    Ok(())
}

fn create_mobile_skeleton(feature_root: &Path, name: &str) -> Result<()> {
    write_text(
        &feature_root.join(format!("{name}.mobile.lzx")),
        &mobile_lzx_stub(name),
    )?;

    for subdir in ["mobile/cells", "mobile/views"] {
        let dir = feature_root.join(subdir);
        fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
        write_text(&dir.join(".gitkeep"), "")?;
    }

    Ok(())
}

fn write_text(path: &Path, content: &str) -> Result<()> {
    fs::write(path, content).with_context(|| format!("writing {}", path.display()))
}

fn lzi_stub(name: &str) -> String {
    format!(
        "feature {name}\n  purpose \"...\"\n\n  domain\n    # add resources, queries, commands here\n\n  policies\n    # add policy categories here\n"
    )
}

fn web_lzx_stub(name: &str) -> String {
    format!(
        "surface {name} web\n  uses feature {name}\n\n  # audience admin\n  #   requires @scope.workspace_admin\n  #\n  #   view list {name}_list at \"/{name}\"\n  #     source {name}.query.mine\n  #     columns ...\n  #     actions ...\n"
    )
}

fn mobile_lzx_stub(name: &str) -> String {
    format!(
        "surface {name} mobile\n  uses feature {name}\n\n  # audience mobile\n  #   requires @scope.workspace_member\n  #\n  #   view list {name}_list at \"/{name}\"\n  #     source {name}.query.mine\n  #     columns ...\n"
    )
}

#[cfg(test)]
mod tests {
    use self::tempfile::TempDir;
    use super::*;

    mod tempfile {
        use std::fs;
        use std::io;
        use std::path::{Path, PathBuf};
        use std::time::{SystemTime, UNIX_EPOCH};

        /// Test-only scratch directory. Created under the system temp
        /// dir with a unique suffix and removed on `Drop`. Lives inside
        /// the `tempfile` sub-module so the test suite can build a
        /// fresh project root for each `cmd_generate_feature::run` case.
        pub struct TempDir {
            path: PathBuf,
        }

        impl TempDir {
            /// Allocate a fresh temp directory rooted at the system
            /// temp dir.
            pub fn new() -> io::Result<Self> {
                let suffix = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos();
                let path = std::env::temp_dir().join(format!(
                    "lazuli-generate-feature-test-{}-{suffix}",
                    std::process::id()
                ));
                fs::create_dir_all(&path)?;
                Ok(Self { path })
            }

            /// Borrow the allocated directory.
            pub fn path(&self) -> &Path {
                &self.path
            }
        }

        impl Drop for TempDir {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.path);
            }
        }
    }

    fn tempdir() -> TempDir {
        TempDir::new().unwrap()
    }

    fn write_manifest(root: &Path, content: &str) {
        fs::write(root.join("Lazurite.toml"), content).unwrap();
    }

    fn frontend_manifest(extra: &str) -> String {
        format!(
            r#"[project]
name = "demo"
module = "github.com/acme/demo"
schema = 1

[lazuli]
runtime = "0.1.0"

[lazurite]
app_dir = "app"

{extra}
"#
        )
    }

    fn assert_backend_skeleton(base: &Path, name: &str) {
        let feature_root = base.join("features").join(name);
        assert!(feature_root.join(format!("{name}.lzi")).exists());
        for subdir in [
            "handlers",
            "queries",
            "jobs",
            "integrations",
            "templates",
            "i18n",
        ] {
            assert!(feature_root.join(subdir).join(".gitkeep").exists());
        }
    }

    #[test]
    fn creates_backend_only_when_no_manifest() {
        let project = tempdir();

        run("slug", project.path()).unwrap();

        assert_backend_skeleton(project.path(), "slug");
        let feature_root = project.path().join("features").join("slug");
        assert!(!feature_root.join("slug.web.lzx").exists());
        assert!(!feature_root.join("slug.mobile.lzx").exists());
        assert!(!feature_root.join("web").exists());
        assert!(!feature_root.join("mobile").exists());
    }

    #[test]
    fn creates_web_when_manifest_declares_web() {
        let project = tempdir();
        write_manifest(
            project.path(),
            &frontend_manifest(
                "[frontends.web]\ntarget = \"tanstack-vite\"\nsource = \"frontends/web\"\nout = \"dist/ts-web\"\naudiences = [\"admin\"]\n",
            ),
        );

        run("slug", project.path()).unwrap();

        let feature_root = project.path().join("app").join("features").join("slug");
        assert!(feature_root.join("slug.web.lzx").exists());
        assert!(
            feature_root
                .join("web")
                .join("cells")
                .join(".gitkeep")
                .exists()
        );
        assert!(
            feature_root
                .join("web")
                .join("views")
                .join(".gitkeep")
                .exists()
        );
        assert!(!feature_root.join("slug.mobile.lzx").exists());
    }

    #[test]
    fn creates_mobile_when_manifest_declares_mobile() {
        let project = tempdir();
        write_manifest(
            project.path(),
            &frontend_manifest(
                "[frontends.mobile]\ntarget = \"expo\"\nsource = \"frontends/mobile\"\nout = \"dist/ts-mobile\"\naudiences = [\"mobile\"]\n",
            ),
        );

        run("slug", project.path()).unwrap();

        let feature_root = project.path().join("app").join("features").join("slug");
        assert!(feature_root.join("slug.mobile.lzx").exists());
        assert!(
            feature_root
                .join("mobile")
                .join("cells")
                .join(".gitkeep")
                .exists()
        );
        assert!(
            feature_root
                .join("mobile")
                .join("views")
                .join(".gitkeep")
                .exists()
        );
        assert!(!feature_root.join("slug.web.lzx").exists());
    }

    #[test]
    fn creates_both_when_both_declared() {
        let project = tempdir();
        write_manifest(
            project.path(),
            &frontend_manifest(
                "[frontends.web]\ntarget = \"tanstack-vite\"\nsource = \"frontends/web\"\nout = \"dist/ts-web\"\naudiences = [\"admin\"]\n\n[frontends.mobile]\ntarget = \"expo\"\nsource = \"frontends/mobile\"\nout = \"dist/ts-mobile\"\naudiences = [\"mobile\"]\n",
            ),
        );

        run("slug", project.path()).unwrap();

        let feature_root = project.path().join("app").join("features").join("slug");
        assert!(feature_root.join("slug.web.lzx").exists());
        assert!(
            feature_root
                .join("web")
                .join("cells")
                .join(".gitkeep")
                .exists()
        );
        assert!(
            feature_root
                .join("web")
                .join("views")
                .join(".gitkeep")
                .exists()
        );
        assert!(feature_root.join("slug.mobile.lzx").exists());
        assert!(
            feature_root
                .join("mobile")
                .join("cells")
                .join(".gitkeep")
                .exists()
        );
        assert!(
            feature_root
                .join("mobile")
                .join("views")
                .join(".gitkeep")
                .exists()
        );
    }

    #[test]
    fn fails_when_feature_already_exists() {
        let project = tempdir();
        fs::create_dir_all(project.path().join("features").join("slug")).unwrap();

        let err = run("slug", project.path()).unwrap_err();

        assert!(err.to_string().contains("feature directory already exists"));
    }

    #[test]
    fn rejects_invalid_name_with_uppercase() {
        assert!(run("SlugThing", tempdir().path()).is_err());
    }

    #[test]
    fn rejects_invalid_name_starting_with_digit() {
        assert!(run("9slug", tempdir().path()).is_err());
    }

    #[test]
    fn rejects_invalid_name_with_hyphen() {
        assert!(run("slug-key", tempdir().path()).is_err());
    }

    #[test]
    fn accepts_valid_snake_case() {
        let project = tempdir();

        run("user_session", project.path()).unwrap();

        assert_backend_skeleton(project.path(), "user_session");
    }

    #[test]
    fn lzi_stub_contains_feature_name() {
        let project = tempdir();

        run("slug", project.path()).unwrap();

        let lzi = fs::read_to_string(project.path().join("features/slug/slug.lzi")).unwrap();
        assert!(lzi.starts_with("feature slug\n"));
    }

    #[test]
    fn creates_under_app_dir_when_manifest_declares_app_dir() {
        let project = tempdir();
        write_manifest(project.path(), &frontend_manifest(""));

        run("slug", project.path()).unwrap();

        assert_backend_skeleton(&project.path().join("app"), "slug");
        assert!(!project.path().join("features/slug/slug.lzi").exists());
    }
}
