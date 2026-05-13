#![allow(unexpected_cfgs)]

#[cfg(feature = "smoke_e2e")]
mod smoke_e2e {
    use std::env;
    use std::fs::{self, OpenOptions};
    use std::io::{self, Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::path::{Path, PathBuf};
    use std::process::{Child, Command, Output, Stdio};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn google_oauth_redirect_issues_provider_location_with_state() {
        let repo_root = repo_root();
        let fixture = TempDir::new(&repo_root, "fixture");
        let out = TempDir::new(&repo_root, "out");

        write_oauth_google_fixture(fixture.path());
        generate_fixture(&repo_root, fixture.path(), out.path());
        append_runtime_replace(&repo_root, out.path());
        write_oauth_redirect_smoke_route(out.path());
        replace_main_with_no_db_smoke_harness(out.path());
        let binary = build_generated_server(out.path());

        let port = free_tcp_port();
        let addr = format!("127.0.0.1:{port}");
        let mut server = ProcessGuard::spawn(&binary, out.path(), &addr);
        wait_for_server(&mut server, &addr);

        let response =
            http_request(&addr, "GET", "/auth/oauth/google", None).expect("GET /auth/oauth/google");

        assert_eq!(
            response.status,
            302,
            "GET /auth/oauth/google status = {}, want 302\nheaders:\n{}\nbody:\n{}\n{}",
            response.status,
            response.headers,
            response.body,
            server.logs()
        );
        let location = header_value(&response.headers, "Location")
            .unwrap_or_else(|| panic!("redirect response missing Location header:\n{response:?}"));

        assert!(
            location.contains("accounts.google.com"),
            "Location should target Google OAuth, got {location:?}"
        );
        assert!(
            location.contains("redirect_uri="),
            "Location should include redirect_uri, got {location:?}"
        );
        assert!(
            location.contains("%2Fauth%2Foauth%2Fgoogle%2Fcallback")
                || location.contains("/auth/oauth/google/callback"),
            "redirect_uri should point at the generated callback route, got {location:?}"
        );

        let state = query_param(location, "state")
            .unwrap_or_else(|| panic!("Location missing state query parameter: {location:?}"));
        assert!(
            state.len() >= 32,
            "state token should carry random entropy, got {state:?}"
        );
        assert!(
            state
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'),
            "state token should be URL-safe base64, got {state:?}"
        );
    }

    #[test]
    #[ignore = "needs httptest mock of Google userinfo"]
    fn google_oauth_callback_sets_session_cookie() {
        // The redirect smoke above pins the route shape and state token. A full
        // callback pass needs the generated server to exchange `code` with a
        // local httptest OAuth token endpoint and fetch Google userinfo through
        // a mock of https://openidconnect.googleapis.com/v1/userinfo.
    }

    fn write_oauth_google_fixture(dir: &Path) {
        fs::create_dir_all(dir).unwrap_or_else(|err| panic!("creating {}: {err}", dir.display()));
        fs::write(
            dir.join("app.lzi"),
            r#"app OAuthGoogleSmoke
  uses
    account

  targets
    backend go

  environments
    local

  urls
    web local "http://127.0.0.1:8080"
    api local "http://127.0.0.1:8080"

  runtime
    unit api
      healthcheck "/healthz"
"#,
        )
        .unwrap_or_else(|err| panic!("writing app.lzi: {err}"));

        fs::write(
            dir.join("account.lzi"),
            r#"feature account
  purpose "OAuth Google redirect smoke fixture."

  domain
    resource User
      email: @semantic.Email required unique
      name: Text optional

    resource Session
      user: User required
      expires_at: DateTime required

  policies
    login: @scope.public

  auth
    identity User.email

    oauth google
      adapter @adapter.google_oauth

    sessions
      resource Session
      ttl "30 days"
      refresh true

  extensions
    adapter google_oauth: IntegrationAdapter[GoogleOAuth] at "./integrations/google_oauth.go"
"#,
        )
        .unwrap_or_else(|err| panic!("writing account.lzi: {err}"));
    }

    fn generate_fixture(repo_root: &Path, fixture_dir: &Path, out_dir: &Path) {
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
            .arg(fixture_dir)
            .arg("--out")
            .arg(out_dir)
            .output()
            .expect("failed to run `lazuli generate go`");
        assert_success("lazuli generate go <oauth-google-fixture>", &generate);
    }

    fn append_runtime_replace(repo_root: &Path, out_dir: &Path) {
        let go_mod = out_dir.join("go.mod");
        let runtime = repo_root.join("runtime").join("go");
        assert!(
            runtime.join("go.mod").exists(),
            "runtime go.mod missing at {}",
            runtime.join("go.mod").display()
        );
        let relative = pathdiff(&runtime, out_dir).unwrap_or(runtime);
        let relative = relative.to_string_lossy().replace('\\', "/");

        let mut file = OpenOptions::new()
            .append(true)
            .open(&go_mod)
            .unwrap_or_else(|err| panic!("opening {} for append: {err}", go_mod.display()));
        writeln!(file, "\nreplace lazuli.dev/runtime => {relative}").unwrap_or_else(|err| {
            panic!("writing replace directive to {}: {err}", go_mod.display())
        });
    }

    fn replace_main_with_no_db_smoke_harness(out_dir: &Path) {
        let module = go_module_path(out_dir);
        let main_go = out_dir.join("main.go");
        fs::write(
            &main_go,
            format!(
                r#"package main

import (
	"log/slog"
	"net/http"
	"os"

	"lazuli.dev/runtime/lazuli"

	"{module}/account"
)

func main() {{
	mux := lazuli.Mux()
	account.MountOAuthGoogleSmokeRoute(mux)
	addr := os.Getenv("LAZULI_ADDR")
	if addr == "" {{
		addr = ":8080"
	}}
	slog.Info("lazuli oauth smoke listening", "addr", addr)
	if err := http.ListenAndServe(addr, mux); err != nil {{
		slog.Error("lazuli oauth smoke exited", "error", err)
		os.Exit(1)
	}}
}}
"#
            ),
        )
        .unwrap_or_else(|err| panic!("writing no-db smoke main {}: {err}", main_go.display()));
    }

    fn write_oauth_redirect_smoke_route(out_dir: &Path) {
        let route = out_dir.join("account").join("oauth_google_smoke_route.go");
        fs::write(
            &route,
            r#"package account

import (
	"net/http"
	"os"

	"lazuli.dev/runtime/lazuli"
	"lazuli.dev/runtime/lazuli/auth"
)

func MountOAuthGoogleSmokeRoute(mux *http.ServeMux) {
	mux.HandleFunc("GET /auth/oauth/google", func(w http.ResponseWriter, r *http.Request) {
		contract := accountAuthOAuthGoogle
		contract.ClientID = os.Getenv("GOOGLE_OAUTH_CLIENT_ID")
		contract.ClientSecret = os.Getenv("GOOGLE_OAUTH_CLIENT_SECRET")
		contract.RedirectURL = "http://" + r.Host + "/auth/oauth/google/callback"
		contract.Scopes = []string{"openid", "email", "profile"}

		ctx := &lazuli.Ctx{Context: r.Context()}
		redirectURL, err := auth.OAuthRedirect(ctx, contract)
		if err != nil {
			http.Error(w, err.Error(), http.StatusInternalServerError)
			return
		}
		state := auth.LoadOAuthState(ctx, contract.Provider)
		http.SetCookie(w, &http.Cookie{
			Name:     "lazuli_oauth_google_state",
			Value:    state,
			Path:     "/auth/oauth/google",
			HttpOnly: true,
			SameSite: http.SameSiteLaxMode,
		})
		http.Redirect(w, r, redirectURL, http.StatusFound)
	})
}
"#,
        )
        .unwrap_or_else(|err| panic!("writing OAuth smoke route {}: {err}", route.display()));
    }

    fn go_module_path(out_dir: &Path) -> String {
        let go_mod = out_dir.join("go.mod");
        let contents = fs::read_to_string(&go_mod)
            .unwrap_or_else(|err| panic!("reading {}: {err}", go_mod.display()));
        contents
            .lines()
            .find_map(|line| line.trim().strip_prefix("module "))
            .map(str::trim)
            .filter(|module| !module.is_empty())
            .unwrap_or_else(|| panic!("go.mod missing module line:\n{contents}"))
            .to_owned()
    }

    fn build_generated_server(out_dir: &Path) -> PathBuf {
        let binary = out_dir.join(format!("oauth-google-smoke{}", env::consts::EXE_SUFFIX));
        let build = Command::new("go")
            .current_dir(out_dir)
            .env("GOFLAGS", "-mod=mod")
            .args(["build", "-o"])
            .arg(&binary)
            .arg(".")
            .output()
            .expect("failed to run `go build`");
        assert_success("go build -o oauth-google-smoke .", &build);
        binary
    }

    fn wait_for_server(server: &mut ProcessGuard, addr: &str) {
        let deadline = Instant::now() + Duration::from_secs(30);
        let mut last_error = "no request attempted".to_owned();

        while Instant::now() < deadline {
            if let Some(status) = server.try_wait() {
                panic!(
                    "generated server exited before readiness with {status}\n{}",
                    server.logs()
                );
            }
            match http_request(addr, "GET", "/healthz", None) {
                Ok(response) if response.status == 200 => return,
                Ok(response) => last_error = format!("status {}", response.status),
                Err(err) => last_error = err.to_string(),
            }
            std::thread::sleep(Duration::from_millis(100));
        }

        panic!(
            "generated server did not become ready at http://{addr}/healthz; last_error={last_error}\n{}",
            server.logs()
        );
    }

    fn http_request(
        addr: &str,
        method: &str,
        path: &str,
        body: Option<&str>,
    ) -> io::Result<HttpResponse> {
        let mut stream = TcpStream::connect(addr)?;
        stream.set_read_timeout(Some(Duration::from_secs(5)))?;
        stream.set_write_timeout(Some(Duration::from_secs(5)))?;

        let body = body.unwrap_or("");
        write!(
            stream,
            "{method} {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        )?;

        let mut raw = Vec::new();
        stream.read_to_end(&mut raw)?;
        parse_http_response(&raw)
    }

    fn parse_http_response(raw: &[u8]) -> io::Result<HttpResponse> {
        let response = String::from_utf8_lossy(raw);
        let (head, body) = response.split_once("\r\n\r\n").ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "HTTP response missing headers")
        })?;
        let status = head
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|status| status.parse::<u16>().ok())
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "HTTP response missing status")
            })?;
        Ok(HttpResponse {
            status,
            headers: head.to_owned(),
            body: body.to_owned(),
        })
    }

    fn header_value<'a>(headers: &'a str, name: &str) -> Option<&'a str> {
        for line in headers.lines().skip(1) {
            let Some((key, value)) = line.split_once(':') else {
                continue;
            };
            if key.eq_ignore_ascii_case(name) {
                return Some(value.trim());
            }
        }
        None
    }

    fn query_param<'a>(url: &'a str, key: &str) -> Option<&'a str> {
        let query = url.split_once('?')?.1.split_once('#').map_or_else(
            || url.split_once('?').map(|(_, query)| query).unwrap_or(""),
            |(query, _)| query,
        );
        for pair in query.split('&') {
            let (candidate, value) = pair.split_once('=')?;
            if candidate == key {
                return Some(value);
            }
        }
        None
    }

    fn free_tcp_port() -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").expect("binding ephemeral port");
        listener
            .local_addr()
            .expect("reading ephemeral socket address")
            .port()
    }

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .expect("crate should live under <repo>/crates/lazuli_codegen_go")
            .to_path_buf()
    }

    fn pathdiff(path: &Path, base: &Path) -> Option<PathBuf> {
        let path = path.canonicalize().ok()?;
        let base = base.canonicalize().ok()?;
        let path_components = path.components().collect::<Vec<_>>();
        let base_components = base.components().collect::<Vec<_>>();
        let common = path_components
            .iter()
            .zip(base_components.iter())
            .take_while(|(a, b)| a == b)
            .count();
        if common == 0 {
            return None;
        }
        let mut out = PathBuf::new();
        for _ in common..base_components.len() {
            out.push("..");
        }
        for component in &path_components[common..] {
            out.push(component.as_os_str());
        }
        Some(out)
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
        headers: String,
        body: String,
    }

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(repo_root: &Path, label: &str) -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time before UNIX_EPOCH")
                .as_nanos();
            let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
            let path = repo_root
                .join("target")
                .join("lazuli-go-smoke-oauth-google")
                .join(format!("{}-{nonce}-{id}-{label}", std::process::id()));
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

    struct ProcessGuard {
        child: Child,
        stdout: PathBuf,
        stderr: PathBuf,
    }

    impl ProcessGuard {
        fn spawn(binary: &Path, out_dir: &Path, addr: &str) -> Self {
            let stdout = out_dir.join("oauth-google.stdout.log");
            let stderr = out_dir.join("oauth-google.stderr.log");
            let stdout_file = fs::File::create(&stdout)
                .unwrap_or_else(|err| panic!("creating {}: {err}", stdout.display()));
            let stderr_file = fs::File::create(&stderr)
                .unwrap_or_else(|err| panic!("creating {}: {err}", stderr.display()));
            let db_url = env::var("LAZULI_SMOKE_DB")
                .or_else(|_| env::var("LAZULI_DB"))
                .unwrap_or_else(|_| {
                    "postgres://lazuli:lazuli@localhost:5432/lazuli?sslmode=disable".to_owned()
                });

            let child = Command::new(binary)
                .current_dir(out_dir)
                .env("LAZULI_ADDR", addr)
                .env("LAZULI_DB", db_url)
                .env("GOOGLE_OAUTH_CLIENT_ID", "smoke-client-id")
                .env("GOOGLE_OAUTH_CLIENT_SECRET", "smoke-client-secret")
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
            self.child
                .try_wait()
                .expect("failed to poll generated server")
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
}
