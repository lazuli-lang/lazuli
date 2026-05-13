#![allow(unexpected_cfgs)]

#[cfg(any(feature = "smoke_e2e", smoke_e2e))]
mod smoke_e2e {
    use std::env;
    use std::fs::{self, OpenOptions};
    use std::io::{self, Read, Write};
    use std::net::{SocketAddr, TcpStream};
    use std::path::{Path, PathBuf};
    use std::process::{Child, Command, Output, Stdio};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::thread;
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    const SMOKE_PORT: u16 = 18766;
    const HEALTH_WAIT: Duration = Duration::from_secs(30);
    const SHUTDOWN_WAIT: Duration = Duration::from_secs(5);
    const CONNECT_TIMEOUT: Duration = Duration::from_millis(200);
    const POLL_INTERVAL: Duration = Duration::from_millis(100);

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
                .join("lazuli-go-smoke-shutdown")
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

    struct ChildGuard {
        child: Option<Child>,
    }

    impl ChildGuard {
        fn new(child: Child) -> Self {
            Self { child: Some(child) }
        }

        fn child_mut(&mut self) -> &mut Child {
            self.child.as_mut().expect("child already taken")
        }
    }

    impl Drop for ChildGuard {
        fn drop(&mut self) {
            let Some(mut child) = self.child.take() else {
                return;
            };
            if matches!(child.try_wait(), Ok(Some(_))) {
                return;
            }
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    #[test]
    fn marketplace_mini_releases_port_after_shutdown_signal() {
        let repo_root = repo_root();
        let tempdir = TempDir::new(&repo_root);

        assert_port_refused(SMOKE_PORT, "before starting marketplace-mini smoke server");

        generate_marketplace_mini(&repo_root, tempdir.path());
        append_runtime_replace(&repo_root, tempdir.path());

        let binary = tempdir.path().join(binary_name());
        build_server(tempdir.path(), &binary);

        let log_path = tempdir.path().join("server.log");
        let mut child = ChildGuard::new(spawn_server(tempdir.path(), &binary, &log_path));

        wait_for_healthz_200(child.child_mut(), &log_path);
        request_shutdown(child.child_mut());
        wait_for_port_refused(SMOKE_PORT);
    }

    fn generate_marketplace_mini(repo_root: &Path, out_dir: &Path) {
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
                "examples/marketplace-mini",
                "--out",
            ])
            .arg(out_dir)
            .output()
            .expect("failed to run `cargo run -p lazuli_cli --bin lazuli -- generate go`");
        assert_success("lazuli generate go examples/marketplace-mini", &generate);
    }

    fn build_server(out_dir: &Path, binary: &Path) {
        let build = Command::new("go")
            .current_dir(out_dir)
            .env("GOFLAGS", "-mod=mod")
            .args(["build", "-o"])
            .arg(binary)
            .arg(".")
            .output()
            .expect("failed to run `go build`");
        assert_success("go build -o <marketplace-mini> .", &build);
    }

    fn spawn_server(out_dir: &Path, binary: &Path, log_path: &Path) -> Child {
        let log = OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_path)
            .unwrap_or_else(|err| panic!("opening {}: {err}", log_path.display()));
        let err_log = log
            .try_clone()
            .unwrap_or_else(|err| panic!("cloning {}: {err}", log_path.display()));

        Command::new(binary)
            .current_dir(out_dir)
            .env("PORT", SMOKE_PORT.to_string())
            .stdout(Stdio::from(log))
            .stderr(Stdio::from(err_log))
            .spawn()
            .unwrap_or_else(|err| panic!("spawning {}: {err}", binary.display()))
    }

    fn wait_for_healthz_200(child: &mut Child, log_path: &Path) {
        let start = Instant::now();
        loop {
            if let Some(status) = child
                .try_wait()
                .expect("checking marketplace-mini server status")
            {
                panic!(
                    "marketplace-mini server exited before /healthz reached 200: {status}\nserver log:\n{}",
                    read_log(log_path)
                );
            }

            if healthz_is_200(SMOKE_PORT).unwrap_or(false) {
                return;
            }

            if start.elapsed() >= HEALTH_WAIT {
                panic!(
                    "timed out after {:?} waiting for /healthz to return 200 on port {SMOKE_PORT}\nserver log:\n{}",
                    HEALTH_WAIT,
                    read_log(log_path)
                );
            }

            thread::sleep(POLL_INTERVAL);
        }
    }

    fn healthz_is_200(port: u16) -> io::Result<bool> {
        let mut stream = connect(port)?;
        stream.set_read_timeout(Some(CONNECT_TIMEOUT))?;
        stream.set_write_timeout(Some(CONNECT_TIMEOUT))?;
        stream
            .write_all(b"GET /healthz HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")?;

        let mut response = [0_u8; 64];
        let n = stream.read(&mut response)?;
        let status = String::from_utf8_lossy(&response[..n]);
        Ok(status.starts_with("HTTP/1.1 200") || status.starts_with("HTTP/1.0 200"))
    }

    fn request_shutdown(child: &mut Child) {
        #[cfg(unix)]
        {
            let status = Command::new("kill")
                .arg("-INT")
                .arg(child.id().to_string())
                .status()
                .expect("failed to send SIGINT with kill");
            assert!(
                status.success(),
                "failed to send SIGINT to marketplace-mini server process {}: {status}",
                child.id()
            );
        }

        #[cfg(windows)]
        {
            child
                .kill()
                .expect("failed to terminate marketplace-mini server process");
        }
    }

    fn wait_for_port_refused(port: u16) {
        let start = Instant::now();
        loop {
            match connect(port) {
                Ok(stream) => {
                    drop(stream);
                    if start.elapsed() >= SHUTDOWN_WAIT {
                        panic!(
                            "port {port} is still accepting TCP connections after {:?}; missing graceful shutdown",
                            SHUTDOWN_WAIT
                        );
                    }
                }
                Err(err) if err.kind() == io::ErrorKind::ConnectionRefused => return,
                Err(err) => {
                    if start.elapsed() >= SHUTDOWN_WAIT {
                        panic!(
                            "port {port} did not report TCP connection refused within {:?} after shutdown signal; last error: {err}; missing graceful shutdown",
                            SHUTDOWN_WAIT
                        );
                    }
                }
            }

            thread::sleep(POLL_INTERVAL);
        }
    }

    fn assert_port_refused(port: u16, context: &str) {
        match connect(port) {
            Err(err) if err.kind() == io::ErrorKind::ConnectionRefused => {}
            Err(_) => {}
            Ok(stream) => {
                drop(stream);
                panic!("port {port} is already accepting TCP connections {context}");
            }
        }
    }

    fn connect(port: u16) -> io::Result<TcpStream> {
        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT)
    }

    fn binary_name() -> &'static str {
        if cfg!(windows) {
            "marketplace-mini-smoke.exe"
        } else {
            "marketplace-mini-smoke"
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
        // The tempdir lives at <repo>/target/lazuli-go-smoke-shutdown/<id>, so
        // a relative replacement avoids Windows drive-letter paths that Go's
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

    fn read_log(path: &Path) -> String {
        fs::read_to_string(path).unwrap_or_else(|err| format!("<failed to read server log: {err}>"))
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
