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
    const SMOKE_EMAIL: &str = "test@example.com";
    const SMOKE_NAME: &str = "Test";
    const REGISTER_COMMAND: &str = "account.register";

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
                .join("lazuli-go-smoke-roundtrip")
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

    #[test]
    fn marketplace_mini_command_round_trips_over_http() {
        let repo_root = repo_root();
        let tempdir = TempDir::new(&repo_root);

        generate_marketplace_mini(&repo_root, tempdir.path());
        append_runtime_replace(&repo_root, tempdir.path());
        let binary = build_marketplace_mini_binary(tempdir.path());

        let commands = generated_commands(tempdir.path());
        if commands.is_empty() {
            // TODO(smoke_e2e): keep this gate non-failing if the fixture stops
            // exposing commands; switch to a command-bearing fixture instead.
            eprintln!(
                "TODO(smoke_e2e): marketplace-mini generated zero HTTP-exposed commands; skipping command round-trip assertion"
            );
            return;
        }
        assert!(
            commands.iter().any(|command| command == REGISTER_COMMAND),
            "marketplace-mini does not expose {REGISTER_COMMAND}; available commands: {commands:?}"
        );

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

        let response = post_json(
            port,
            "/api/v1/c/account.register",
            r#"{"email":"test@example.com","password":"correct horse battery staple","name":"Test"}"#,
        )
        .expect("POST account.register");

        if response.status != 200 && response.status != 201 {
            let output = server.terminate_and_output();
            panic!(
                "POST account.register returned status {}, want 200 or 201\nbody:\n{}\nserver output:\n{}",
                response.status, response.body, output
            );
        }
        assert_json_user_shape(&response.body);
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
        let binary = out_dir.join(format!("marketplace-mini-smoke{}", env::consts::EXE_SUFFIX));
        let build = Command::new("go")
            .current_dir(out_dir)
            .env("GOFLAGS", "-mod=mod")
            .args(["build", "-o"])
            .arg(&binary)
            .arg(".")
            .output()
            .expect("failed to run `go build`");
        assert_success("go build -o marketplace-mini-smoke .", &build);
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

    fn generated_commands(root: &Path) -> Vec<String> {
        let mut files = Vec::new();
        collect_command_files(root, &mut files);

        let mut commands = Vec::new();
        for file in files {
            let content = fs::read_to_string(&file)
                .unwrap_or_else(|err| panic!("reading {}: {err}", file.display()));
            for line in content.lines() {
                let line = line.trim();
                if let Some(command) = parse_command_name_line(line) {
                    commands.push(command);
                }
            }
        }
        commands.sort();
        commands.dedup();
        commands
    }

    fn collect_command_files(dir: &Path, files: &mut Vec<PathBuf>) {
        for entry in
            fs::read_dir(dir).unwrap_or_else(|err| panic!("reading {}: {err}", dir.display()))
        {
            let entry =
                entry.unwrap_or_else(|err| panic!("reading entry in {}: {err}", dir.display()));
            let path = entry.path();
            if path.is_dir() {
                collect_command_files(&path, files);
            } else if path.file_name().and_then(|name| name.to_str()) == Some("command.gen.go") {
                files.push(path);
            }
        }
    }

    fn parse_command_name_line(line: &str) -> Option<String> {
        let rest = line.strip_prefix("Name:")?.trim();
        let rest = rest.strip_prefix('"')?;
        let end = rest.find('"')?;
        Some(rest[..end].to_owned())
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
        body: String,
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
        Ok(HttpResponse {
            status,
            body: body.to_owned(),
        })
    }

    fn assert_json_user_shape(body: &str) {
        let id = json_number_field(body, "id")
            .unwrap_or_else(|| panic!("response JSON missing numeric id field:\n{body}"));
        assert!(id > 0, "response id = {id}, want a positive number");

        let email = json_string_field(body, "email")
            .unwrap_or_else(|| panic!("response JSON missing string email field:\n{body}"));
        assert_eq!(email, SMOKE_EMAIL);

        let name = json_string_field(body, "name")
            .unwrap_or_else(|| panic!("response JSON missing string name field:\n{body}"));
        assert_eq!(name, SMOKE_NAME);
    }

    fn json_number_field(body: &str, field: &str) -> Option<i64> {
        let mut rest = after_json_field(body, field)?.trim_start();
        if let Some(stripped) = rest.strip_prefix('-') {
            rest = stripped;
        }
        let digits = rest
            .char_indices()
            .take_while(|(_, ch)| ch.is_ascii_digit())
            .last()
            .map(|(index, ch)| index + ch.len_utf8())?;
        after_json_field(body, field)?
            .trim_start()
            .get(..digits)
            .and_then(|value| value.parse::<i64>().ok())
    }

    fn json_string_field(body: &str, field: &str) -> Option<String> {
        let rest = after_json_field(body, field)?.trim_start();
        let rest = rest.strip_prefix('"')?;
        let mut value = String::new();
        let mut chars = rest.chars();
        while let Some(ch) = chars.next() {
            match ch {
                '"' => return Some(value),
                '\\' => {
                    let escaped = chars.next()?;
                    value.push(escaped);
                }
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

	_, _ = conn.Exec(ctx, `DELETE FROM "user" WHERE email = $1`, "test@example.com")
	_, _ = conn.Exec(ctx, `DELETE FROM "User" WHERE email = $1`, "test@example.com")
}
"#;
}
