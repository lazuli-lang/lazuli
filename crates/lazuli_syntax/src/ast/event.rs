//! Event-group AST — pattern-grouped event declarations on the IR.
//!
//! `event_group <pattern>` is a feature-scoped block that ties together
//! a set of `event <name>` declarations under one shape contract. The
//! glob pattern (e.g. `customer_*`) is enforced by doctor against the
//! actual `event <name>` headers inside the group.
//!
//! Per-variant payloads (B5 framework gap 1,
//! `docs/proposals/event-group-per-variant-payload.md`) are kept in
//! **parallel arrays** with `events`: index `i` of `events` lines up
//! with index `i` of `events_outbox_guaranteed`, `event_variants`, and
//! `event_variant_kinds`. Empty payloads (zero typed fields) preserve
//! the legacy `event foo` shorthand.
//!
//! Two event kinds today (`EventVariantKindAst`):
//! - `Committed` — authored as `event <name>`, lands on the bus.
//! - `Trace` — authored as `event.trace <name>`, never crosses the bus,
//!   only emitted to telemetry.

use serde::{Deserialize, Serialize};

use super::Span;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventGroup {
    /// `customer_*` glob pattern.
    pub pattern: String,
    /// `on Customer` — owning resource type.
    pub on_resource: Option<String>,
    /// `payload` child lines captured verbatim.
    pub payload: Vec<String>,
    /// `audit ...` line captured verbatim.
    pub audit: Option<String>,
    /// Concrete `event <name>` headers under this group, recorded as
    /// name strings. The full event bodies stay in the legacy lowering
    /// pipeline; this slot drives doctor's pattern-prefix rule.
    pub events: Vec<String>,
    /// EVENT-OUTBOX §3.3 — parallel to `events`: `true` at index `i`
    /// when the corresponding `event <name>` block authored
    /// `outbox guaranteed`. Length always matches `events.len()`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub events_outbox_guaranteed: Vec<bool>,
    /// B5 framework gap 1 — per-event typed payload field bodies.
    /// Parallel to `events`: `event_variants[i]` holds the typed-field
    /// rows authored under `events[i]`. Each entry is an
    /// `EventVariantFieldDecl` (name + type-literal + required/optional).
    /// When an event was authored without a field body, the inner Vec
    /// is empty (preserves back-compat with the `event foo` shorthand).
    /// Lifted into typed `EventVariant` records by the analyzer; see
    /// `docs/proposals/event-group-per-variant-payload.md`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub event_variants: Vec<Vec<EventVariantFieldDecl>>,
    /// B5 framework gap 1 — parallel to `events`: closed catalog of
    /// the keyword authored on the event header. Distinguishes
    /// `event <name>` (Committed) from `event.trace <name>` (Trace) so
    /// the analyzer can lower into the correct `EventKind`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub event_variant_kinds: Vec<EventVariantKindAst>,
    pub span: Span,
}

/// B5 framework gap 1 — per-event variant kind on the AST surface.
/// Mirrors the `ir::EventKind` catalog so the parser stays decoupled
/// from the IR while the analyzer can lift losslessly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventVariantKindAst {
    /// Authored as `event <name>` — committed bus variant.
    Committed,
    /// Authored as `event.trace <name>` — trace-only variant.
    Trace,
}

/// B5 framework gap 1 — a single typed field row inside an
/// `event_group`'s `event <name>` body. Mirrors the surface shape of
/// `ResourceFieldDecl` but keeps the slot count minimal because event
/// payloads are projection-only (no defaults, no constraints, no
/// `unique`/`slug`/`@full_text`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventVariantFieldDecl {
    /// Field name as authored.
    pub name: String,
    /// Type literal verbatim (`Text`, `@semantic.Money`, `ID`, ...).
    /// Lifted to `ir::TypeRef` via `type_ref_from_syntax` on lowering.
    pub type_text: String,
    /// `required` modifier authored.
    pub required: bool,
    /// `optional` modifier authored.
    pub optional: bool,
    pub span: Span,
}
