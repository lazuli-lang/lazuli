//! Output payload produced by a fix application.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixResult {
    pub outcome: FixOutcome,
    /// Human-readable preview of the patch that was (or would have been)
    /// applied. Always populated for surfaces that want to display it
    /// without re-running the fix.
    pub preview: String,
    /// Free-form note for the user / agent (e.g. why the fix could not
    /// be applied automatically).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FixOutcome {
    /// The action wrote the change to disk.
    Applied,
    /// The action produced a preview only (`apply: false`).
    Preview,
    /// The action recognized the rule but determined no change was
    /// needed (idempotent re-runs land here).
    NoChange,
    /// The action could not run — either the file doesn't match the
    /// fix's preconditions or the rule has no registered action.
    Skipped,
}
