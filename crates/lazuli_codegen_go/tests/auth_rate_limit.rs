#![allow(unexpected_cfgs)]

#[cfg(feature = "smoke_e2e")]
mod smoke_e2e {
    use std::collections::BTreeMap;
    use std::env;
    use std::fs::{self, OpenOptions};
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::path::{Path, PathBuf};
    use std::process::{Child, Command, Output, Stdio};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

    const EMAIL: &str = "rate-limit@example.com";
    const PASSWORD: &str = "correct horse battery staple";
    const WRONG_PASSWORD: &str = "not the password";

    #[test]
    fn failed_login_rate_limit_returns_429_then_resets() {
        let repo_root = repo_root();
        let tempdir = TempDir::new(&repo_root);
        write_fixture_source(tempdir.path());
        generate_fixture(&repo_root, tempdir.path());
        append_runtime_replace(tempdir.path());
        write_fixture_glue(tempdir.path());
        gofmt(tempdir.path());
        let binary = go_build_server(tempdir.path());

        let port = free_tcp_port();
        let addr = format!("127.0.0.1:{port}");
        let mut server = ProcessGuard::spawn(&binary, tempdir.path(), &addr);
        wait_for_server(&mut server, &addr);

        let signup = post_json(
            &addr,
            "/auth/signup",
            &format!(r#"{{"email":"{EMAIL}","password":"{PASSWORD}","name":"Rate Limit"}}"#),
        )
        .expect("POST /auth/signup");
        assert_status(&signup, 201, "signup");

        for attempt in 1..=3 {
            let response = post_json(
                &addr,
                "/auth/login",
                &format!(r#"{{"email":"{EMAIL}","password":"{WRONG_PASSWORD}"}}"#),
            )
            .unwrap_or_else(|err| panic!("POST /auth/login attempt {attempt}: {err}"));
            assert_status(&response, 401, &format!("wrong-password attempt {attempt}"));
        }

        let limited = post_json(
            &addr,
            "/auth/login",
            &format!(r#"{{"email":"{EMAIL}","password":"{WRONG_PASSWORD}"}}"#),
        )
        .expect("POST /auth/login limited attempt");
        assert_status(&limited, 429, "fourth wrong-password attempt");
        assert!(
            limited.headers.contains_key("retry-after"),
            "429 response missing Retry-After header; headers={:?}\nbody:\n{}",
            limited.headers,
            limited.body
        );

        std::thread::sleep(Duration::from_secs(11));

        let reset = post_json(
            &addr,
            "/auth/login",
            &format!(r#"{{"email":"{EMAIL}","password":"{WRONG_PASSWORD}"}}"#),
        )
        .expect("POST /auth/login after rate-limit window");
        assert_status(&reset, 401, "wrong-password attempt after reset");
    }

    fn write_fixture_source(root: &Path) {
        fs::write(
            root.join("app.lzi"),
            r#"app AuthRateLimitSmoke
  uses
    account

  targets
    backend go
"#,
        )
        .unwrap_or_else(|err| panic!("writing app.lzi: {err}"));

        fs::write(
            root.join("account.lzi"),
            r#"feature account
  purpose "Password auth rate limit smoke fixture."

  domain
    resource User
      email: @semantic.Email required unique
      name: Text required
      password_hash: @cap.Hashed(algorithm:argon2id) required

    resource Session
      user: User required on_delete cascade
      refresh_token_hash: @cap.Hashed(algorithm:argon2id) required
      expires_at: DateTime required

  policies
    auth: @scope.public

  auth
    identity User.email

    password
      algorithm argon2id
      hash @fn.hash_password
      verify @fn.verify_password
      rate_limit "3 per 10 seconds per ip"

    sessions
      resource Session
      ttl "1 hour"
      refresh false
"#,
        )
        .unwrap_or_else(|err| panic!("writing account.lzi: {err}"));
    }

    fn generate_fixture(repo_root: &Path, source_dir: &Path) {
        let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
        let out_dir = source_dir.join("generated");
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
            .arg(&out_dir)
            .output()
            .expect("failed to run `lazuli generate go`");
        assert_success("lazuli generate go auth-rate-limit fixture", &generate);
    }

    fn append_runtime_replace(source_dir: &Path) {
        let go_mod = source_dir.join("generated").join("go.mod");
        let runtime = Path::new("..")
            .join("..")
            .join("..")
            .join("..")
            .join("runtime")
            .join("go");
        let runtime = runtime.to_string_lossy().replace('\\', "/");
        let mut file = OpenOptions::new()
            .append(true)
            .open(&go_mod)
            .unwrap_or_else(|err| panic!("opening {} for append: {err}", go_mod.display()));
        writeln!(file, "\nreplace lazuli.dev/runtime => {runtime}")
            .unwrap_or_else(|err| panic!("writing replace directive: {err}"));
    }

    fn write_fixture_glue(source_dir: &Path) {
        let out_dir = source_dir.join("generated");
        fs::write(out_dir.join("main.go"), MAIN_GO)
            .unwrap_or_else(|err| panic!("overwriting generated main.go: {err}"));
        fs::write(
            out_dir.join("account").join("auth_contract_export.go"),
            ACCOUNT_AUTH_EXPORT_GO,
        )
        .unwrap_or_else(|err| panic!("writing auth contract export: {err}"));
    }

    fn gofmt(source_dir: &Path) {
        let out_dir = source_dir.join("generated");
        let status = Command::new("gofmt")
            .current_dir(&out_dir)
            .args(["-w", "main.go", "account/auth_contract_export.go"])
            .status()
            .expect("failed to run gofmt");
        assert!(status.success(), "gofmt failed with status {status}");
    }

    fn go_build_server(source_dir: &Path) -> PathBuf {
        let out_dir = source_dir.join("generated");
        let binary = out_dir.join(binary_name("auth-rate-limit-smoke"));
        let build = Command::new("go")
            .current_dir(&out_dir)
            .env("GOFLAGS", "-mod=mod")
            .args(["build", "-o"])
            .arg(&binary)
            .arg(".")
            .output()
            .expect("failed to run `go build`");
        assert_success("go build -o auth-rate-limit-smoke .", &build);
        binary
    }

    fn wait_for_server(server: &mut ProcessGuard, addr: &str) {
        let deadline = Instant::now() + Duration::from_secs(20);
        while Instant::now() < deadline {
            if let Some(status) = server.try_wait() {
                panic!(
                    "auth-rate-limit fixture exited before readiness with {status}\n{}",
                    server.logs()
                );
            }
            if let Ok(response) = get(addr, "/healthz") {
                if response.status == 200 {
                    return;
                }
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        panic!(
            "auth-rate-limit fixture did not become ready at http://{addr}/healthz\n{}",
            server.logs()
        );
    }

    fn get(addr: &str, path: &str) -> std::io::Result<HttpResponse> {
        http_request(addr, "GET", path, None)
    }

    fn post_json(addr: &str, path: &str, body: &str) -> std::io::Result<HttpResponse> {
        http_request(addr, "POST", path, Some(body))
    }

    fn http_request(
        addr: &str,
        method: &str,
        path: &str,
        body: Option<&str>,
    ) -> std::io::Result<HttpResponse> {
        let mut stream = TcpStream::connect(addr)?;
        stream.set_read_timeout(Some(Duration::from_secs(5)))?;
        stream.set_write_timeout(Some(Duration::from_secs(5)))?;

        let body = body.unwrap_or("");
        write!(
            stream,
            "{method} {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        )?;

        let mut raw = String::new();
        stream.read_to_string(&mut raw)?;
        parse_http_response(&raw)
    }

    fn parse_http_response(raw: &str) -> std::io::Result<HttpResponse> {
        let (head, body) = raw.split_once("\r\n\r\n").ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("malformed HTTP response: {raw:?}"),
            )
        })?;
        let status = head
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|status| status.parse::<u16>().ok())
            .ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::InvalidData, "missing HTTP status")
            })?;
        let mut headers = BTreeMap::new();
        for line in head.lines().skip(1) {
            if let Some((name, value)) = line.split_once(':') {
                headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_owned());
            }
        }
        Ok(HttpResponse {
            status,
            headers,
            body: body.to_owned(),
        })
    }

    fn assert_status(response: &HttpResponse, want: u16, label: &str) {
        assert_eq!(
            response.status, want,
            "{label} returned status {}, want {want}\nheaders: {:?}\nbody:\n{}",
            response.status, response.headers, response.body
        );
    }

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .expect("crate should live under <repo>/crates/lazuli_codegen_go")
            .to_path_buf()
    }

    fn free_tcp_port() -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").expect("binding ephemeral port");
        listener.local_addr().expect("reading local addr").port()
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
                .join("lazuli-go-smoke-auth-rate-limit")
                .join(format!("{}-{nonce}-{id}", std::process::id()));
            fs::create_dir_all(&path)
                .unwrap_or_else(|err| panic!("creating {}: {err}", path.display()));
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

    struct ProcessGuard {
        child: Child,
        stdout: PathBuf,
        stderr: PathBuf,
    }

    impl ProcessGuard {
        fn spawn(binary: &Path, work_dir: &Path, addr: &str) -> Self {
            let stdout = work_dir.join("server.stdout.log");
            let stderr = work_dir.join("server.stderr.log");
            let stdout_file = fs::File::create(&stdout)
                .unwrap_or_else(|err| panic!("creating {}: {err}", stdout.display()));
            let stderr_file = fs::File::create(&stderr)
                .unwrap_or_else(|err| panic!("creating {}: {err}", stderr.display()));
            let child = Command::new(binary)
                .current_dir(work_dir.join("generated"))
                .env("LAZULI_ADDR", addr)
                .stdout(Stdio::from(stdout_file))
                .stderr(Stdio::from(stderr_file))
                .spawn()
                .unwrap_or_else(|err| panic!("spawning {}: {err}", binary.display()));
            Self {
                child,
                stdout,
                stderr,
            }
        }

        fn try_wait(&mut self) -> Option<std::process::ExitStatus> {
            self.child.try_wait().expect("checking server process")
        }

        fn logs(&self) -> String {
            format!(
                "stdout:\n{}\nstderr:\n{}",
                fs::read_to_string(&self.stdout).unwrap_or_default(),
                fs::read_to_string(&self.stderr).unwrap_or_default()
            )
        }
    }

    impl Drop for ProcessGuard {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    struct HttpResponse {
        status: u16,
        headers: BTreeMap<String, String>,
        body: String,
    }

    const ACCOUNT_AUTH_EXPORT_GO: &str = r#"package account

import "lazuli.dev/runtime/lazuli/auth"

func AuthPasswordContract() auth.PasswordContract {
	return accountAuthPassword
}
"#;

    const MAIN_GO: &str = r#"package main

import (
	"encoding/json"
	"errors"
	"log/slog"
	"net"
	"net/http"
	"os"
	"strconv"
	"strings"
	"sync"
	"time"

	"lazuli.dev/runtime/lazuli"
	"lazuli.dev/runtime/lazuli/auth"

	"lazuli/auth-rate-limit-smoke/account"
)

type credentials struct {
	Email    string `json:"email"`
	Password string `json:"password"`
	Name     string `json:"name"`
}

type userRecord struct {
	Email        string `json:"email"`
	Name         string `json:"name"`
	PasswordHash string `json:"-"`
}

var (
	usersMu sync.Mutex
	users   = map[string]userRecord{}
	limitMu sync.Mutex
	buckets = map[string]*failureBucket{}
)

type failureBucket struct {
	windowStart time.Time
	count       int
}

func main() {
	addr := os.Getenv("LAZULI_ADDR")
	if addr == "" {
		addr = "127.0.0.1:8080"
	}

	mux := http.NewServeMux()
	mux.HandleFunc("GET /healthz", func(w http.ResponseWriter, _ *http.Request) {
		writeJSON(w, http.StatusOK, map[string]string{"status": "ok"})
	})
	mux.HandleFunc("POST /auth/signup", signup)
	mux.HandleFunc("POST /auth/login", login)

	slog.Info("auth-rate-limit smoke listening", "addr", addr)
	if err := http.ListenAndServe(addr, mux); err != nil {
		slog.Error("server exited", "error", err)
		os.Exit(1)
	}
}

func signup(w http.ResponseWriter, r *http.Request) {
	var input credentials
	if err := json.NewDecoder(r.Body).Decode(&input); err != nil {
		writeProblem(w, http.StatusBadRequest, "bad_request")
		return
	}
	if input.Email == "" || input.Password == "" {
		writeProblem(w, http.StatusBadRequest, "bad_request")
		return
	}

	contract := account.AuthPasswordContract()
	hash, err := auth.HashPassword(&lazuli.Ctx{Context: r.Context(), Now: time.Now()}, contract, input.Password)
	if err != nil {
		writeProblem(w, http.StatusInternalServerError, "internal")
		return
	}

	usersMu.Lock()
	defer usersMu.Unlock()
	users[input.Email] = userRecord{Email: input.Email, Name: input.Name, PasswordHash: hash}
	writeJSON(w, http.StatusCreated, map[string]string{"email": input.Email})
}

func login(w http.ResponseWriter, r *http.Request) {
	var input credentials
	if err := json.NewDecoder(r.Body).Decode(&input); err != nil {
		writeProblem(w, http.StatusBadRequest, "bad_request")
		return
	}

	key := requestIP(r)
	spec, err := lazuli.ParseRateLimit(lazuli.RateLimit(account.AuthPasswordContract().RateLimit))
	if err != nil {
		writeProblem(w, http.StatusInternalServerError, "internal")
		return
	}
	if retryAfter, limited := reserveFailureAttempt(key, spec); limited {
		w.Header().Set("Retry-After", retryAfter)
		writeProblem(w, http.StatusTooManyRequests, "rate_limited")
		return
	}

	usersMu.Lock()
	user, ok := users[input.Email]
	usersMu.Unlock()
	if !ok {
		writeProblem(w, http.StatusUnauthorized, "auth.password_mismatch")
		return
	}
	err = auth.VerifyPassword(&lazuli.Ctx{Context: r.Context(), Now: time.Now()}, account.AuthPasswordContract(), input.Password, user.PasswordHash)
	if errors.Is(err, auth.ErrPasswordMismatch) {
		writeProblem(w, http.StatusUnauthorized, "auth.password_mismatch")
		return
	}
	if err != nil {
		writeProblem(w, http.StatusInternalServerError, "internal")
		return
	}
	writeJSON(w, http.StatusOK, map[string]string{"email": user.Email})
}

func reserveFailureAttempt(key string, spec lazuli.RateLimitSpec) (string, bool) {
	now := time.Now()
	window := time.Duration(float64(spec.Burst)/spec.PerSecond) * time.Second
	if window <= 0 {
		window = time.Second
	}

	limitMu.Lock()
	defer limitMu.Unlock()
	bucket := buckets[key]
	if bucket == nil || now.Sub(bucket.windowStart) >= window {
		bucket = &failureBucket{windowStart: now}
		buckets[key] = bucket
	}
	if bucket.count >= spec.Burst {
		remaining := window - now.Sub(bucket.windowStart)
		if remaining < time.Second {
			remaining = time.Second
		}
		return strconv.Itoa(int((remaining + time.Second - 1) / time.Second)), true
	}
	bucket.count++
	return "", false
}

func requestIP(r *http.Request) string {
	host, _, err := net.SplitHostPort(r.RemoteAddr)
	if err == nil && host != "" {
		return host
	}
	return strings.TrimSpace(r.RemoteAddr)
}

func writeJSON(w http.ResponseWriter, status int, value any) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(status)
	_ = json.NewEncoder(w).Encode(value)
}

func writeProblem(w http.ResponseWriter, status int, code string) {
	writeJSON(w, status, map[string]any{
		"status": status,
		"code":   code,
	})
}
"#;
}
