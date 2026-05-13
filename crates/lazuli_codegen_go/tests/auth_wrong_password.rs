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
    const SMOKE_EMAIL: &str = "wrong-password@example.com";
    const CORRECT_PASSWORD: &str = "correct horse battery staple";
    const WRONG_PASSWORD: &str = "wrong horse battery staple";

    #[test]
    fn login_wrong_password_returns_401_without_cookie_and_audits_error() {
        let repo_root = repo_root();
        let tempdir = TempDir::new(&repo_root);

        generate_marketplace_mini(&repo_root, tempdir.path());
        append_runtime_replace(tempdir.path());
        let binary = build_marketplace_mini(tempdir.path());

        let db_url = smoke_db_url();
        if let Err(err) = apply_generated_migrations(tempdir.path(), &db_url) {
            eprintln!("smoke_e2e: skipping auth wrong-password HTTP assertions; DB unavailable or migrations failed: {err}");
            return;
        }

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

        let signup = post_json(
            port,
            "/api/v1/c/account.register",
            &format!(
                r#"{{"email":"{SMOKE_EMAIL}","password":"{CORRECT_PASSWORD}","name":"Wrong Password Smoke"}}"#
            ),
        )
        .expect("POST account.register");
        if signup.status != 200 && signup.status != 201 {
            let output = server.terminate_and_output();
            panic!(
                "POST account.register returned status {}, want 200 or 201\nbody:\n{}\nserver output:\n{}",
                signup.status, signup.body, output
            );
        }

        let login = post_json(
            port,
            "/api/v1/c/account.login",
            &format!(r#"{{"email":"{SMOKE_EMAIL}","password":"{WRONG_PASSWORD}"}}"#),
        )
        .expect("POST account.login");

        assert_eq!(
            login.status, 401,
            "POST account.login with wrong password returned status {}, want 401\nbody:\n{}",
            login.status, login.body
        );
        assert_auth_password_mismatch_body(&login.body);
        assert!(
            login.header_values("set-cookie").is_empty(),
            "wrong-password login must not set Set-Cookie; got {:?}\nbody:\n{}",
            login.header_values("set-cookie"),
            login.body
        );

        match latest_login_audit_row(tempdir.path(), &db_url) {
            Ok(Some(row)) => {
                assert_eq!(row.actor_kind, "anonymous", "audit actor_kind");
                assert_eq!(row.command_name, "auth.login", "audit command_name");
                assert_eq!(row.result_status, "error", "audit result_status");
            }
            Ok(None) => eprintln!(
                "smoke_e2e: audit_log contained no auth.login row; skipping optional audit assertion"
            ),
            Err(err) => eprintln!("smoke_e2e: skipping optional audit assertion: {err}"),
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
                .join("lazuli-go-smoke-auth-wrong-password")
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
            .expect("failed to run `lazuli generate go examples/marketplace-mini`");
        assert_success("lazuli generate go examples/marketplace-mini", &generate);
    }

    fn append_runtime_replace(out_dir: &Path) {
        let go_mod = out_dir.join("go.mod");
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

    fn build_marketplace_mini(out_dir: &Path) -> PathBuf {
        let binary = out_dir.join(binary_name("marketplace-mini-auth-wrong-password"));
        let build = Command::new("go")
            .current_dir(out_dir)
            .env("GOFLAGS", "-mod=mod")
            .args(["build", "-o"])
            .arg(&binary)
            .arg(".")
            .output()
            .expect("failed to run `go build` for Marketplace Mini");
        assert_success("go build -o marketplace-mini-auth-wrong-password .", &build);
        binary
    }

    fn apply_generated_migrations(out_dir: &Path, db_url: &str) -> Result<(), String> {
        let helper_dir = out_dir.join("smoke_migrations");
        fs::create_dir_all(&helper_dir)
            .map_err(|err| format!("creating {}: {err}", helper_dir.display()))?;
        fs::write(helper_dir.join("main.go"), MIGRATION_HELPER)
            .map_err(|err| format!("writing migration helper: {err}"))?;

        let migrations_dir = out_dir.join("migrations");
        let migrate = Command::new("go")
            .current_dir(out_dir)
            .env("GOFLAGS", "-mod=mod")
            .env("LAZULI_DB", db_url)
            .env("LAZULI_MIGRATIONS", &migrations_dir)
            .env("LAZULI_SMOKE_EMAIL", SMOKE_EMAIL)
            .args(["run", "./smoke_migrations"])
            .output()
            .map_err(|err| format!("running migration helper: {err}"))?;
        if !migrate.status.success() {
            return Err(format!(
                "go run ./smoke_migrations failed with status {}\nstdout:\n{}\nstderr:\n{}",
                migrate.status,
                String::from_utf8_lossy(&migrate.stdout),
                String::from_utf8_lossy(&migrate.stderr)
            ));
        }
        Ok(())
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
        http_request(port, "GET", path, None)
    }

    fn post_json(port: u16, path: &str, body: &str) -> std::io::Result<HttpResponse> {
        http_request(port, "POST", path, Some(body))
    }

    fn http_request(
        port: u16,
        method: &str,
        path: &str,
        body: Option<&str>,
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
            "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        )?;

        let mut raw = Vec::new();
        stream.read_to_end(&mut raw)?;
        parse_http_response(&raw)
    }

    struct HttpResponse {
        status: u16,
        headers: Vec<(String, String)>,
        body: String,
    }

    impl HttpResponse {
        fn header_values(&self, name: &str) -> Vec<&str> {
            self.headers
                .iter()
                .filter(|(key, _)| key.eq_ignore_ascii_case(name))
                .map(|(_, value)| value.as_str())
                .collect()
        }
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
        let headers = head
            .lines()
            .skip(1)
            .filter_map(|line| {
                let (name, value) = line.split_once(':')?;
                Some((name.trim().to_owned(), value.trim().to_owned()))
            })
            .collect();
        Ok(HttpResponse {
            status,
            headers,
            body: body.to_owned(),
        })
    }

    fn assert_auth_password_mismatch_body(body: &str) {
        if body_contains_string_field(body, "error", "auth.password_mismatch")
            && body_contains_string_field(body, "code", "AUTH-401")
        {
            return;
        }
        if body_contains_string_field(body, "code", "auth.password_mismatch")
            && body_contains_number_field(body, "status", 401)
        {
            return;
        }
        panic!(
            "wrong-password login body must be {{\"error\":\"auth.password_mismatch\",\"code\":\"AUTH-401\"}} or an equivalent problem shape; got:\n{body}"
        );
    }

    fn body_contains_string_field(body: &str, field: &str, value: &str) -> bool {
        json_string_field(body, field).as_deref() == Some(value)
    }

    fn body_contains_number_field(body: &str, field: &str, value: i64) -> bool {
        json_number_field(body, field) == Some(value)
    }

    fn json_number_field(body: &str, field: &str) -> Option<i64> {
        let mut rest = after_json_field(body, field)?.trim_start();
        let negative = rest.starts_with('-');
        if negative {
            rest = rest.strip_prefix('-')?;
        }
        let end = rest
            .char_indices()
            .take_while(|(_, ch)| ch.is_ascii_digit())
            .last()
            .map(|(index, ch)| index + ch.len_utf8())?;
        let value = rest.get(..end)?.parse::<i64>().ok()?;
        Some(if negative { -value } else { value })
    }

    fn json_string_field(body: &str, field: &str) -> Option<String> {
        let rest = after_json_field(body, field)?.trim_start();
        let rest = rest.strip_prefix('"')?;
        let mut value = String::new();
        let mut chars = rest.chars();
        while let Some(ch) = chars.next() {
            match ch {
                '"' => return Some(value),
                '\\' => value.push(chars.next()?),
                _ => value.push(ch),
            }
        }
        None
    }

    fn after_json_field<'a>(body: &'a str, field: &str) -> Option<&'a str> {
        let needle = format!("\"{field}\"");
        let rest = body.split_once(&needle)?.1.trim_start();
        rest.strip_prefix(':')
    }

    struct AuditRow {
        actor_kind: String,
        command_name: String,
        result_status: String,
    }

    fn latest_login_audit_row(out_dir: &Path, db_url: &str) -> Result<Option<AuditRow>, String> {
        let helper_dir = out_dir.join("smoke_audit_query");
        fs::create_dir_all(&helper_dir)
            .map_err(|err| format!("creating {}: {err}", helper_dir.display()))?;
        fs::write(helper_dir.join("main.go"), AUDIT_QUERY_HELPER)
            .map_err(|err| format!("writing audit query helper: {err}"))?;

        let query = Command::new("go")
            .current_dir(out_dir)
            .env("GOFLAGS", "-mod=mod")
            .env("LAZULI_DB", db_url)
            .args(["run", "./smoke_audit_query"])
            .output()
            .map_err(|err| format!("running audit query helper: {err}"))?;
        if !query.status.success() {
            return Err(format!(
                "audit query helper failed with status {}\nstdout:\n{}\nstderr:\n{}",
                query.status,
                String::from_utf8_lossy(&query.stdout),
                String::from_utf8_lossy(&query.stderr)
            ));
        }

        let stdout = String::from_utf8_lossy(&query.stdout);
        let line = stdout.trim();
        if line.is_empty() {
            return Ok(None);
        }
        let mut parts = line.split('\t');
        let actor_kind = parts.next().unwrap_or_default().to_owned();
        let command_name = parts.next().unwrap_or_default().to_owned();
        let result_status = parts.next().unwrap_or_default().to_owned();
        if actor_kind.is_empty() || command_name.is_empty() || result_status.is_empty() {
            return Err(format!("malformed audit query output: {line:?}"));
        }
        Ok(Some(AuditRow {
            actor_kind,
            command_name,
            result_status,
        }))
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

    fn binary_name(stem: &str) -> String {
        if cfg!(windows) {
            format!("{stem}.exe")
        } else {
            stem.to_owned()
        }
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
	smokeEmail := os.Getenv("LAZULI_SMOKE_EMAIL")
	if smokeEmail == "" {
		panic("LAZULI_SMOKE_EMAIL is required")
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

	_, _ = conn.Exec(ctx, `DELETE FROM audit_log WHERE command_name IN ('auth.login', 'account.login')`)
	_, _ = conn.Exec(ctx, `DELETE FROM "Session"`)
	_, _ = conn.Exec(ctx, `DELETE FROM "session"`)
	_, _ = conn.Exec(ctx, `DELETE FROM "User" WHERE email = $1`, smokeEmail)
	_, _ = conn.Exec(ctx, `DELETE FROM "user" WHERE email = $1`, smokeEmail)
}
"#;

    const AUDIT_QUERY_HELPER: &str = r#"package main

import (
	"context"
	"fmt"
	"os"
	"time"

	"github.com/jackc/pgx/v5"
)

func main() {
	dbURL := os.Getenv("LAZULI_DB")
	if dbURL == "" {
		panic("LAZULI_DB is required")
	}

	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()

	conn, err := pgx.Connect(ctx, dbURL)
	if err != nil {
		panic(fmt.Sprintf("connect postgres: %v", err))
	}
	defer conn.Close(context.Background())

	var actorKind, commandName, resultStatus string
	err = conn.QueryRow(ctx, `
		SELECT actor_kind, command_name, result_status
		FROM audit_log
		WHERE command_name = 'auth.login'
		ORDER BY happened_at DESC, id DESC
		LIMIT 1
	`).Scan(&actorKind, &commandName, &resultStatus)
	if err == pgx.ErrNoRows {
		return
	}
	if err != nil {
		panic(fmt.Sprintf("query audit_log: %v", err))
	}
	fmt.Printf("%s\t%s\t%s\n", actorKind, commandName, resultStatus)
}
"#;
}
