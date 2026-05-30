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
        std::fs::create_dir_all(tmp.path().join("app").join("clients").join("the canonical pilot-app"))
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
            Some("app/clients/the canonical pilot-app".to_string())
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
