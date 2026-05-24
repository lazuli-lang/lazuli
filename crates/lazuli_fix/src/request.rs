//! Input payload for a fix application.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

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
