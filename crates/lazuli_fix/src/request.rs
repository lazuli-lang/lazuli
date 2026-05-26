//! Input payload for a fix application.
//!
//! Mirrors the shape of a `lazuli doctor` finding plus an `apply`
//! toggle that distinguishes preview-only invocations (LSP code
//! actions) from on-disk writes (`lazuli fix --apply`).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// One fix request — produced by the CLI or LSP and consumed by a
/// [`crate::FixRegistry`].
///
/// Serializable so the CLI and LSP can exchange requests over JSON for
/// agent tooling, and so request fixtures can be checked into tests.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixRequest {
    /// Doctor rule code — selects the action to invoke.
    pub rule: String,
    /// `.lzi` / `.lzx` file the fix applies to.
    pub path: PathBuf,
    /// 1-based line anchor (same convention as doctor findings).
    pub line: usize,
    /// 1-based column anchor.
    pub column: usize,
    /// When `true`, the action writes the change to disk. When `false`,
    /// only a preview is produced (used by the LSP for `CodeAction.edit`).
    pub apply: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_round_trips_through_json() {
        let request = FixRequest {
            rule: "TEST-MISSING-AUTHORED-001".to_owned(),
            path: PathBuf::from("examples/full-capsule/full-capsule.lzi"),
            line: 12,
            column: 1,
            apply: false,
        };
        let json = serde_json::to_string(&request).expect("request serializes");
        let back: FixRequest = serde_json::from_str(&json).expect("request deserializes");
        assert_eq!(back.rule, request.rule);
        assert_eq!(back.line, request.line);
        assert_eq!(back.apply, request.apply);
    }
}
