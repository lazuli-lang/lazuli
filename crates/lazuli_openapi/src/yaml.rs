//! Tiny purpose-built YAML emitter and string quoting helpers.
//!
//! Indent-prefixed lines with two-space steps. Avoids pulling in
//! `serde_yaml` so the build graph stays tight. All other modules in
//! this crate emit through this single sink so the wire output stays
//! consistent.

/// Quote an OpenAPI key only when it carries spaces or colons. Paths are
/// the usual reason a key needs quoting; ordinary identifiers stay bare.
pub(crate) fn quote_key(k: &str) -> String {
    if k.contains(' ') || k.contains(':') {
        format!("\"{}\"", k)
    } else {
        k.to_owned()
    }
}

/// Always-quoted string value with embedded double-quotes escaped.
pub(crate) fn quote_value(v: &str) -> String {
    format!("\"{}\"", v.replace('"', "\\\""))
}

pub(crate) struct YamlEmitter {
    buf: String,
    depth: usize,
}

impl YamlEmitter {
    pub(crate) fn new() -> Self {
        Self {
            buf: String::with_capacity(4096),
            depth: 0,
        }
    }

    pub(crate) fn indent(&mut self) {
        self.depth += 1;
    }

    pub(crate) fn dedent(&mut self) {
        if self.depth > 0 {
            self.depth -= 1;
        }
    }

    pub(crate) fn line(&mut self, s: &str) {
        for _ in 0..self.depth {
            self.buf.push_str("  ");
        }
        self.buf.push_str(s);
        self.buf.push('\n');
    }

    pub(crate) fn kv(&mut self, k: &str, v: &str) {
        self.line(&format!("{}: {}", k, v));
    }

    pub(crate) fn kv_quoted(&mut self, k: &str, v: &str) {
        self.line(&format!("{}: {}", k, quote_value(v)));
    }

    pub(crate) fn into_string(self) -> String {
        self.buf
    }
}
