#![allow(unexpected_cfgs)]

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

    static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

    const DEFAULT_DB_URL: &str = "postgres://lazuli:lazuli@localhost:5432/lazuli?sslmode=disable";
    const SESSION_COOKIE: &str = "lazuli_session";
    const SMOKE_PASSWORD: &str = "correct horse battery staple";

    #[test]
    fn marketplace_mini_logout_clears_cookie_and_invalidates_session() {
        let repo_root = repo_root();
        let tempdir = TempDir::new(&repo_root);

        generate_marketplace_mini(&repo_root, tempdir.path());
        append_runtime_replace(&repo_root, tempdir.path());
        let binary = build_marketplace_mini_binary(tempdir.path());

        let db_url = smoke_db_url();
        apply_generated_migrations(tempdir.path(), &db_url);

        let port = unused_local_port();
        let addr = format!("127.0.0.1:{port}");
        let child = Command::new(&binary)
            .current_dir(tempdir.path())
            .env("LAZULI_ADDR", &addr)
            .env("LAZULI_DB", &db_url)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap_or_else(|err| panic!("spawning generated server {}: {err}", binary.display()));
        let mut server = ServerChild::new(child);
        wait_for_server(&mut server, port);

        let email = unique_email();
        let signup_body =
            format!(r#"{{"email":"{email}","password":"{SMOKE_PASSWORD}","name":"Logout Smoke"}}"#);
        assert_status(
            &mut server,
            post_json(port, "/auth/signup", &signup_body, None).expect("POST /auth/signup"),
            200,
            "POST /auth/signup",
        );

        let login_body = format!(r#"{{"email":"{email}","password":"{SMOKE_PASSWORD}"}}"#);
        let login = post_json(port, "/auth/login", &login_body, None).expect("POST /auth/login");
        assert_response_status(&mut server, &login, 200, "POST /auth/login");
        let session_cookie = session_cookie_value(&login).unwrap_or_else(|| {
            panic!("POST /auth/login did not set {SESSION_COOKIE}; response:\n{login:?}")
        });

        let cookie_header = format!("{SESSION_COOKIE}={session_cookie}");
        let logout =
            post_json(port, "/auth/logout", "{}", Some(&cookie_header)).expect("POST /auth/logout");
        assert_response_status(&mut server, &logout, 200, "POST /auth/logout");
        assert_clears_session_cookie(&logout);

        let stale = post_json(port, "/auth/logout", "{}", Some(&cookie_header))
            .expect("POST /auth/logout with stale cookie");
        assert_response_status(
            &mut server,
            &stale,
            401,
            "POST /auth/logout with stale cookie",
        );
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

    fn build_marketplace_mini_binary(out_dir: &Path) -> PathBuf {
        let binary = out_dir.join(format!(
            "marketplace-mini-auth-logout{}",
            env::consts::EXE_SUFFIX
        ));
        let build = Command::new("go")
            .current_dir(out_dir)
            .env("GOFLAGS", "-mod=mod")
            .args(["build", "-o"])
            .arg(&binary)
            .arg(".")
            .output()
            .expect("failed to run `go build`");
        assert_success("go build -o marketplace-mini-auth-logout .", &build);
        binary
    }

    fn apply_generated_migrations(out_dir: &Path, db_url: &str) {
        let helper_dir = out_dir.join("smoke_migrations");
        fs::create_dir_all(&helper_dir)
            .unwrap_or_else(|err| panic!("creating {}: {err}", helper_dir.display()));
        fs::write(helper_dir.join("main.go"), MIGRATION_HELPER)
            .unwrap_or_else(|err| panic!("writing migration helper: {err}"));

        let migrations_dir = out_dir.join("migrations");
        let migrate = Command::new("go")
            .current_dir(out_dir)
            .env("GOFLAGS", "-mod=mod")
            .env("LAZULI_DB", db_url)
            .env("LAZULI_MIGRATIONS", &migrations_dir)
            .args(["run", "./smoke_migrations"])
            .output()
            .expect("failed to run migration helper");
        assert_success("go run ./smoke_migrations", &migrate);
    }

    fn wait_for_server(server: &mut ServerChild, port: u16) {
        let started = Instant::now();
        while started.elapsed() < Duration::from_secs(20) {
            if let Some(status) = server.has_exited() {
                let output = server.terminate_and_output();
                panic!("generated server exited before readiness with {status}\n{output}");
            }
            if let Ok(response) = get(port, "/healthz") {
                if response.status == 200 {
                    return;
                }
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let output = server.terminate_and_output();
        panic!("generated server did not become ready within 20s\n{output}");
    }

    fn get(port: u16, path: &str) -> std::io::Result<HttpResponse> {
        http_request(port, "GET", path, None, None)
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
        let cookie = cookie
            .map(|value| format!("Cookie: {value}\r\n"))
            .unwrap_or_default();
        write!(
            stream,
            "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\nContent-Type: application/json\r\n{cookie}Content-Length: {}\r\n\r\n{}",
            body.len(),
            body
        )?;

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
        let headers = head.lines().skip(1).map(str::to_owned).collect();
        Ok(HttpResponse {
            status,
            headers,
            body: body.to_owned(),
        })
    }

    fn session_cookie_value(response: &HttpResponse) -> Option<String> {
        set_cookie_headers(response)
            .find_map(|header| cookie_value(header, SESSION_COOKIE).map(str::to_owned))
    }

    fn assert_clears_session_cookie(response: &HttpResponse) {
        let clear = set_cookie_headers(response)
            .find(|header| header.starts_with(&format!("{SESSION_COOKIE}=")))
            .unwrap_or_else(|| {
                panic!(
                    "POST /auth/logout did not return Set-Cookie for {SESSION_COOKIE}; headers: {:?}",
                    response.headers
                )
            });
        assert!(
            clear.starts_with(&format!("{SESSION_COOKIE}=;")),
            "Set-Cookie did not clear {SESSION_COOKIE}; got {clear:?}"
        );
        assert!(
            clear
                .split(';')
                .any(|part| part.trim().eq_ignore_ascii_case("Max-Age=0")),
            "Set-Cookie did not include Max-Age=0; got {clear:?}"
        );
    }

    fn set_cookie_headers(response: &HttpResponse) -> impl Iterator<Item = &str> {
        response.headers.iter().filter_map(|header| {
            let (name, value) = header.split_once(':')?;
            if name.eq_ignore_ascii_case("set-cookie") {
                Some(value.trim())
            } else {
                None
            }
        })
    }

    fn cookie_value<'a>(set_cookie: &'a str, name: &str) -> Option<&'a str> {
        let rest = set_cookie.strip_prefix(&format!("{name}="))?;
        let end = rest.find(';').unwrap_or(rest.len());
        let value = &rest[..end];
        if value.is_empty() {
            None
        } else {
            Some(value)
        }
    }

    fn assert_status(server: &mut ServerChild, response: HttpResponse, want: u16, label: &str) {
        assert_response_status(server, &response, want, label);
    }

    fn assert_response_status(
        server: &mut ServerChild,
        response: &HttpResponse,
        want: u16,
        label: &str,
    ) {
        if response.status != want {
            let output = server.terminate_and_output();
            panic!(
                "{label} returned status {}, want {want}\nheaders:\n{}\nbody:\n{}\nserver output:\n{}",
                response.status,
                response.headers.join("\n"),
                response.body,
                output
            );
        }
    }

    fn unused_local_port() -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").expect("binding ephemeral port");
        listener
            .local_addr()
            .expect("reading ephemeral socket address")
            .port()
    }

    fn unique_email() -> String {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before UNIX_EPOCH")
            .as_nanos();
        format!("logout-{nonce}@example.com")
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

    fn assert_success(command: &str, output: &Output) {
        assert!(
            output.status.success(),
            "{command} failed with status {}\nstdout:\n{}\nstderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn process_output(output: &Output) -> String {
        format!(
            "stdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
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
                .join("lazuli-go-smoke-auth-logout")
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
                Ok(output) => process_output(&output),
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

    #[derive(Debug)]
    struct HttpResponse {
        status: u16,
        headers: Vec<String>,
        body: String,
    }

    const MIGRATION_HELPER: &str = r#"package main

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"time"

	"github.com/jackc/pgx/v5"
)

func main() {
	dbURL := os.Getenv("LAZULI_DB")
	if dbURL == "" {
		panic("LAZULI_DB is required")
	}
	migrationsDir := os.Getenv("LAZULI_MIGRATIONS")
	if migrationsDir == "" {
		panic("LAZULI_MIGRATIONS is required")
	}

	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()

	conn, err := pgx.Connect(ctx, dbURL)
	if err != nil {
		panic(fmt.Sprintf("connect postgres: %v", err))
	}
	defer conn.Close(context.Background())

	entries, err := os.ReadDir(migrationsDir)
	if err != nil {
		panic(fmt.Sprintf("read migrations: %v", err))
	}

	files := make([]string, 0, len(entries))
	for _, entry := range entries {
		name := entry.Name()
		if entry.IsDir() || !strings.HasSuffix(name, ".sql") || strings.HasSuffix(name, ".down.sql") {
			continue
		}
		files = append(files, filepath.Join(migrationsDir, name))
	}
	sort.Strings(files)

	for _, file := range files {
		sql, err := os.ReadFile(file)
		if err != nil {
			panic(fmt.Sprintf("read %s: %v", file, err))
		}
		if _, err := conn.Exec(ctx, string(sql)); err != nil {
			panic(fmt.Sprintf("apply %s: %v", file, err))
		}
	}
}
"#;
}
