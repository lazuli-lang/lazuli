//! Pure-leaf parsers, predicates, and pretty-printers used across
//! doctor diagnostics.
//!
//! Every function here is a "no allocation surprise, no IO, no
//! dependency on `DoctorPackage` or `DoctorDiagnostic`" leaf — exactly
//! the shape that survives an extraction in isolation. They were
//! historically scattered across `doctor/mod.rs` next to the rule
//! that first needed them; collecting them here cuts ~150 LOC off
//! the dispatch file and gives future rules an obvious place to land
//! similarly-shaped helpers.
//!
//! Three rough clusters:
//!
//!   - **Wire format parsers** — `parse_iso_date`,
//!     `is_parseable_duration` / `_size` / `_cidr`. The lazy-acceptance
//!     contract is documented at each call site: the Go runtime
//!     re-parses at wire time and surfaces real errors; doctor only
//!     catches the obvious typo (missing slash, malformed digits).
//!   - **IR vocabulary mappers** — `http_method_word`,
//!     `tool_kind_word`, `format_visibility`. Convert closed IR
//!     enums into the canonical wire-string used by both diagnostic
//!     messages and the JSON envelope.
//!   - **Pretty-printers** — `catalog_list`, `environments_summary`,
//!     `format_accept_list`. Format `BTreeSet<&str>` /
//!     `Vec<MimeType>` into the comma-separated catalog strings used
//!     by the message bodies. Centralizing them keeps the
//!     "what does an enumeration look like in a diagnostic?"
//!     contract uniform.
//!
//! Plus a small cluster of path / version predicates that compose
//! over `Path` and version strings (`is_lzi_path`, `is_lzx_path`,
//! `major_minor`, `is_one_dot_zero_plus`, `normalise_path`,
//! `same_origin`).

use std::collections::BTreeSet;
use std::path::Path;

use lazuli_ir as ir;

/// `true` when the path's extension is `.lzi`. The doctor walker uses
/// extension-based dispatch — `app.lzi`, `registry.lzi`, and feature
/// files all share the `.lzi` suffix.
pub(super) fn is_lzi_path(path: &Path) -> bool {
    path.extension().and_then(|ext| ext.to_str()) == Some("lzi")
}

/// `true` when the path's extension is `.lzx` (experience / surface
/// projection files).
pub(super) fn is_lzx_path(path: &Path) -> bool {
    path.extension().and_then(|ext| ext.to_str()) == Some("lzx")
}

/// Reduce a SemVer-shaped string to its `major.minor` prefix. Used by
/// `LAZULI-VERSION-001` to compare the manifest pin against the IR
/// schema version while ignoring patch jitter.
pub(super) fn major_minor(version: &str) -> String {
    let mut parts = version.split('.');
    let Some(major) = parts.next() else {
        return version.to_owned();
    };
    let Some(minor) = parts.next() else {
        return version.to_owned();
    };
    format!("{major}.{minor}")
}

/// `true` when the version string's major component parses as `>= 1`.
/// `LAZULI-VERSION-002` uses this to detect 1.0+ projects that should
/// no longer carry the pre-1.0 schema escape hatches.
pub(super) fn is_one_dot_zero_plus(version: &str) -> bool {
    version
        .split('.')
        .next()
        .and_then(|major| major.parse::<u64>().ok())
        .is_some_and(|major| major >= 1)
}

/// Strip path parameters down to `:_` placeholders so two routes that
/// differ only in slot names (`/users/:id` vs `/users/:userId`) compare
/// equal. Preserves the leading `/`.
pub(super) fn normalise_path(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for segment in path.split('/') {
        if !out.is_empty() {
            out.push('/');
        } else if path.starts_with('/') {
            // preserve leading `/`
        }
        if let Some(_name) = segment.strip_prefix(':') {
            out.push_str(":_");
        } else {
            out.push_str(segment);
        }
    }
    if path.starts_with('/') && !out.starts_with('/') {
        format!("/{out}")
    } else {
        out
    }
}

/// IR HTTP method → canonical wire word. Closed catalog: the IR enum
/// is exhaustive.
pub(super) fn http_method_word(method: ir::HttpMethod) -> &'static str {
    match method {
        ir::HttpMethod::Get => "GET",
        ir::HttpMethod::Post => "POST",
        ir::HttpMethod::Put => "PUT",
        ir::HttpMethod::Patch => "PATCH",
        ir::HttpMethod::Delete => "DELETE",
    }
}

/// IR `ToolKind` → canonical wire word used by `agent_tool_diagnostics`
/// and the agent-tool MCP surface.
pub(super) fn tool_kind_word(kind: ir::ToolKind) -> &'static str {
    match kind {
        ir::ToolKind::QueryList => "query.list",
        ir::ToolKind::QueryLookup => "query.lookup",
        ir::ToolKind::QuerySql => "query.sql",
        ir::ToolKind::QueryView => "query.view",
        ir::ToolKind::QueryUnspecified => "query",
        ir::ToolKind::Command => "command",
        ir::ToolKind::Api => "api",
    }
}

/// IR `FileVisibility` → canonical wire word for `@cap.File` /
/// `cap_file_*` diagnostics.
pub(super) fn format_visibility(v: lazuli_ir::FileVisibility) -> &'static str {
    match v {
        lazuli_ir::FileVisibility::Public => "public",
        lazuli_ir::FileVisibility::Private => "private",
        lazuli_ir::FileVisibility::Signed => "signed",
    }
}

/// Format a list of MIME types as the canonical pipe-separated wire
/// string (`image/png|image/jpeg|application/pdf`).
pub(super) fn format_accept_list(accept: &[lazuli_ir::MimeType]) -> String {
    accept
        .iter()
        .map(|m| format!("{}/{}", m.family, m.subtype))
        .collect::<Vec<_>>()
        .join("|")
}

