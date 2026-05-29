//! `--expand=knowledge` projection shape (iron-hand context).
//!
//! The feature intent triad — `purpose`, `non_goals`, and the
//! `knowledge` sector slug — grouped into one axis read entirely from
//! the lowered IR (`Feature.purpose` / `Feature.non_goals` /
//! `Feature.knowledge`). This is the "memory is the compiler"
//! projection: the first surface that co-locates a feature's declared
//! intent with the sector it draws knowledge from.
//!
//! v0 scope is IR-only. Reading the on-disk
//! `.lazuli/knowledge/<sector>/` vault (gold docs, tiers, decay) is a
//! later/runtime concern, gated by the planned `VOCAB-KNOWLEDGE-*`
//! doctor lints. See `docs/proposals/knowledge-sector-field.md`.

use serde::Serialize;

#[derive(Debug, Serialize)]
pub(in crate::commands::inspect) struct InspectKnowledge {
    /// `purpose "<sentence>"` text, verbatim. `None` when the feature
    /// declares no `purpose` line.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) purpose: Option<String>,
    /// `non_goals` entries, each lifted into `{ key, description }`.
    /// Empty when the feature declares no `non_goals` block.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(in crate::commands::inspect) non_goals: Vec<lazuli_ir::NonGoal>,
    /// `knowledge <sector>` slug. `None` when the feature declares no
    /// `knowledge` line.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) sector: Option<String>,
}
