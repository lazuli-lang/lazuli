#[cfg(feature = "smoke_e2e")]
mod smoke_e2e {
    use std::env;
    use std::fs::{self, OpenOptions};
    use std::io::{Read, Write};
    use std::net::{SocketAddr, TcpListener, TcpStream};
    use std::path::{Path, PathBuf};
    use std::process::{Child, Command, ExitStatus, Output, Stdio};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::thread::{self, JoinHandle};
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

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
                .join("lazuli-go-smoke-binary")
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

    struct RunningServer {
        child: Child,
        stdout: Option<JoinHandle<String>>,
        stderr: Option<JoinHandle<String>>,
    }

    struct ServerOutput {
        status: ExitStatus,
        stdout: String,
        stderr: String,
    }

    impl RunningServer {
        fn spawn(binary: &Path, work_dir: &Path, port: u16) -> Self {
            let mut child = Command::new(binary)
                .current_dir(work_dir)
                .env("LAZULI_PORT", port.to_string())
                .env("LAZULI_DB", "")
                .env_remove("LAZULI_ADDR")
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap_or_else(|err| panic!("spawning {}: {err}", binary.display()));

            let stdout = child
                .stdout
                .take()
                .map(|pipe| thread::spawn(move || read_pipe(pipe)));
            let stderr = child
                .stderr
                .take()
                .map(|pipe| thread::spawn(move || read_pipe(pipe)));

            Self {
                child,
                stdout,
                stderr,
            }
        }

        fn try_wait(&mut self) -> Option<ExitStatus> {
            self.child
                .try_wait()
                .unwrap_or_else(|err| panic!("checking server process status: {err}"))
        }

        fn shutdown_and_collect(&mut self) -> ServerOutput {
            if self.try_wait().is_none() {
                request_process_shutdown(&mut self.child);
            }

            let status = match wait_for_exit(&mut self.child, Duration::from_secs(10)) {
                Ok(status) => status,
                Err(_) => {
                    let _ = self.child.kill();
                    self.child
                        .wait()
                        .unwrap_or_else(|err| panic!("waiting for killed server process: {err}"))
                }
            };

            ServerOutput {
                status,
                stdout: join_pipe(&mut self.stdout),
                stderr: join_pipe(&mut self.stderr),
            }
        }

        fn kill_and_collect(&mut self) -> ServerOutput {
            if self.try_wait().is_none() {
                let _ = self.child.kill();
            }
            let status = self
                .child
                .wait()
                .unwrap_or_else(|err| panic!("waiting for server process: {err}"));
            ServerOutput {
                status,
                stdout: join_pipe(&mut self.stdout),
                stderr: join_pipe(&mut self.stderr),
            }
        }
    }

    impl Drop for RunningServer {
        fn drop(&mut self) {
            if self.try_wait().is_none() {
                let _ = self.child.kill();
                let _ = self.child.wait();
            }
            let _ = self.stdout.take().map(|handle| handle.join());
            let _ = self.stderr.take().map(|handle| handle.join());
        }
    }

