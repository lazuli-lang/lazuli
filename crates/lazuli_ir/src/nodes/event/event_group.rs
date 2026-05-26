//! Phase L Tier 3 — `event_group <pattern> on <Resource>` declarations.
//!
//! Declares a *family* of names under a glob pattern with a shared payload
//! spine; concrete events authored under the group inherit the spine and
//! contribute their own payload shape via [`EventVariant`]. Doctor walks the
//! pattern to bind concrete events. [`EventGroup::raw_payload`] is a verbatim
//! string list until Tier 4 lifts to typed `EventField`.

use serde::{Deserialize, Serialize};

use crate::SpanRef;

use super::{EventField, OutboxMode};

/// Phase L Tier 3 — `event_group <pattern> on <Resource>` declaration.
///
/// The pattern is a glob (`customer_*`) the doctor uses to bind
/// concrete events authored under the group. The lifted IR records the
/// pattern + the owning resource verbatim; the payload block is
/// captured as a raw string list (Tier 4 lifts to typed event-field
/// projection).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventGroup {
    /// `customer_*` — glob pattern matched against event names.
    pub pattern: String,
    /// `on Customer` — owning resource type. `None` for resource-free
    /// groups (none in the fixture today; the field stays optional for
    /// forward compatibility).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_resource: Option<String>,
    /// `payload` child lines captured verbatim. Tier 4 lifts into typed
    /// `EventField`/`Expr` shapes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub raw_payload: Vec<String>,
    /// `audit ...` line captured verbatim. None when not authored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_audit: Option<String>,
    /// Concrete events authored directly under this group, identified
    /// by name only. The actual event records remain attached to
    /// `Feature.events`; this slot records the inheritance link so
    /// doctor can run `EVENTGROUP-NESTING-001` and the pattern-prefix
    /// rule (`event_group_can_own_short_event_declarations`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<String>,
    /// EVENT-OUTBOX §3.3 — per-event outbox mode, parallel to `events`.
    /// `events_outbox[i]` is the mode authored on `events[i]` (or
    /// `OutboxMode::None` when the line did not author one).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub events_outbox: Vec<OutboxMode>,
    /// B5 framework gap 1 — per-event typed payload variants. One
    /// entry per `event <name>` authored under the group, carrying
    /// the kind (committed/trace) and the typed payload fields lifted
    /// from the per-event body. When `variants` is non-empty the
    /// codegen prefers the typed projection over the legacy
    /// `Feature.events` lookup. Back-compat: legacy fixtures that
    /// author only `event foo` lines lower as variants with empty
    /// `fields` and `kind = Committed`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variants: Vec<EventVariant>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// B5 framework gap 1 — one variant under an `EventGroup`. Carries
/// the kind (`event <name>` vs `event.trace <name>`), the typed
/// payload fields, and a span back to the source line for diagnostics.
///
/// Reuses `EventField` (the same shape standalone `Feature.events`
/// use) so codegen and doctor can read variant fields with a single
/// path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventVariant {
    /// Short name as authored under the group (e.g. `confirmed` for
    /// `event confirmed`). The full prefixed name is computed by
    /// codegen using the group pattern (`charge_*` + `confirmed` ->
    /// `charge_confirmed`).
    pub name: String,
    /// Closed catalog: committed (`event`) or trace (`event.trace`).
    pub kind: EventVariantKind,
    /// EVENT-OUTBOX §3.3 — `outbox guaranteed` flag authored on the
    /// variant header. Mirrors the parallel `events_outbox` slot but
    /// reaches the codegen directly from the variant record so it
    /// does not need to index two vectors.
    #[serde(default, skip_serializing_if = "OutboxMode::is_none")]
    pub outbox: OutboxMode,
    /// Typed payload fields lifted from the variant body. Empty when
    /// the variant was authored without a field body (back-compat).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<EventField>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// B5 framework gap 1 — closed catalog of event-variant kinds.
/// Mirrors `EventKind` (the catalog already used by `Feature.events`)
/// but is duplicated to keep the `EventGroup`-side surface
/// self-contained.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventVariantKind {
    /// `event <name>` — committed domain bus variant.
    Committed,
    /// `event.trace <name>` — observability-only trace variant.
    Trace,
}

impl EventVariantKind {
    /// Returns `true` for trace-only variants (observability stream).
    ///
    /// ## Examples
    ///
    /// ```
    /// use lazuli_ir::EventVariantKind;
    ///
    /// assert!(EventVariantKind::Trace.is_trace());
    /// assert!(!EventVariantKind::Committed.is_trace());
    /// ```
    pub fn is_trace(&self) -> bool {
        matches!(self, EventVariantKind::Trace)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn event_variant_kind_serializes_snake_case() {
        assert_eq!(
            serde_json::to_value(EventVariantKind::Committed).unwrap(),
            json!("committed")
        );
        assert!(EventVariantKind::Trace.is_trace());
        assert!(!EventVariantKind::Committed.is_trace());
    }

    #[test]
    fn event_group_round_trips_with_empty_optional_slots() {
        let g = EventGroup {
            pattern: "charge_*".to_owned(),
            on_resource: Some("Charge".to_owned()),
            raw_payload: vec![],
            raw_audit: None,
            events: vec!["confirmed".to_owned()],
            events_outbox: vec![OutboxMode::Guaranteed],
            variants: vec![],
            span_ref: None,
        };
        let v = serde_json::to_value(&g).unwrap();
        let back: EventGroup = serde_json::from_value(v).unwrap();
        assert_eq!(back, g);
    }
}
