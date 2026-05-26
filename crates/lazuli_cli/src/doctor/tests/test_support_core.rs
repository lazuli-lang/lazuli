    // Doctor-test small filesystem + git fixture helpers — split from
    // `crates/lazuli_cli/src/doctor/tests.rs`. Promoted to `pub(super)`
    // so each sibling test sub-module can pull them via
    // `use super::test_support_core::*;`.

    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    pub(super) fn temp_project_root(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("lazuli-{name}-{unique}"));
        std::fs::create_dir_all(&root).expect("create temp project root");
        root
    }

    pub(super) fn write_file(path: &Path, source: &str) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create parent dirs");
        }
        std::fs::write(path, source).expect("write test file");
    }

    pub(super) fn init_git_repo_with_commit(root: &Path, timestamp: u64) -> bool {
        let init = std::process::Command::new("git")
            .args(["init"])
            .current_dir(root)
            .output();
        if !init.map(|output| output.status.success()).unwrap_or(false) {
            return false;
        }

        for args in [
            ["config", "user.email", "test@example.com"],
            ["config", "user.name", "Lazuli Test"],
        ] {
            let _ = std::process::Command::new("git")
                .args(args)
                .current_dir(root)
                .output();
        }

        let add = std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(root)
            .output();
        if !add.map(|output| output.status.success()).unwrap_or(false) {
            return false;
        }

        std::process::Command::new("git")
            .args(["commit", "-m", "fixture"])
            .env("GIT_AUTHOR_DATE", format!("@{timestamp} +0000"))
            .env("GIT_COMMITTER_DATE", format!("@{timestamp} +0000"))
            .current_dir(root)
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    pub(super) fn temp_project(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "lazuli-doctor-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    pub(super) fn codes(diagnostics: &[crate::doctor::DoctorDiagnostic]) -> std::collections::BTreeSet<&str> {
        diagnostics.iter().map(|d| d.code.as_str()).collect()
    }
