#[cfg(feature = "smoke")]
mod smoke {
    use std::env;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Output};
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(repo_root: &Path) -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time before UNIX_EPOCH")
                .as_nanos();
            let path = repo_root
                .join("target")
                .join("lazuli-ts-smoke")
                .join(format!("{}-{nonce}", std::process::id()));
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
    fn full_capsule_typechecks_under_tsc() {
        if env::var("LAZULI_TS_SMOKE").ok().as_deref() != Some("1") {
            eprintln!("skipping TS smoke test; set LAZULI_TS_SMOKE=1 to run");
            return;
        }

        let repo_root = repo_root();
        let tempdir = TempDir::new(&repo_root);

        let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
        let generate = Command::new(cargo)
            .current_dir(&repo_root)
            .args([
                "run",
                "-q",
                "-p",
                "lazuli_cli",
                "--bin",
                "lazuli",
                "--",
                "generate",
                "ts",
                "examples/full-capsule",
                "--output",
            ])
            .arg(tempdir.path())
            .output()
            .expect("failed to run `cargo run -p lazuli_cli --bin lazuli -- generate ts`");
        assert_success("lazuli generate ts examples/full-capsule", &generate);

        write_tsconfig(&repo_root, tempdir.path());
        write_zod_shim(tempdir.path());

        let mut tsc = tsc_command();
        let typecheck = tsc
            .current_dir(tempdir.path())
            .arg("--noEmit")
            .output()
            .expect("failed to run `tsc --noEmit`; install TypeScript 5+ or make tsc/npx available");
        assert_success("tsc --noEmit", &typecheck);
    }

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .expect("crate should live under <repo>/crates/lazuli_codegen_ts")
            .to_path_buf()
    }

    fn write_tsconfig(repo_root: &Path, out_dir: &Path) {
        let runtime_index = repo_root
            .join("runtime")
            .join("web")
            .join("lazuli")
            .join("src")
            .join("index.ts");
        assert!(
            runtime_index.exists(),
            "runtime web entrypoint missing at {}",
            runtime_index.display()
        );
        let runtime_index = json_path(&runtime_index);

        let tsconfig = format!(
            r#"{{
  "compilerOptions": {{
    "strict": true,
    "exactOptionalPropertyTypes": true,
    "moduleResolution": "node",
    "target": "ES2020",
    "module": "ESNext",
    "paths": {{
      "@lazuli/runtime": ["{runtime_index}"],
      "zod": ["./zod-smoke-shim.d.ts"]
    }}
  }},
  "include": ["**/*.gen.ts", "**/*.zod.ts"]
}}
"#
        );
        let path = out_dir.join("tsconfig.json");
        fs::write(&path, tsconfig)
            .unwrap_or_else(|err| panic!("writing {}: {err}", path.display()));
    }

    fn write_zod_shim(out_dir: &Path) {
        let shim = r#"declare module "zod" {
  export const z: any;
}
"#;
        let path = out_dir.join("zod-smoke-shim.d.ts");
        fs::write(&path, shim).unwrap_or_else(|err| panic!("writing {}: {err}", path.display()));
    }

    fn tsc_command() -> Command {
        if command_succeeds("tsc", &["--version"]) {
            return Command::new("tsc");
        }
        if cfg!(windows) && command_succeeds("tsc.cmd", &["--version"]) {
            return Command::new("tsc.cmd");
        }
        if command_succeeds("npx", &["tsc", "--version"]) {
            let mut command = Command::new("npx");
            command.arg("tsc");
            return command;
        }
        if cfg!(windows) && command_succeeds("npx.cmd", &["tsc", "--version"]) {
            let mut command = Command::new("npx.cmd");
            command.arg("tsc");
            return command;
        }
        panic!("TypeScript smoke test requires tsc 5+ on PATH, or npx that can run `npx tsc`");
    }

    fn command_succeeds(program: &str, args: &[&str]) -> bool {
        Command::new(program)
            .args(args)
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    fn json_path(path: &Path) -> String {
        path.to_string_lossy().replace('\\', "/")
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
