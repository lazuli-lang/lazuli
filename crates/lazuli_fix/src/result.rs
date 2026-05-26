//! Output payload produced by a fix application.
//!
//! Every action returns the same shape regardless of preview vs. apply;
//! the discriminator is [`FixOutcome`]. Serializable so the CLI and LSP
//! can exchange results over JSON.

use serde::{Deserialize, Serialize};

/// Result of one [`crate::FixRegistry::execute`] call.
///
/// Always populated with a `preview`, even on [`FixOutcome::Applied`],
/// so surfaces that want to display the patch don't need to re-run the
/// fix.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixResult {
    /// What the action did (or didn't do).
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

/// Discriminator over the four ways a fix action can complete.
///
/// `Applied` and `Preview` are the success cases; `NoChange` and
/// `Skipped` are non-error no-ops the surface should surface verbatim
/// instead of treating as failure.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn result_round_trips_through_json_with_snake_case_outcome() {
        let result = FixResult {
            outcome: FixOutcome::NoChange,
            preview: String::new(),
            note: Some("already idempotent".to_owned()),
        };
        let json = serde_json::to_string(&result).expect("result serializes");
        assert!(json.contains("no_change"));
        let back: FixResult = serde_json::from_str(&json).expect("result deserializes");
        assert_eq!(back.outcome, FixOutcome::NoChange);
        assert_eq!(back.note.as_deref(), Some("already idempotent"));
    }
}
