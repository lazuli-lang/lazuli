#[cfg(feature = "smoke")]
mod smoke {
    use std::env;
    use std::fs::{self, OpenOptions};
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Output};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(repo_root: &Path) -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time before UNIX_EPOCH")
                .as_nanos();
            let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
            let path = repo_root
                .join("target")
                .join("lazuli-go-smoke")
                .join(format!("{}-{nonce}-{id}", std::process::id()));
            fs::create_dir_all(&path)
                .unwrap_or_else(|err| panic!("creating tempdir {}: {err}", path.display()));
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn full_capsule_compiles_with_go_build() {
        let repo_root = repo_root();
        let tempdir = TempDir::new(&repo_root);

        generate_full_capsule(&repo_root, tempdir.path());

        append_runtime_replace(&repo_root, tempdir.path());

        let build = Command::new("go")
            .current_dir(tempdir.path())
            .env("GOFLAGS", "-mod=mod")
            .args(["build", "./..."])
            .output()
            .expect("failed to run `go build ./...`");
        assert_success("go build ./...", &build);
    }

    #[test]
    fn full_capsule_generated_go_is_gofmt_clean() {
        let repo_root = repo_root();
        let tempdir = TempDir::new(&repo_root);

        generate_full_capsule(&repo_root, tempdir.path());

        let go_files = collect_go_files(tempdir.path());
        assert!(
            !go_files.is_empty(),
            "expected generated Go files under {}",
            tempdir.path().display()
        );

        let gofmt = Command::new("gofmt")
            .current_dir(tempdir.path())
            .arg("-l")
            .args(&go_files)
            .output()
            .expect("failed to run `gofmt -l`");
        assert_success("gofmt -l", &gofmt);

        let listed = String::from_utf8_lossy(&gofmt.stdout);
        assert!(
            listed.trim().is_empty(),
            "generated Go files are not gofmt-clean:\n{listed}"
        );
    }

    fn generate_full_capsule(repo_root: &Path, out_dir: &Path) {
        let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
        let generate = Command::new(cargo)
            .current_dir(repo_root)
            .args([
                "run",
                "-q",
                "-p",
                "lazuli_cli",
                "--bin",
                "lazuli",
                "--",
                "generate",
                "go",
                "examples/full-capsule",
                "--out",
            ])
            .arg(out_dir)
            .output()
            .expect("failed to run `cargo run -p lazuli_cli --bin lazuli -- generate go`");
        assert_success("lazuli generate go examples/full-capsule", &generate);
    }

    fn collect_go_files(root: &Path) -> Vec<PathBuf> {
        let mut files = Vec::new();
        collect_go_files_into(root, &mut files);
        files.sort();
        files
    }

    fn collect_go_files_into(dir: &Path, files: &mut Vec<PathBuf>) {
        for entry in
            fs::read_dir(dir).unwrap_or_else(|err| panic!("reading {}: {err}", dir.display()))
        {
            let entry =
                entry.unwrap_or_else(|err| panic!("reading entry in {}: {err}", dir.display()));
            let path = entry.path();
            if path.is_dir() {
                collect_go_files_into(&path, files);
            } else if path.extension().and_then(|ext| ext.to_str()) == Some("go") {
                files.push(path);
            }
        }
    }

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .expect("crate should live under <repo>/crates/lazuli_codegen_go")
            .to_path_buf()
    }

    fn append_runtime_replace(repo_root: &Path, out_dir: &Path) {
        let go_mod = out_dir.join("go.mod");
        let runtime = repo_root.join("runtime").join("go");
        assert!(
            runtime.join("go.mod").exists(),
            "runtime go.mod missing at {}",
            runtime.join("go.mod").display()
        );
        // The tempdir lives at <repo>/target/lazuli-go-smoke/<id>, so a
        // relative replacement avoids Windows drive-letter paths that Go's
        // modfile parser rejects.
        let runtime = Path::new("..")
            .join("..")
            .join("..")
            .join("runtime")
            .join("go");
        let runtime = runtime.to_string_lossy().replace('\\', "/");

        let mut file = OpenOptions::new()
            .append(true)
            .open(&go_mod)
            .unwrap_or_else(|err| panic!("opening {} for append: {err}", go_mod.display()));
        writeln!(file, "\nreplace lazuli.dev/runtime => {runtime}").unwrap_or_else(|err| {
            panic!("writing replace directive to {}: {err}", go_mod.display())
        });
    }

    fn assert_success(command: &str, output: &Output) {
        assert!(
            output.status.success(),
            "{command} failed with status {}\nstdout:\n{}\nstderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