/// Render an environment set as the human-readable list used by the
/// profile / runtime contract diagnostics. Returns `"none declared"`
/// when the set is empty so messages still parse cleanly.
pub(super) fn environments_summary(environments: &BTreeSet<&str>) -> String {
    if environments.is_empty() {
        "none declared".to_owned()
    } else {
        environments
            .iter()
            .map(|e| format!("`{e}`"))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// Format a closed catalog (`["a", "b", "c"]`) as the inline
/// backtick-wrapped list used by diagnostic messages
/// (`expected one of \`a\`, \`b\`, \`c\``).
pub(super) fn catalog_list(items: &[&str]) -> String {
    items
        .iter()
        .map(|i| format!("`{i}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Parse an ISO-8601 date (`YYYY-MM-DD`) into a `(year, month, day)`
/// tuple. The tuple sorts lexically because each component is
/// fixed-width — that's the contract relied on by the OpenAPI
/// `sunset_in_past` comparison. Returns `None` for malformed input;
/// the doctor falls back to a deterministic pivot in that case.
///
/// `today_pivot` is lexical (the tuple sorts as if it were a real date
/// because each component is fixed-width).
pub(super) fn parse_iso_date(s: &str) -> Option<(u16, u8, u8)> {
    let trimmed = s.trim();
    let bytes = trimmed.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return None;
    }
    let year: u16 = trimmed[0..4].parse().ok()?;
    let month: u8 = trimmed[5..7].parse().ok()?;
    let day: u8 = trimmed[8..10].parse().ok()?;
    if !(1..=12).contains(&month) {
        return None;
    }
    if !(1..=31).contains(&day) {
        return None;
    }
    Some((year, month, day))
}

/// Calendar pivot the OpenAPI `sunset_in_past` rule compares against.
/// The runtime context exposes no `chrono` dependency; we anchor the
/// pivot at the current Lazuli development date so the diagnostic is
/// deterministic across runs. Bump alongside the canonical fixture
/// each cycle; in practice the day-of-month precision is sufficient.
pub(super) fn openapi_today_pivot() -> (u16, u8, u8) {
    (2026, 5, 11)
}

/// Liberal duration parser. Accepts `<digits><suffix>` where suffix is
/// one of `ms | s | m | h | d`. The Go runtime re-parses with
/// `time.ParseDuration` at wire time; this check just catches the
/// obvious typo (empty, no suffix, garbage prefix).
pub(super) fn is_parseable_duration(raw: &str) -> bool {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return false;
    }
    let suffixes = ["ms", "s", "m", "h", "d"];
    for suffix in suffixes {
        if let Some(head) = trimmed.strip_suffix(suffix) {
            if !head.is_empty() && head.chars().all(|c| c.is_ascii_digit()) {
                return true;
            }
        }
    }
    false
}

/// Liberal size parser. Matches the common Go idiom (`512b`, `16kb`,
/// `10mb`, `2gb`). The numeric prefix must be a positive integer; the
/// suffix is one of `b | kb | mb | gb | tb`.
pub(super) fn is_parseable_size(raw: &str) -> bool {
    let trimmed = raw.trim().to_ascii_lowercase();
    if trimmed.is_empty() {
        return false;
    }
    let suffixes = ["tb", "gb", "mb", "kb", "b"];
    for suffix in suffixes {
        if let Some(head) = trimmed.strip_suffix(suffix) {
            if !head.is_empty() && head.chars().all(|c| c.is_ascii_digit()) {
                return true;
            }
        }
    }
    false
}

/// Liberal CIDR parser. Accepts IPv4 (`a.b.c.d/n`, `0 ≤ n ≤ 32`) and
/// IPv6 (`prefix::/n`, `0 ≤ n ≤ 128`). We don't need full RFC 4632
/// canonicalization at this layer — the Go runtime parses via
/// `netip.ParsePrefix` at wire time and surfaces real errors there.
/// This check just catches the obvious typo (missing slash, garbage
/// prefix length).
pub(super) fn is_parseable_cidr(raw: &str) -> bool {
    let Some((addr, mask)) = raw.split_once('/') else {
        return false;
    };
    if addr.is_empty() || mask.is_empty() {
        return false;
    }
    let Ok(prefix_len) = mask.parse::<u32>() else {
        return false;
    };
    if addr.contains(':') {
        prefix_len <= 128
    } else {
        let octets: Vec<&str> = addr.split('.').collect();
        if octets.len() != 4 {
            return false;
        }
        for octet in &octets {
            let Ok(value) = octet.parse::<u32>() else {
                return false;
            };
            if value > 255 {
                return false;
            }
        }
        prefix_len <= 32
    }
}

/// Compare two URLs by scheme + host (ignoring path, query, port
/// where absent). A declared `url` is the canonical reference; the
/// origin must match its scheme + authority for the CORS layer to
/// recognise it as the same browser origin.
pub(super) fn same_origin(declared_url: &str, origin: &str) -> bool {
    let canon = |raw: &str| {
        let raw = raw.trim();
        // Strip path / query — keep scheme + authority only.
        let cut = raw
            .find("://")
            .and_then(|idx| {
                let after = &raw[idx + 3..];
                let tail_start = after.find('/').map(|p| idx + 3 + p);
                tail_start.map(|p| raw[..p].to_owned())
            })
            .unwrap_or_else(|| raw.to_owned());
        cut.trim_end_matches('/').to_owned()
    };
    canon(declared_url) == canon(origin)
}
