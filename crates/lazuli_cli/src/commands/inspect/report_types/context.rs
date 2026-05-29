//! `--expand=context` (alias `ctx`) composite projection shape.
//!
//! CUT 2 — a fixed "feature context" section catalog that COMPOSES the
//! existing per-axis projectors under one schema. Zero IR change: every
//! section's payload is an already-projected shape, boxed opaquely as a
//! `serde_json::Value`, tagged with a [`ContextStatus`] provenance
//! marker. The point of the projection is to ENUMERATE the full section
//! catalog and mark which sections the compiler can vs cannot derive —
//! the human prose for `prose`/`vault`/`absent` sections lives in the
//! co-located `.ctx.md`, not here.
//!
//! Section → existing-axis map (the status tag is load-bearing):
//!
//! | Section        | Source projector / IR                         | status                 |
//! |----------------|-----------------------------------------------|------------------------|
//! | purpose        | `Feature.purpose`                             | `derived`              |
//! | non_goals      | `Feature.non_goals`                           | `derived`              |
//! | data_model     | resources + enums + records                   | `derived`              |
//! | operations     | commands + queries + apis                     | `derived`              |
//! | contracts      | command inputs + query params + api outputs + records | `derived`      |
//! | errors         | `Feature.errors`                              | `derived`              |
//! | authorization  | policies + auth (text-walkers)                | `derived-via-textwalk` |
//! | events         | events (text-walker)                          | `derived-via-textwalk` |
//! | security       | security (text-walker)                        | `derived-via-textwalk` |
//! | invariants     | resource soft_delete/append_only subset       | `derived` / `absent`   |
//! | code_pointers  | (no feature-level file:line table)            | `absent`               |
//! | test_matrix    | (coverage layers not projected)               | `absent`               |
//! | boundaries     | human prose                                   | `prose`                |
//! | performance    | human prose                                   | `prose`                |
//! | examples       | human prose                                   | `prose`                |
//! | decisions      | knowledge vault `knowledge/decisions/`        | `vault`                |
//!
//! CRITICAL: authorization/events/security carry `derived-via-textwalk`,
//! NOT `derived` — their underlying projectors (`security.rs`,
//! `events.rs`, `policies.rs`) are text-walkers over source lines, not
//! verbatim typed-IR clones. Mislabeling them as clean IR derivation
//! overstates the projection's fidelity.

use serde::Serialize;

/// Provenance marker for a context section. Serialized as a
/// lowercase-kebab string so machine consumers read the status verbatim
/// (`derived-via-textwalk`, not `DerivedViaTextWalk`).
#[derive(Debug, Clone, Copy, Serialize)]
pub(in crate::commands::inspect) enum ContextStatus {
    /// Verbatim typed-IR derivation (the projector clones a lowered IR
    /// shape with no text re-scan).
    #[serde(rename = "derived")]
    Derived,
    /// Derived by a text-walker over the source lines, NOT a verbatim
    /// typed-IR clone. Lower fidelity than `derived` — the projector
    /// re-scans `.lzi` text. Load-bearing for authorization/events/
    /// security.
    #[serde(rename = "derived-via-textwalk")]
    DerivedViaTextWalk,
    /// Human prose the compiler cannot derive; lives in the co-located
    /// `.ctx.md`. The section is enumerated with an empty payload so the
    /// catalog stays complete.
    #[serde(rename = "prose")]
    Prose,
    /// Sourced from the on-disk knowledge vault (e.g.
    /// `knowledge/decisions/`), which this projection does not read.
    #[serde(rename = "vault")]
    Vault,
    /// Sourced from outside the feature IR (reserved; no current section
    /// uses it). Kept in the enum so the status vocabulary is closed.
    #[serde(rename = "external")]
    #[allow(dead_code)]
    External,
    /// No projector / IR surface exists for this section yet. Enumerated
    /// with an empty payload so consumers see the full catalog and know
    /// the compiler cannot derive it today.
    #[serde(rename = "absent")]
    Absent,
}

/// One section of the feature-context catalog: a provenance [`ContextStatus`]
/// plus an opaque, already-projected payload.
///
/// The payload is boxed as `serde_json::Value` so the composite reuses
/// the existing typed projections (purpose string, `Vec<InspectPolicy>`,
/// `InspectSecurity`, …) without re-typing them here. `None` for
/// `prose`/`vault`/`absent` sections and for `derived` sections that the
/// feature simply did not author.
#[derive(Debug, Serialize)]
pub(in crate::commands::inspect) struct InspectContextSection {
    pub(in crate::commands::inspect) status: ContextStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) payload: Option<serde_json::Value>,
}

impl InspectContextSection {
    /// A section whose payload is an already-projected shape. Serializes
    /// the shape to an opaque `serde_json::Value`; serialization failure
    /// degrades to a `None` payload (the status tag still surfaces) so
    /// the composite never flips inspect into an error path.
    pub(in crate::commands::inspect) fn derived_value<T: Serialize>(
        status: ContextStatus,
        value: &T,
    ) -> Self {
        Self {
            status,
            payload: serde_json::to_value(value).ok(),
        }
    }

    /// A section that enumerates a catalog slot the compiler cannot
    /// (or does not here) derive: `prose` / `vault` / `absent`. The
    /// payload is always empty; the status tag carries the meaning.
    pub(in crate::commands::inspect) fn empty(status: ContextStatus) -> Self {
        Self {
            status,
            payload: None,
        }
    }
}

/// The full feature-context section catalog. Every field is always
/// present (the catalog is fixed); the `status` inside each section
/// records whether the compiler derived it, and how.
#[derive(Debug, Serialize)]
pub(in crate::commands::inspect) struct InspectContext {
    pub(in crate::commands::inspect) purpose: InspectContextSection,
    pub(in crate::commands::inspect) non_goals: InspectContextSection,
    pub(in crate::commands::inspect) data_model: InspectContextSection,
    pub(in crate::commands::inspect) operations: InspectContextSection,
    pub(in crate::commands::inspect) contracts: InspectContextSection,
    pub(in crate::commands::inspect) errors: InspectContextSection,
    pub(in crate::commands::inspect) authorization: InspectContextSection,
    pub(in crate::commands::inspect) events: InspectContextSection,
    pub(in crate::commands::inspect) security: InspectContextSection,
    pub(in crate::commands::inspect) invariants: InspectContextSection,
    pub(in crate::commands::inspect) code_pointers: InspectContextSection,
    pub(in crate::commands::inspect) test_matrix: InspectContextSection,
    pub(in crate::commands::inspect) boundaries: InspectContextSection,
    pub(in crate::commands::inspect) performance: InspectContextSection,
    pub(in crate::commands::inspect) examples: InspectContextSection,
    pub(in crate::commands::inspect) decisions: InspectContextSection,
}
