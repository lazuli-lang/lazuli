#![allow(unexpected_cfgs)]

#[cfg(feature = "smoke_e2e")]
mod templates;

#[cfg(feature = "smoke_e2e")]
mod smoke_e2e {
    use std::env;
    use std::fs::{self, OpenOptions};
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::path::{Path, PathBuf};
    use std::process::{Child, Command, Output, Stdio};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    use super::templates::{ACCOUNT_AUTH_TESTHOOK, MIGRATION_HELPER, SMOKE_SERVER};

    static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

    const DEFAULT_DB_URL: &str = "postgres://lazuli:lazuli@localhost:5432/lazuli?sslmode=disable";
    #[test]
    fn generated_auth_session_cookie_expires_before_protected_request() {
        let repo_root = repo_root();
        let tempdir = TempDir::new(&repo_root);
        let source_dir = tempdir.path().join("source");
        let out_dir = tempdir.path().join("generated");

        copy_dir(
            &repo_root.join("examples").join("marketplace-mini"),
            &source_dir,
        );
        rewrite_session_ttl(&source_dir.join("marketplace-mini.lzi"), "\"2 seconds\"");
        generate_marketplace_mini(&repo_root, &source_dir, &out_dir);
        append_runtime_replace(&repo_root, &out_dir);
        write_smoke_support(&out_dir);

        let db_url = smoke_db_url();
        apply_generated_migrations(&out_dir, &db_url);
        let binary = build_smoke_server(&out_dir);

        let port = unused_local_port();
        let addr = format!("127.0.0.1:{port}");
        let child = Command::new(&binary)
            .current_dir(&out_dir)
            .env("LAZULI_ADDR", &addr)
            .env("LAZULI_DB", &db_url)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap_or_else(|err| panic!("spawning smoke server {}: {err}", binary.display()));
        let mut server = ServerChild::new(child);
        wait_for_server(&mut server, port);

        let signup = post_json(
            port,
            "/signup",
            r#"{"email":"expired-session@example.com","password":"correct horse battery staple","name":"Expired Session"}"#,
            None,
        )
        .expect("POST /signup");
        assert_status(&mut server, "POST /signup", &signup, 201);

        let login = post_json(
            port,
            "/login",
            r#"{"email":"expired-session@example.com","password":"correct horse battery staple"}"#,
            None,
        )
        .expect("POST /login");
        assert_status(&mut server, "POST /login", &login, 200);
        let cookie = login
            .header("set-cookie")
            .unwrap_or_else(|| panic!("login response missing Set-Cookie header:\n{login:?}"))
            .split(';')
            .next()
            .expect("cookie pair")
            .to_owned();

        std::thread::sleep(Duration::from_secs(3));

        let protected = get(port, "/protected", Some(&cookie)).expect("GET /protected");
        assert_eq!(
            protected.status, 401,
            "GET /protected with expired cookie returned status {}, want 401\nbody:\n{}",
            protected.status, protected.body
        );
        assert_eq!(
            protected.body.trim(),
            r#"{"error":"auth.session_expired"}"#,
            "GET /protected expired-session body mismatch"
        );
    }

