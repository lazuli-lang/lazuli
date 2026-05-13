#![allow(unexpected_cfgs)]

#[cfg(feature = "smoke_e2e")]
mod smoke_e2e {
    use std::collections::BTreeMap;
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
    fn hostpoint_mini_healthz_matches_canonical_schema() {
        let repo_root = repo_root();
        let tempdir = TempDir::new(&repo_root);

        generate_hostpoint_mini(&repo_root, tempdir.path());
        append_runtime_replace(tempdir.path());
        let binary = build_hostpoint_mini(tempdir.path());

        let port = free_tcp_port();
        let addr = format!("127.0.0.1:{port}");
        let mut server = ProcessGuard::spawn(&binary, tempdir.path(), &addr);

        let response = wait_for_healthz(&mut server, &addr);
        let json = JsonParser::parse(&response.body).unwrap_or_else(|err| {
            panic!(
                "GET /healthz returned non-JSON body; status={} parse_error={err}\nbody:\n{}",
                response.status, response.body
            )
        });

        assert_health_schema(&json, &response);
    }

    fn generate_hostpoint_mini(repo_root: &Path, out_dir: &Path) {
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
                "examples/hostpoint-mini",
                "--out",
            ])
            .arg(out_dir)
            .output()
            .expect("failed to run `lazuli generate go examples/hostpoint-mini`");
        assert_success("lazuli generate go examples/hostpoint-mini", &generate);
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

    fn build_hostpoint_mini(out_dir: &Path) -> PathBuf {
        let binary = out_dir.join(binary_name("hostpoint-mini"));
        let build = Command::new("go")
            .current_dir(out_dir)
            .env("GOFLAGS", "-mod=mod")
            .args(["build", "-o"])
            .arg(&binary)
            .arg(".")
            .output()
            .expect("failed to run `go build` for Hostpoint Mini");
        assert_success("go build -o hostpoint-mini .", &build);
        binary
    }

    fn wait_for_healthz(server: &mut ProcessGuard, addr: &str) -> HttpResponse {
        let deadline = Instant::now() + Duration::from_secs(20);
        let mut last_error = "no request attempted".to_owned();

        loop {
            if let Some(status) = server.try_wait() {
                panic!(
                    "hostpoint-mini exited before /healthz responded; status={status}\n{}",
                    server.logs()
                );
            }

            if Instant::now() >= deadline {
                panic!(
                    "timed out waiting for hostpoint-mini /healthz at http://{addr}/healthz; last_error={}\n{}",
                    last_error,
                    server.logs()
                );
            }

            match http_get(addr, "/healthz") {
                Ok(response) => return response,
                Err(err) => last_error = err.to_string(),
            }

            std::thread::sleep(Duration::from_millis(100));
        }
    }

    fn http_get(addr: &str, path: &str) -> io::Result<HttpResponse> {
        let mut stream = TcpStream::connect(addr)?;
        stream.set_read_timeout(Some(Duration::from_secs(2)))?;
        stream.set_write_timeout(Some(Duration::from_secs(2)))?;

        write!(stream, "GET {path} HTTP/1.0\r\nHost: {addr}\r\n\r\n")?;

        let mut raw = String::new();
        stream.read_to_string(&mut raw)?;
        parse_http_response(&raw)
    }

    fn parse_http_response(raw: &str) -> io::Result<HttpResponse> {
        let (head, body) = raw.split_once("\r\n\r\n").ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("malformed HTTP response: {raw:?}"),
            )
        })?;
        let status = head
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|code| code.parse::<u16>().ok())
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("missing HTTP status line: {head:?}"),
                )
            })?;

        Ok(HttpResponse {
            status,
            body: body.to_owned(),
        })
    }

    fn assert_health_schema(json: &JsonValue, response: &HttpResponse) {
        let mut errors = Vec::new();
        let Some(root) = json.as_object() else {
            panic!(
                "/healthz root JSON must be an object; found {}\nbody:\n{}",
                json.shape(),
                response.body
            );
        };

        let allowed_root = ["checks", "status", "version"];
        for key in root.keys() {
            if !allowed_root.contains(&key.as_str()) {
                errors.push(format!("unexpected top-level field `{key}`"));
            }
        }

        match root.get("status") {
            Some(JsonValue::String(status))
                if matches!(status.as_str(), "ok" | "degraded" | "unhealthy") => {}
            Some(found) => errors.push(format!(
                "`status` must be one of ok/degraded/unhealthy strings; found {}",
                found.shape()
            )),
            None => errors.push("missing required top-level field `status`".to_owned()),
        }

        match root.get("version") {
            Some(JsonValue::String(_)) => {}
            Some(found) => errors.push(format!(
                "`version` must be a string; found {}",
                found.shape()
            )),
            None => errors.push("missing required top-level field `version`".to_owned()),
        }

        match root.get("checks") {
            Some(JsonValue::Object(checks)) => {
                for (name, value) in checks {
                    assert_check_shape(name, value, &mut errors);
                }
            }
            Some(found) => errors.push(format!(
                "`checks` must be an object; found {}",
                found.shape()
            )),
            None => errors.push("missing required top-level field `checks`".to_owned()),
        }

        assert!(
            errors.is_empty(),
            "/healthz schema mismatch; HTTP status={}\nerrors:\n- {}\nfound shape: {}\nbody:\n{}",
            response.status,
            errors.join("\n- "),
            json.shape(),
            response.body
        );
    }

    fn assert_check_shape(name: &str, value: &JsonValue, errors: &mut Vec<String>) {
        let Some(check) = value.as_object() else {
            errors.push(format!(
                "`checks.{name}` must be an object; found {}",
                value.shape()
            ));
            return;
        };

        let allowed_check = ["duration_ms", "error", "status"];
        for key in check.keys() {
            if !allowed_check.contains(&key.as_str()) {
                errors.push(format!("unexpected field `checks.{name}.{key}`"));
            }
        }

        match check.get("status") {
            Some(JsonValue::String(status)) if matches!(status.as_str(), "ok" | "fail") => {}
            Some(found) => errors.push(format!(
                "`checks.{name}.status` must be one of ok/fail strings; found {}",
                found.shape()
            )),
            None => errors.push(format!("missing required field `checks.{name}.status`")),
        }

        match check.get("error") {
            Some(JsonValue::String(_)) | None => {}
            Some(found) => errors.push(format!(
                "`checks.{name}.error` must be a string when present; found {}",
                found.shape()
            )),
        }

        match check.get("duration_ms") {
            Some(JsonValue::Number(_)) => {}
            Some(found) => errors.push(format!(
                "`checks.{name}.duration_ms` must be a number; found {}",
                found.shape()
            )),
            None => errors.push(format!(
                "missing required field `checks.{name}.duration_ms`"
            )),
        }
    }

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .expect("crate should live under <repo>/crates/lazuli_codegen_go")
            .to_path_buf()
    }

    fn free_tcp_port() -> u16 {
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .expect("failed to reserve a local TCP port for hostpoint-mini");
        listener
            .local_addr()
            .expect("reserved listener should have a local address")
            .port()
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
                .join("lazuli-go-smoke-health-schema")
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

    struct ProcessGuard {
        child: Child,
        stdout: PathBuf,
        stderr: PathBuf,
    }

    impl ProcessGuard {
        fn spawn(binary: &Path, out_dir: &Path, addr: &str) -> Self {
            let stdout = out_dir.join("hostpoint-mini.stdout.log");
            let stderr = out_dir.join("hostpoint-mini.stderr.log");
            let stdout_file = fs::File::create(&stdout)
                .unwrap_or_else(|err| panic!("creating {}: {err}", stdout.display()));
            let stderr_file = fs::File::create(&stderr)
                .unwrap_or_else(|err| panic!("creating {}: {err}", stderr.display()));
            let db_url = env::var("LAZULI_DB").unwrap_or_else(|_| {
                "postgres://lazuli:lazuli@localhost:5432/lazuli?sslmode=disable".to_owned()
            });

            let child = Command::new(binary)
                .current_dir(out_dir)
                .env("LAZULI_ADDR", addr)
                .env("LAZULI_DB", db_url)
                .stdout(Stdio::from(stdout_file))
                .stderr(Stdio::from(stderr_file))
                .spawn()
                .unwrap_or_else(|err| panic!("failed to start {}: {err}", binary.display()));

            Self {
                child,
                stdout,
                stderr,
            }
        }

        fn try_wait(&mut self) -> Option<std::process::ExitStatus> {
            self.child
                .try_wait()
                .expect("failed to poll hostpoint-mini process")
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
        body: String,
    }

    #[derive(Debug)]
    enum JsonValue {
        Null,
        Bool(bool),
        Number(f64),
        String(String),
        Array(Vec<JsonValue>),
        Object(BTreeMap<String, JsonValue>),
    }

    impl JsonValue {
        fn as_object(&self) -> Option<&BTreeMap<String, JsonValue>> {
            match self {
                JsonValue::Object(value) => Some(value),
                _ => None,
            }
        }

        fn shape(&self) -> String {
            match self {
                JsonValue::Null => "null".to_owned(),
                JsonValue::Bool(value) => format!("bool({value})"),
                JsonValue::Number(value) => format!("number({value})"),
                JsonValue::String(value) => format!("string({value:?})"),
                JsonValue::Array(values) => {
                    let inner = values
                        .iter()
                        .map(JsonValue::shape)
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!("[{inner}]")
                }
                JsonValue::Object(fields) => {
                    let inner = fields
                        .iter()
                        .map(|(key, value)| format!("{key}: {}", value.shape()))
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!("{{{inner}}}")
                }
            }
        }
    }

    struct JsonParser<'a> {
        input: &'a [u8],
        pos: usize,
    }

    impl<'a> JsonParser<'a> {
        fn parse(input: &'a str) -> Result<JsonValue, String> {
            let mut parser = Self {
                input: input.as_bytes(),
                pos: 0,
            };
            let value = parser.parse_value()?;
            parser.skip_ws();
            if parser.pos != parser.input.len() {
                return Err(format!("trailing data at byte {}", parser.pos));
            }
            Ok(value)
        }

        fn parse_value(&mut self) -> Result<JsonValue, String> {
            self.skip_ws();
            match self.peek() {
                Some(b'n') => self.parse_literal(b"null", JsonValue::Null),
                Some(b't') => self.parse_literal(b"true", JsonValue::Bool(true)),
                Some(b'f') => self.parse_literal(b"false", JsonValue::Bool(false)),
                Some(b'"') => self.parse_string().map(JsonValue::String),
                Some(b'{') => self.parse_object(),
                Some(b'[') => self.parse_array(),
                Some(b'-' | b'0'..=b'9') => self.parse_number(),
                Some(byte) => Err(format!(
                    "unexpected byte {:?} at byte {}",
                    byte as char, self.pos
                )),
                None => Err("unexpected end of input".to_owned()),
            }
        }

        fn parse_literal(&mut self, literal: &[u8], value: JsonValue) -> Result<JsonValue, String> {
            if self.input[self.pos..].starts_with(literal) {
                self.pos += literal.len();
                Ok(value)
            } else {
                Err(format!(
                    "expected literal {} at byte {}",
                    String::from_utf8_lossy(literal),
                    self.pos
                ))
            }
        }

        fn parse_object(&mut self) -> Result<JsonValue, String> {
            self.expect(b'{')?;
            let mut fields = BTreeMap::new();
            self.skip_ws();
            if self.consume(b'}') {
                return Ok(JsonValue::Object(fields));
            }

            loop {
                self.skip_ws();
                let key = self.parse_string()?;
                self.skip_ws();
                self.expect(b':')?;
                let value = self.parse_value()?;
                fields.insert(key, value);
                self.skip_ws();
                if self.consume(b'}') {
                    break;
                }
                self.expect(b',')?;
            }

            Ok(JsonValue::Object(fields))
        }

        fn parse_array(&mut self) -> Result<JsonValue, String> {
            self.expect(b'[')?;
            let mut values = Vec::new();
            self.skip_ws();
            if self.consume(b']') {
                return Ok(JsonValue::Array(values));
            }

            loop {
                values.push(self.parse_value()?);
                self.skip_ws();
                if self.consume(b']') {
                    break;
                }
                self.expect(b',')?;
            }

            Ok(JsonValue::Array(values))
        }

        fn parse_string(&mut self) -> Result<String, String> {
            self.expect(b'"')?;
            let mut value = String::new();

            while let Some(byte) = self.next() {
                match byte {
                    b'"' => return Ok(value),
                    b'\\' => value.push(self.parse_escape()?),
                    0x00..=0x1f => {
                        return Err(format!("unescaped control byte at byte {}", self.pos - 1));
                    }
                    _ => {
                        let start = self.pos - 1;
                        while let Some(next) = self.peek() {
                            if next == b'"' || next == b'\\' || next <= 0x1f {
                                break;
                            }
                            self.pos += 1;
                        }
                        let chunk = std::str::from_utf8(&self.input[start..self.pos])
                            .map_err(|err| format!("invalid UTF-8 string chunk: {err}"))?;
                        value.push_str(chunk);
                    }
                }
            }

            Err("unterminated string".to_owned())
        }

        fn parse_escape(&mut self) -> Result<char, String> {
            match self.next() {
                Some(b'"') => Ok('"'),
                Some(b'\\') => Ok('\\'),
                Some(b'/') => Ok('/'),
                Some(b'b') => Ok('\u{0008}'),
                Some(b'f') => Ok('\u{000c}'),
                Some(b'n') => Ok('\n'),
                Some(b'r') => Ok('\r'),
                Some(b't') => Ok('\t'),
                Some(b'u') => self.parse_unicode_escape(),
                Some(byte) => Err(format!(
                    "invalid escape {:?} at byte {}",
                    byte as char,
                    self.pos - 1
                )),
                None => Err("unterminated escape".to_owned()),
            }
        }

        fn parse_unicode_escape(&mut self) -> Result<char, String> {
            let mut code = 0u32;
            for _ in 0..4 {
                let byte = self
                    .next()
                    .ok_or_else(|| "unterminated unicode escape".to_owned())?;
                code = code * 16
                    + match byte {
                        b'0'..=b'9' => u32::from(byte - b'0'),
                        b'a'..=b'f' => u32::from(byte - b'a' + 10),
                        b'A'..=b'F' => u32::from(byte - b'A' + 10),
                        _ => {
                            return Err(format!(
                                "invalid unicode escape byte {:?} at byte {}",
                                byte as char,
                                self.pos - 1
                            ));
                        }
                    };
            }
            char::from_u32(code).ok_or_else(|| format!("invalid unicode scalar U+{code:04X}"))
        }

        fn parse_number(&mut self) -> Result<JsonValue, String> {
            let start = self.pos;
            if self.consume(b'-') {
                self.require_digit()?;
            }

            if self.consume(b'0') {
                // Leading zero is the whole integer part.
            } else {
                self.require_digit()?;
                while self.consume_digit() {}
            }

            if self.consume(b'.') {
                self.require_digit()?;
                while self.consume_digit() {}
            }

            if self.consume(b'e') || self.consume(b'E') {
                let _ = self.consume(b'+') || self.consume(b'-');
                self.require_digit()?;
                while self.consume_digit() {}
            }

            let raw = std::str::from_utf8(&self.input[start..self.pos])
                .map_err(|err| format!("invalid UTF-8 number: {err}"))?;
            let value = raw
                .parse::<f64>()
                .map_err(|err| format!("invalid number {raw:?}: {err}"))?;
            Ok(JsonValue::Number(value))
        }

        fn require_digit(&mut self) -> Result<(), String> {
            if self.consume_digit() {
                Ok(())
            } else {
                Err(format!("expected digit at byte {}", self.pos))
            }
        }

        fn consume_digit(&mut self) -> bool {
            matches!(self.peek(), Some(b'0'..=b'9')) && {
                self.pos += 1;
                true
            }
        }

        fn skip_ws(&mut self) {
            while matches!(self.peek(), Some(b' ' | b'\n' | b'\r' | b'\t')) {
                self.pos += 1;
            }
        }

        fn expect(&mut self, byte: u8) -> Result<(), String> {
            if self.consume(byte) {
                Ok(())
            } else {
                Err(format!("expected {:?} at byte {}", byte as char, self.pos))
            }
        }

        fn consume(&mut self, byte: u8) -> bool {
            if self.peek() == Some(byte) {
                self.pos += 1;
                true
            } else {
                false
            }
        }

        fn next(&mut self) -> Option<u8> {
            let byte = self.peek()?;
            self.pos += 1;
            Some(byte)
        }

        fn peek(&self) -> Option<u8> {
            self.input.get(self.pos).copied()
        }
    }
}
