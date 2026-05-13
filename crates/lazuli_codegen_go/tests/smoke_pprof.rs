#![allow(unexpected_cfgs)]
#![cfg(any(feature = "smoke_e2e", smoke_e2e))]

use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const PORT: u16 = 18767;

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
            .join("lazuli-go-smoke-pprof")
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

struct RunningApp {
    child: Child,
    stdout: PathBuf,
    stderr: PathBuf,
}

impl RunningApp {
    fn spawn(binary: &Path, workdir: &Path, pprof: bool) -> Self {
        let stdout = workdir.join(if pprof {
            "marketplace-mini-pprof.stdout.log"
        } else {
            "marketplace-mini-no-pprof.stdout.log"
        });
        let stderr = workdir.join(if pprof {
            "marketplace-mini-pprof.stderr.log"
        } else {
            "marketplace-mini-no-pprof.stderr.log"
        });

        let mut command = Command::new(binary);
        command
            .current_dir(workdir)
            .env("PORT", PORT.to_string())
            .env_remove("LAZULI_ADDR")
            .stdout(Stdio::from(File::create(&stdout).unwrap_or_else(|err| {
                panic!("creating {}: {err}", stdout.display())
            })))
            .stderr(Stdio::from(File::create(&stderr).unwrap_or_else(|err| {
                panic!("creating {}: {err}", stderr.display())
            })));
        if pprof {
            command.env("LAZULI_PPROF", "1");
        } else {
            command.env_remove("LAZULI_PPROF");
        }

        let child = command
            .spawn()
            .unwrap_or_else(|err| panic!("spawning {}: {err}", binary.display()));

        Self {
            child,
            stdout,
            stderr,
        }
    }

    fn wait_until_ready(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            if let Some(status) = self
                .child
                .try_wait()
                .expect("checking marketplace-mini process")
            {
                panic!(
                    "marketplace-mini exited before /healthz became ready with status {status}\nstdout:\n{}\nstderr:\n{}",
                    read_lossy(&self.stdout),
                    read_lossy(&self.stderr)
                );
            }

            let probe = match http_get("/healthz") {
                Ok(response) if response.status == 200 => return,
                Ok(response) => format!("HTTP {}", response.status),
                Err(err) => err.to_string(),
            };

            if Instant::now() >= deadline {
                panic!(
                    "timed out waiting for marketplace-mini on port {PORT}; last probe: {probe}\nstdout:\n{}\nstderr:\n{}",
                    read_lossy(&self.stdout),
                    read_lossy(&self.stderr)
                );
            }

            thread::sleep(Duration::from_millis(100));
        }
    }
}

impl Drop for RunningApp {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

struct HttpResponse {
    status: u16,
    body: String,
}

#[test]
fn lazuli_pprof_mount_is_opt_in() {
    let repo_root = repo_root();
    let tempdir = TempDir::new(&repo_root);

    generate_marketplace_mini(&repo_root, tempdir.path());
    append_runtime_replace(&repo_root, tempdir.path());
    let binary = build_marketplace_mini(tempdir.path());

    {
        let mut app = RunningApp::spawn(&binary, tempdir.path(), true);
        app.wait_until_ready();

        let index = http_get("/debug/pprof/").expect("GET /debug/pprof/");
        assert_eq!(
            index.status, 200,
            "expected LAZULI_PPROF=1 to mount /debug/pprof/, got HTTP {}\nbody:\n{}",
            index.status, index.body
        );

        let goroutine =
            http_get("/debug/pprof/goroutine?debug=2").expect("GET /debug/pprof/goroutine");
        assert_eq!(
            goroutine.status, 200,
            "expected goroutine pprof endpoint to return 200, got HTTP {}\nbody:\n{}",
            goroutine.status, goroutine.body
        );
        assert!(
            goroutine.body.contains("goroutine"),
            "expected goroutine debug body to contain `goroutine`, got:\n{}",
            goroutine.body
        );
    }

    {
        let mut app = RunningApp::spawn(&binary, tempdir.path(), false);
        app.wait_until_ready();

        let index = http_get("/debug/pprof/").expect("GET /debug/pprof/ without LAZULI_PPROF");
        assert_eq!(
            index.status, 404,
            "expected /debug/pprof/ to stay unmounted without LAZULI_PPROF, got HTTP {}\nbody:\n{}",
            index.status, index.body
        );
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
        .expect("failed to run `cargo run -p lazuli_cli --bin lazuli -- generate go`");
    assert_success("lazuli generate go examples/marketplace-mini", &generate);
}

fn build_marketplace_mini(out_dir: &Path) -> PathBuf {
    let binary = out_dir.join(if cfg!(windows) {
        "marketplace-mini-smoke.exe"
    } else {
        "marketplace-mini-smoke"
    });
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

fn http_get(path: &str) -> std::io::Result<HttpResponse> {
    let mut stream = TcpStream::connect(("127.0.0.1", PORT))?;
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{PORT}\r\nConnection: close\r\n\r\n"
    )?;

    let mut raw = String::new();
    stream.read_to_string(&mut raw)?;
    let status_line = raw.lines().next().unwrap_or("");
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|status| status.parse::<u16>().ok())
        .unwrap_or(0);
    let body = raw
        .split_once("\r\n\r\n")
        .map(|(_, body)| body.to_owned())
        .unwrap_or_default();
    Ok(HttpResponse { status, body })
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
    writeln!(file, "\nreplace lazuli.dev/runtime => {runtime}")
        .unwrap_or_else(|err| panic!("writing replace directive to {}: {err}", go_mod.display()));
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

fn read_lossy(path: &Path) -> String {
    fs::read_to_string(path)
        .unwrap_or_else(|err| format!("<failed to read {}: {err}>", path.display()))
}