    #[test]
    fn marketplace_mini_binary_starts_and_serves_healthz() {
        let repo_root = repo_root();
        let tempdir = TempDir::new(&repo_root);

        generate_marketplace_mini(&repo_root, tempdir.path());
        append_runtime_replace(&repo_root, tempdir.path());
        go_mod_tidy(tempdir.path());
        go_build_all(tempdir.path());
        let server_binary = go_build_server(tempdir.path());

        let port = probe_port();
        let mut server = RunningServer::spawn(&server_binary, tempdir.path(), port);

        if let Err(err) = wait_for_tcp(&mut server, port, Duration::from_secs(30)) {
            let output = server.kill_and_collect();
            panic!(
                "{err}\nserver status: {}\nstdout:\n{}\nstderr:\n{}",
                output.status, output.stdout, output.stderr
            );
        }

        assert_healthz(port);
        let _ = server.shutdown_and_collect();
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

    fn append_runtime_replace(repo_root: &Path, out_dir: &Path) {
        let go_mod = out_dir.join("go.mod");
        let runtime = repo_root.join("runtime").join("go");
        assert!(
            runtime.join("go.mod").exists(),
            "runtime go.mod missing at {}",
            runtime.join("go.mod").display()
        );
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

    fn go_mod_tidy(out_dir: &Path) {
        let tidy = Command::new("go")
            .current_dir(out_dir)
            .args(["mod", "tidy"])
            .output()
            .expect("failed to run `go mod tidy`");
        assert_success("go mod tidy", &tidy);
    }

    fn go_build_all(out_dir: &Path) {
        let build = Command::new("go")
            .current_dir(out_dir)
            .args(["build", "./..."])
            .output()
            .expect("failed to run `go build ./...`");
        assert_success("go build ./...", &build);
    }

    fn go_build_server(out_dir: &Path) -> PathBuf {
        let server = out_dir.join(format!("server{}", env::consts::EXE_SUFFIX));
        let build = Command::new("go")
            .current_dir(out_dir)
            .args(["build", "-o"])
            .arg(&server)
            .arg(".")
            .output()
            .expect("failed to run `go build -o <server> .`");
        assert_success("go build -o <server> .", &build);
        server
    }

    fn wait_for_tcp(
        server: &mut RunningServer,
        port: u16,
        timeout: Duration,
    ) -> Result<(), String> {
        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        let deadline = Instant::now() + timeout;
        let mut last_error = None;

        while Instant::now() < deadline {
            if let Some(status) = server.try_wait() {
                return Err(format!(
                    "server exited before accepting TCP connections on {addr}: {status}"
                ));
            }

            match TcpStream::connect_timeout(&addr, Duration::from_millis(250)) {
                Ok(_) => return Ok(()),
                Err(err) => last_error = Some(err),
            }

            thread::sleep(Duration::from_millis(100));
        }

        Err(format!(
            "timed out waiting for server to accept TCP connections on {addr}; last error: {}",
            last_error
                .map(|err| err.to_string())
                .unwrap_or_else(|| "no connection attempt made".to_owned())
        ))
    }

    fn assert_healthz(port: u16) {
        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(5))
            .unwrap_or_else(|err| panic!("connecting to health endpoint on {addr}: {err}"));
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .expect("setting health response read timeout");
        stream
            .write_all(b"GET /healthz HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
            .expect("writing health request");

        let mut response = String::new();
        stream
            .read_to_string(&mut response)
            .expect("reading health response");
        let status = response.lines().next().unwrap_or("<empty response>");
        assert!(
            status.starts_with("HTTP/1.1 200 ") || status.starts_with("HTTP/1.0 200 "),
            "GET /healthz returned unexpected response status `{status}`\nresponse:\n{response}"
        );
    }

    fn probe_port() -> u16 {
        if let Ok(listener) = TcpListener::bind(("127.0.0.1", 18765)) {
            let port = listener
                .local_addr()
                .expect("reading preferred probe port")
                .port();
            drop(listener);
            return port;
        }

        let listener =
            TcpListener::bind(("127.0.0.1", 0)).expect("binding random probe port on localhost");
        let port = listener
            .local_addr()
            .expect("reading random probe port")
            .port();
        drop(listener);
        port
    }

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .expect("crate should live under <repo>/crates/lazuli_codegen_go")
            .to_path_buf()
    }

    fn wait_for_exit(child: &mut Child, timeout: Duration) -> Result<ExitStatus, ()> {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if let Some(status) = child
                .try_wait()
                .unwrap_or_else(|err| panic!("checking server process status: {err}"))
            {
                return Ok(status);
            }
            thread::sleep(Duration::from_millis(100));
        }
        Err(())
    }

    fn request_process_shutdown(child: &mut Child) {
        #[cfg(unix)]
        {
            let status = Command::new("kill")
                .arg("-INT")
                .arg(child.id().to_string())
                .status()
                .expect("failed to run `kill -INT`");
            assert!(status.success(), "`kill -INT` failed with status {status}");
        }

        #[cfg(windows)]
        {
            child.kill().expect("failed to kill server process");
        }
    }

    fn read_pipe<R: Read>(mut pipe: R) -> String {
        let mut bytes = Vec::new();
        let _ = pipe.read_to_end(&mut bytes);
        String::from_utf8_lossy(&bytes).into_owned()
    }

    fn join_pipe(handle: &mut Option<JoinHandle<String>>) -> String {
        handle
            .take()
            .map(|handle| handle.join().expect("joining server output reader"))
            .unwrap_or_default()
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