    fn generate_marketplace_mini(repo_root: &Path, source_dir: &Path, out_dir: &Path) {
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
            ])
            .arg(source_dir)
            .arg("--out")
            .arg(out_dir)
            .output()
            .expect("failed to run `lazuli generate go <patched marketplace-mini>`");
        assert_success("lazuli generate go <patched marketplace-mini>", &generate);
    }

    fn rewrite_session_ttl(path: &Path, ttl: &str) {
        let source = fs::read_to_string(path)
            .unwrap_or_else(|err| panic!("reading {}: {err}", path.display()));
        let patched = source.replace("ttl \"30 days\"", &format!("ttl {ttl}"));
        assert_ne!(
            source, patched,
            "marketplace-mini fixture did not contain the auth sessions ttl"
        );
        fs::write(path, patched).unwrap_or_else(|err| panic!("writing {}: {err}", path.display()));
    }

    fn append_runtime_replace(repo_root: &Path, out_dir: &Path) {
        let go_mod = out_dir.join("go.mod");
        let runtime = repo_root.join("runtime").join("go");
        assert!(runtime.join("go.mod").exists(), "runtime go.mod missing");
        let relative_runtime = pathdiff(&runtime, out_dir).replace('\\', "/");

        let mut file = OpenOptions::new()
            .append(true)
            .open(&go_mod)
            .unwrap_or_else(|err| panic!("opening {} for append: {err}", go_mod.display()));
        writeln!(file, "\nreplace lazuli.dev/runtime => {relative_runtime}")
            .unwrap_or_else(|err| panic!("writing replace directive: {err}"));
    }

    fn write_smoke_support(out_dir: &Path) {
        let account_hook = out_dir.join("account").join("smoke_auth_testhook.go");
        fs::write(&account_hook, ACCOUNT_AUTH_TESTHOOK)
            .unwrap_or_else(|err| panic!("writing {}: {err}", account_hook.display()));

        let cmd_dir = out_dir.join("cmd").join("auth_expired_smoke");
        fs::create_dir_all(&cmd_dir)
            .unwrap_or_else(|err| panic!("creating {}: {err}", cmd_dir.display()));
        fs::write(cmd_dir.join("main.go"), SMOKE_SERVER)
            .unwrap_or_else(|err| panic!("writing smoke server: {err}"));
    }

    fn apply_generated_migrations(out_dir: &Path, db_url: &str) {
        let helper_dir = out_dir.join("smoke_migrations");
        fs::create_dir_all(&helper_dir)
            .unwrap_or_else(|err| panic!("creating {}: {err}", helper_dir.display()));
        fs::write(helper_dir.join("main.go"), MIGRATION_HELPER)
            .unwrap_or_else(|err| panic!("writing migration helper: {err}"));

        let migrate = Command::new("go")
            .current_dir(out_dir)
            .env("GOFLAGS", "-mod=mod")
            .env("LAZULI_DB", db_url)
            .env("LAZULI_MIGRATIONS", out_dir.join("migrations"))
            .args(["run", "./smoke_migrations"])
            .output()
            .expect("failed to run migration helper");
        assert_success("go run ./smoke_migrations", &migrate);
    }

    fn build_smoke_server(out_dir: &Path) -> PathBuf {
        let binary = out_dir.join(format!("auth-expired-smoke{}", env::consts::EXE_SUFFIX));
        let build = Command::new("go")
            .current_dir(out_dir)
            .env("GOFLAGS", "-mod=mod")
            .args(["build", "-o"])
            .arg(&binary)
            .arg("./cmd/auth_expired_smoke")
            .output()
            .expect("failed to run `go build`");
        assert_success("go build ./cmd/auth_expired_smoke", &build);
        binary
    }

    fn wait_for_server(server: &mut ServerChild, port: u16) {
        let started = Instant::now();
        while started.elapsed() < Duration::from_secs(20) {
            if let Some(status) = server.has_exited() {
                let output = server.terminate_and_output();
                panic!("generated server exited before readiness with {status}\n{output}");
            }
            if let Ok(response) = get(port, "/healthz", None) {
                if response.status == 200 {
                    return;
                }
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let output = server.terminate_and_output();
        panic!("generated server did not become ready within 20s\n{output}");
    }

    fn get(port: u16, path: &str, cookie: Option<&str>) -> std::io::Result<HttpResponse> {
        http_request(port, "GET", path, None, cookie)
    }

    fn post_json(
        port: u16,
        path: &str,
        body: &str,
        cookie: Option<&str>,
    ) -> std::io::Result<HttpResponse> {
        http_request(port, "POST", path, Some(body), cookie)
    }

    fn http_request(
        port: u16,
        method: &str,
        path: &str,
        body: Option<&str>,
        cookie: Option<&str>,
    ) -> std::io::Result<HttpResponse> {
        let mut stream = TcpStream::connect_timeout(
            &format!("127.0.0.1:{port}")
                .parse()
                .expect("valid socket addr"),
            Duration::from_secs(2),
        )?;
        stream.set_read_timeout(Some(Duration::from_secs(5)))?;
        stream.set_write_timeout(Some(Duration::from_secs(5)))?;

        let body = body.unwrap_or("");
        write!(
            stream,
            "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n",
            body.len(),
        )?;
        if let Some(cookie) = cookie {
            write!(stream, "Cookie: {cookie}\r\n")?;
        }
        write!(stream, "\r\n{body}")?;

        let mut raw = Vec::new();
        stream.read_to_end(&mut raw)?;
        parse_http_response(&raw)
    }

    fn parse_http_response(raw: &[u8]) -> std::io::Result<HttpResponse> {
        let response = String::from_utf8_lossy(raw);
        let (head, body) = response.split_once("\r\n\r\n").ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "HTTP response missing headers",
            )
        })?;
        let status = head
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|status| status.parse::<u16>().ok())
            .ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "HTTP response missing status",
                )
            })?;
        let mut headers = Vec::new();
        for line in head.lines().skip(1) {
            if let Some((name, value)) = line.split_once(':') {
                headers.push((name.trim().to_ascii_lowercase(), value.trim().to_owned()));
            }
        }
        Ok(HttpResponse {
            status,
            headers,
            body: body.to_owned(),
        })
    }

    fn assert_status(server: &mut ServerChild, label: &str, response: &HttpResponse, want: u16) {
        if response.status != want {
            let output = server.terminate_and_output();
            panic!(
                "{label} returned status {}, want {want}\nbody:\n{}\nserver output:\n{}",
                response.status, response.body, output
            );
        }
    }

    fn copy_dir(src: &Path, dst: &Path) {
        fs::create_dir_all(dst).unwrap_or_else(|err| panic!("creating {}: {err}", dst.display()));
        for entry in
            fs::read_dir(src).unwrap_or_else(|err| panic!("reading {}: {err}", src.display()))
        {
            let entry = entry.unwrap_or_else(|err| panic!("reading dir entry: {err}"));
            let src_path = entry.path();
            let dst_path = dst.join(entry.file_name());
            if src_path.is_dir() {
                copy_dir(&src_path, &dst_path);
            } else {
                fs::copy(&src_path, &dst_path).unwrap_or_else(|err| {
                    panic!(
                        "copying {} to {}: {err}",
                        src_path.display(),
                        dst_path.display()
                    )
                });
            }
        }
    }

    fn pathdiff(path: &Path, base: &Path) -> String {
        let path = path.components().collect::<Vec<_>>();
        let base = base.components().collect::<Vec<_>>();
        let mut common = 0;
        while common < path.len() && common < base.len() && path[common] == base[common] {
            common += 1;
        }
        let mut out = PathBuf::new();
        for _ in common..base.len() {
            out.push("..");
        }
        for component in &path[common..] {
            out.push(component.as_os_str());
        }
        out.to_string_lossy().into_owned()
    }

    fn unused_local_port() -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").expect("binding ephemeral port");
        listener
            .local_addr()
            .expect("reading ephemeral socket address")
            .port()
    }

    fn smoke_db_url() -> String {
        env::var("LAZULI_SMOKE_DB")
            .or_else(|_| env::var("LAZULI_DB"))
            .unwrap_or_else(|_| DEFAULT_DB_URL.to_owned())
    }

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .expect("crate should live under <repo>/crates/lazuli_codegen_go")
            .to_path_buf()
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

    #[derive(Debug)]
    struct HttpResponse {
        status: u16,
        headers: Vec<(String, String)>,
        body: String,
    }

    impl HttpResponse {
        fn header(&self, name: &str) -> Option<&str> {
            let name = name.to_ascii_lowercase();
            self.headers
                .iter()
                .find(|(candidate, _)| candidate == &name)
                .map(|(_, value)| value.as_str())
        }
    }

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
                .join("lazuli-go-smoke-auth-expired")
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

    struct ServerChild {
        child: Option<Child>,
    }

    impl ServerChild {
        fn new(child: Child) -> Self {
            Self { child: Some(child) }
        }

        fn has_exited(&mut self) -> Option<std::process::ExitStatus> {
            self.child
                .as_mut()
                .and_then(|child| child.try_wait().expect("checking server process"))
        }

        fn terminate_and_output(&mut self) -> String {
            let Some(mut child) = self.child.take() else {
                return String::new();
            };
            let _ = child.kill();
            match child.wait_with_output() {
                Ok(output) => format!(
                    "stdout:\n{}\nstderr:\n{}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                ),
                Err(err) => format!("failed to collect server output: {err}"),
            }
        }
    }

    impl Drop for ServerChild {
        fn drop(&mut self) {
            if let Some(mut child) = self.child.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }
}
