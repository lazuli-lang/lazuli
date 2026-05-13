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

    const ACCOUNT_AUTH_TESTHOOK: &str = r#"package account

import "lazuli.dev/runtime/lazuli/auth"

func SmokeSessionsContract() auth.SessionsContract {
	return accountAuthSessions
}
"#;

    const SMOKE_SERVER: &str = r#"package main

import (
	"context"
	"encoding/json"
	"errors"
	"log/slog"
	"net/http"
	"os"
	"time"

	"github.com/jackc/pgx/v5"

	"lazuli.dev/runtime/lazuli"
	"lazuli.dev/runtime/lazuli/auth"

	"github.com/lazuli-lang/example-marketplace-mini/generated/account"
)

type signupInput struct {
	Email    string `json:"email"`
	Password string `json:"password"`
	Name     string `json:"name"`
}

type loginInput struct {
	Email    string `json:"email"`
	Password string `json:"password"`
}

func main() {
	ctx := context.Background()
	slog.SetDefault(slog.New(slog.NewTextHandler(os.Stdout, &slog.HandlerOptions{Level: slog.LevelInfo})))

	dbURL := os.Getenv("LAZULI_DB")
	if dbURL == "" {
		dbURL = "postgres://lazuli:lazuli@localhost:5432/lazuli?sslmode=disable"
	}
	if err := lazuli.Boot(ctx, dbURL); err != nil {
		slog.Error("lazuli boot failed", "error", err)
		os.Exit(1)
	}
	if err := ensureAuthSessionTable(ctx); err != nil {
		slog.Error("auth session table setup failed", "error", err)
		os.Exit(1)
	}

	mux := http.NewServeMux()
	mux.HandleFunc("GET /healthz", func(w http.ResponseWriter, _ *http.Request) {
		writeJSON(w, http.StatusOK, map[string]string{"status": "ok"})
	})
	mux.HandleFunc("POST /signup", signup)
	mux.HandleFunc("POST /login", login)
	mux.HandleFunc("GET /protected", protected)

	addr := os.Getenv("LAZULI_ADDR")
	if addr == "" {
		addr = ":8080"
	}
	if err := http.ListenAndServe(addr, mux); err != nil {
		slog.Error("lazuli http server exited", "error", err)
		os.Exit(1)
	}
}

func signup(w http.ResponseWriter, r *http.Request) {
	var input signupInput
	if err := json.NewDecoder(r.Body).Decode(&input); err != nil {
		writeJSON(w, http.StatusBadRequest, map[string]string{"error": "bad_request"})
		return
	}
	_, err := lazuli.DB().Exec(r.Context(),
		`INSERT INTO "user" (email, name, role, password_hash) VALUES ($1, $2, 'buyer', $3)`,
		input.Email, input.Name, input.Password)
	if err != nil {
		writeJSON(w, http.StatusInternalServerError, map[string]string{"error": "signup_failed"})
		return
	}
	writeJSON(w, http.StatusCreated, map[string]string{"status": "created"})
}

func login(w http.ResponseWriter, r *http.Request) {
	var input loginInput
	if err := json.NewDecoder(r.Body).Decode(&input); err != nil {
		writeJSON(w, http.StatusBadRequest, map[string]string{"error": "bad_request"})
		return
	}
	var userID lazuli.ID
	err := lazuli.DB().QueryRow(r.Context(), `SELECT id FROM "user" WHERE email = $1`, input.Email).Scan(&userID)
	if errors.Is(err, pgx.ErrNoRows) {
		writeJSON(w, http.StatusUnauthorized, map[string]string{"error": "auth.password_mismatch"})
		return
	}
	if err != nil {
		writeJSON(w, http.StatusInternalServerError, map[string]string{"error": "login_failed"})
		return
	}

	ctx := &lazuli.Ctx{Context: r.Context(), Now: time.Now()}
	token, expiresAt, err := auth.IssueSession(ctx, account.SmokeSessionsContract(), userID, nil)
	if err != nil {
		writeJSON(w, http.StatusInternalServerError, map[string]string{"error": "session_issue_failed"})
		return
	}
	auth.WriteSessionCookie(w, token, expiresAt, auth.SessionCookieOptions{})
	writeJSON(w, http.StatusOK, map[string]string{"status": "ok"})
}

func protected(w http.ResponseWriter, r *http.Request) {
	token, err := auth.ReadSessionCookie(r, auth.SessionCookieOptions{})
	if err != nil {
		writeJSON(w, http.StatusUnauthorized, map[string]string{"error": "auth.session_missing"})
		return
	}
	ctx := &lazuli.Ctx{Context: r.Context(), Now: time.Now()}
	if _, _, err := auth.ResolveSession(ctx, account.SmokeSessionsContract(), token); err != nil {
		if errors.Is(err, auth.ErrSessionExpired) {
			auth.ClearSessionCookie(w, auth.SessionCookieOptions{})
			writeJSON(w, http.StatusUnauthorized, map[string]string{"error": "auth.session_expired"})
			return
		}
		writeJSON(w, http.StatusUnauthorized, map[string]string{"error": "auth.session_unknown"})
		return
	}
	writeJSON(w, http.StatusOK, map[string]string{"status": "ok"})
}

func ensureAuthSessionTable(ctx context.Context) error {
	_, err := lazuli.DB().Exec(ctx, `CREATE TABLE IF NOT EXISTS "Session" (
		id BIGSERIAL PRIMARY KEY,
		user_id BIGINT NOT NULL,
		token_hash TEXT NOT NULL UNIQUE,
		expires_at TIMESTAMPTZ NOT NULL
	)`)
	return err
}

func writeJSON(w http.ResponseWriter, status int, value any) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(status)
	_ = json.NewEncoder(w).Encode(value)
}
"#;

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

	_, _ = conn.Exec(ctx, `CREATE TABLE IF NOT EXISTS "Session" (
		id BIGSERIAL PRIMARY KEY,
		user_id BIGINT NOT NULL,
		token_hash TEXT NOT NULL UNIQUE,
		expires_at TIMESTAMPTZ NOT NULL
	)`)
	_, _ = conn.Exec(ctx, `DELETE FROM "Session"`)
	_, _ = conn.Exec(ctx, `DELETE FROM "user" WHERE email = $1`, "expired-session@example.com")
}
"#;
}
