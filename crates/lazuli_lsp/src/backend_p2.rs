/// Run the LSP server over stdin/stdout. Blocks until the client
/// closes the stream — the canonical wiring used by `lazuli lsp`
/// (the CLI entry point) and by VS Code / Helix integrations that
/// launch the server as a subprocess.
///
/// Owns the live `Backend` instance and the tower-lsp `Server` plumbing;
/// no public API beyond this function — every diagnostic / completion /
/// hover surface routes through the trait impl on `Backend`.
///
/// ## Examples
///
/// ```no_run
/// // Run the LSP server over stdin/stdout. Blocks forever until the
/// // client disconnects.
/// # async fn run() {
/// lazuli_lsp::serve_stdio().await;
/// # }
/// ```
pub async fn serve_stdio() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let (service, socket) = LspService::new(|client| Backend {
        client,
        documents: Arc::new(RwLock::new(HashMap::new())),
        workspace_root: Arc::new(RwLock::new(None)),
        doctor_run_generation: Arc::new(AtomicU64::new(0)),
    });

    Server::new(stdin, stdout, socket).serve(service).await;
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use lazuli_doctor_config::{DoctorProfile, DoctorSeverity, RuleCategory, effective_severity};
    use tower_lsp::lsp_types::Url;

    use super::resolve_workspace_config;

    /// Make a unique throwaway directory under the system temp dir. Mirrors
    /// the pattern the LSP lib-test helpers use (no `tempfile` dev-dep).
    fn unique_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "lazuli-lsp-cfg-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp workspace");
        dir
    }

    /// v2 residual — the UNSAVED `Lazurite.toml` editor buffer drives the
    /// resolved doctor config (and thus the package finding's published
    /// severity) when that file is open, over the on-disk contents.
    ///
    /// Disk pins `[doctor] profile = "prototype"`; the open buffer flips it
    /// to `production` AND escalates the VOCAB-CONTEXT family via
    /// `[doctor.coverage] preset = "tdd-iron-hand"`. The resolved config —
    /// and the severity `effective_severity` yields for a doctor-owned
    /// package code — must reflect the BUFFER, proving unsaved edits take
    /// effect in-editor.
    #[test]
    fn open_buffer_lazurite_toml_overrides_disk_for_severity() {
        let root = unique_dir("buffer-wins");
        let manifest_path = root.join("Lazurite.toml");
        // DISK: prototype profile, no coverage preset. Under prototype a
        // doctor-owned vocab code lands at WARNING and the VOCAB-CONTEXT
        // family is NOT escalated.
        std::fs::write(&manifest_path, "[doctor]\nprofile = \"prototype\"\n")
            .expect("write disk Lazurite.toml");

        // A feature `.lzi` in the same dir, so config discovery anchors on
        // this workspace's manifest.
        let feature = root.join("acct.lzi");
        std::fs::write(&feature, "feature acct\n").expect("write feature");
        let doc_uri = Url::from_file_path(&feature).expect("doc uri");
        let manifest_uri = Url::from_file_path(&manifest_path).expect("manifest uri");

        // BUFFER: the editor has the manifest open with UNSAVED edits —
        // production profile + iron-hand coverage preset (escalates the
        // VOCAB-CONTEXT trio to ERROR).
        let mut open_docs: HashMap<Url, String> = HashMap::new();
        open_docs.insert(
            manifest_uri,
            "[doctor]\nprofile = \"production\"\n\n[doctor.coverage]\npreset = \"tdd-iron-hand\"\n"
                .to_owned(),
        );

        let cfg = resolve_workspace_config(&doc_uri, Some(&root), &open_docs);

        // Profile reflects the BUFFER (production), not disk (prototype).
        assert_eq!(
            cfg.profile.0,
            DoctorProfile::Production,
            "buffer profile must win over disk"
        );

        // A doctor-owned VOCAB-CONTEXT code is ESCALATED to ERROR by the
        // buffer's iron-hand coverage preset — disk (no preset, prototype)
        // would have yielded WARNING.
        let sev = effective_severity(
            "VOCAB-CONTEXT-PURPOSE-001",
            DoctorSeverity::Warning,
            RuleCategory::Vocabulary,
            &cfg,
        );
        assert_eq!(
            sev,
            Some(DoctorSeverity::Error),
            "buffer's iron-hand coverage preset must escalate the package finding"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    /// Fallback — with NO open buffer for the workspace `Lazurite.toml`,
    /// the resolved config comes from DISK. Same fixture, but the docs map
    /// is empty: profile + preset must reflect the on-disk file.
    #[test]
    fn no_open_buffer_falls_back_to_disk_for_severity() {
        let root = unique_dir("disk-fallback");
        let manifest_path = root.join("Lazurite.toml");
        // DISK: production profile + iron-hand coverage preset.
        std::fs::write(
            &manifest_path,
            "[doctor]\nprofile = \"production\"\n\n[doctor.coverage]\npreset = \"tdd-iron-hand\"\n",
        )
        .expect("write disk Lazurite.toml");

        let feature = root.join("acct.lzi");
        std::fs::write(&feature, "feature acct\n").expect("write feature");
        let doc_uri = Url::from_file_path(&feature).expect("doc uri");

        // No open documents at all -> disk is the only source.
        let open_docs: HashMap<Url, String> = HashMap::new();
        let cfg = resolve_workspace_config(&doc_uri, Some(&root), &open_docs);

        assert_eq!(
            cfg.profile.0,
            DoctorProfile::Production,
            "disk profile drives the config when the manifest is not open"
        );
        let sev = effective_severity(
            "VOCAB-CONTEXT-PURPOSE-001",
            DoctorSeverity::Warning,
            RuleCategory::Vocabulary,
            &cfg,
        );
        assert_eq!(
            sev,
            Some(DoctorSeverity::Error),
            "disk's iron-hand preset escalates the package finding on fallback"
        );

        let _ = std::fs::remove_dir_all(&root);
    }
}
