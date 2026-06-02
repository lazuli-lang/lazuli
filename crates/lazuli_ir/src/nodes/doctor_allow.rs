//! `@doctor.allow(CODE, reason: "...")` — the first-class waiver node (spec 0028).
//!
//! A doctor waiver used to live ONLY in a `# doctor:allow <CODE>` comment,
//! recovered by re-scanning raw `.lzi`/`.lzx` source. That made a waiver
//! indistinguishable from prose to the AST/IR. Spec 0028 adds a first-class
//! line annotation, `@doctor.allow(CODE, reason: "...")`, that the parser
//! captures into [`crate::Module::doctor_allows`] — a queryable, structured
//! side-channel.
//!
//! This module holds the **FROZEN contract** (spec 0028 ADR §3): [`DoctorAllow`]
//! + [`DoctorAllowScope`]. Spec 0029 (comment-discipline) builds on this shape;
//! do not change it after 0028 lands.
//!
//! ## Capture, don't interpret
//!
//! The parser records `{ code, reason, scope, legacy, span }` verbatim. It does
//! NOT validate `code` against the rule registry (the doctor owns rule identity)
//! and does NOT alter any construct. `legacy: true` marks a waiver recovered
//! from the deprecated `# doctor:allow` comment form (honored during the
//! migration window).

use serde::{Deserialize, Serialize};

use crate::Span;

/// One captured doctor waiver — a `@doctor.allow(CODE, reason: "...")` node
/// OR a legacy `# doctor:allow <CODE>` comment lifted into the same shape.
///
/// FROZEN contract (spec 0028). Spec 0029 reads this to exempt waiver lines.
///
/// ## Examples
///
/// ```rust
/// use lazuli_ir::{DoctorAllow, DoctorAllowScope};
///
/// let waiver = DoctorAllow {
///     code: "LZI-FILE-SIZE-001".to_owned(),
///     reason: Some("generated table".to_owned()),
///     scope: DoctorAllowScope::File,
///     legacy: false,
///     span: None,
/// };
/// assert_eq!(waiver.code, "LZI-FILE-SIZE-001");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DoctorAllow {
    /// The rule code being waived, verbatim (case preserved).
    pub code: String,
    /// The `reason: "..."` value; `None` when omitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// File-level (col-0, before any feature) or construct-level (1-based
    /// source line of the construct the waiver sits above).
    pub scope: DoctorAllowScope,
    /// `true` when recovered from a `# doctor:allow` comment (back-compat).
    #[serde(default)]
    pub legacy: bool,
    /// Node-form source span; `None` for legacy comment-scan recoveries.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,
}

/// Scope of a captured waiver — the only two cases the ~37 consumers need.
///
/// ## Examples
///
/// ```rust
/// use lazuli_ir::DoctorAllowScope;
///
/// let file = DoctorAllowScope::File;
/// let at = DoctorAllowScope::Construct { line: 12 };
/// assert_ne!(file, at);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DoctorAllowScope {
    /// Applies to the whole file (col-0, before any feature).
    File,
    /// 1-based source line of the construct the waiver waives.
    Construct { line: usize },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frozen_shape_constructs_and_compares() {
        let a = DoctorAllow {
            code: "X-1".to_owned(),
            reason: Some("why".to_owned()),
            scope: DoctorAllowScope::Construct { line: 3 },
            legacy: true,
            span: Some(Span { start: 0, end: 4 }),
        };
        let b = a.clone();
        assert_eq!(a, b);
        assert_eq!(a.scope, DoctorAllowScope::Construct { line: 3 });
        assert!(a.legacy);
    }

    #[test]
    fn round_trips_through_json() {
        let a = DoctorAllow {
            code: "LZI-FILE-SIZE-001".to_owned(),
            reason: None,
            scope: DoctorAllowScope::File,
            legacy: false,
            span: None,
        };
        let json = serde_json::to_string(&a).expect("serializes");
        let back: DoctorAllow = serde_json::from_str(&json).expect("deserializes");
        assert_eq!(a, back);
    }
}
