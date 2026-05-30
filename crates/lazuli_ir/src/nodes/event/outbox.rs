//! `OutboxMode` — EVENT-OUTBOX §3.3 per-event transactional outbox guarantee.
//!
//! Closed catalog for the per-event outbox guarantee. The default is
//! [`OutboxMode::None`] (legacy best-effort dispatch); [`OutboxMode::Guaranteed`]
//! opts the event into the transactional outbox: the producing command's pgx
//! tx writes a `lazuli_outbox` row in the same commit as the resource mutation;
//! the runtime pump dispatches post-commit.

use serde::{Deserialize, Serialize};

/// EVENT-OUTBOX §3.3 — closed catalog for the per-event outbox
/// guarantee. The default is `None` (legacy best-effort dispatch);
/// `Guaranteed` opts the event into the transactional outbox.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum OutboxMode {
    #[default]
    None,
    Guaranteed,
}

impl OutboxMode {
    /// Returns `true` for the legacy best-effort dispatch path. Used by
    /// `skip_serializing_if` so the default round-trips cleanly.
    ///
    /// ## Examples
    ///
    /// ```
    /// use lazuli_ir::OutboxMode;
    ///
    /// assert!(OutboxMode::None.is_none());
    /// ```
    pub fn is_none(&self) -> bool {
        matches!(self, OutboxMode::None)
    }
    /// Returns `true` when the event opted into the transactional outbox.
    ///
    /// ## Examples
    ///
    /// ```
    /// use lazuli_ir::OutboxMode;
    ///
    /// assert!(OutboxMode::Guaranteed.is_guaranteed());
    /// ```
    pub fn is_guaranteed(&self) -> bool {
        matches!(self, OutboxMode::Guaranteed)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn outbox_mode_default_is_none() {
        assert_eq!(OutboxMode::default(), OutboxMode::None);
        assert!(OutboxMode::None.is_none());
        assert!(OutboxMode::Guaranteed.is_guaranteed());
    }

    #[test]
    fn outbox_mode_serializes_snake_case() {
        assert_eq!(
            serde_json::to_value(OutboxMode::Guaranteed).unwrap(),
            json!("guaranteed")
        );
    }
}
