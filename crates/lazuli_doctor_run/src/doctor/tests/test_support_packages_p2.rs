    pub(super) fn package_from_sources_with_manifest(
        sources: Vec<(&str, &str)>,
        manifest_source: &str,
    ) -> DoctorPackage {
        let mut package = package_from_sources(sources);
        let root = std::env::temp_dir().join(format!(
            "lazuli-doctor-manifest-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("create temp manifest project");
        fs::write(root.join("Lazurite.toml"), manifest_source).expect("write Lazurite.toml");
        package.project_root = root;
        let manifest: lazuli_manifest::lazurite_manifest::Manifest =
            toml::from_str(manifest_source).unwrap();
        // v2 — the severity config now drives every preset/override
        // decision. Build it from the SAME manifest `[doctor]` section
        // the test wrote, at the package's Strict profile, so
        // preset/override-driven test assertions keep their behavior.
        package.config = lazuli_doctor_config::ResolvedDoctorConfig::from_doctor(
            manifest.doctor.as_ref(),
            package.security_profile,
        );
        package.lazurite_manifest = Some(manifest);
        package
    }

    pub(super) fn minimal_manifest(extra: &str) -> String {
        format!(
            r#"
[project]
name = "demo"
module = "example.com/demo"
schema = 1

[lazuli]
runtime = "v0.1.0"

{extra}
"#
        )
    }
