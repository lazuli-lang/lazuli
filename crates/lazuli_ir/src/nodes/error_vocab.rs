//! Error-vocab IR — typed translation-key references + feature-level error
//! contracts.
//!
//! Errors that reach a user must come from one place: the feature's
//! `errors` block. This module is the IR-side of that contract — the
//! single typed surface that codegen, doctor, LSP, and the runtime all
//! agree on.
//!
//! Two coexisting shapes:
//!
//! - **Legacy `Rule.message_ref: Option<String>`** — the original v1
//!   string form. UNTOUCHED for back-compat: existing fixtures
//!   deserialize unchanged. v2 will migrate rules onto the typed shape;
//!   the slot exists today so the migration is additive.
//! - **Typed [`TranslationKeyRef`]** — used by feature-level errors,
//!   per-command / per-policy `when_denied` overrides, and (post-pilot)
//!   per-field validator errors.
//!
//! ## Resolution chain
//!
//! When a runtime error fires, the message is resolved in this order
//! (proposal §2.E):
//!
//! 1. Per-command `policy_when_denied` (if present on the call site).
//! 2. Per-policy `when_denied` (on the resolved policy).
//! 3. Feature-level [`FeatureErrors::messages`] matching the error code.
//! 4. Runtime default.
//!
//! Step 3 is the slot this module owns.
//!
//! ## Closed catalogs
//!
//! - [`ErrorExposureDefault`] — `hide` (envelope fields suppressed
//!   unless an `expose client 4xx/5xx ...` line opts them in) vs
//!   `expose` (default flip).
//! - 4xx envelope fields — `message`, `code`, `data`, `message_key`.
//! - 5xx envelope fields — `code`, `data` (**not** `message`; see
//!   proposal §2.C — preventing accidental leak of stack/inner-exception
//!   text to clients).
//! - Override-eligible codes — `policy_denied`, `validation_failed`,
//!   `tenant_mismatch`, `not_found`, `rate_limited`, `bad_request`,
//!   `method_not_allowed`, `integration_error`. Doctor diagnostic
//!   `ERR-VOCAB-CODE-UNKNOWN` rejects entries outside this set.
//!
//! ## Additive guarantees
//!
//! Every slot is `Option`/`Vec` with
//! `#[serde(default, skip_serializing_if = "...")]`. Pre-vocab fixtures
//! deserialize unchanged; consumers that don't read the new slots see
//! the legacy shape unchanged.
//!
//! ## See also
//!
//! - `docs/proposals/ir-error-messages-vocab.md` §3 — full design
//! - [`crate::Rule`] — legacy `message_ref` string slot lives there
//! - [`FeatureFieldError`] — reserved-for-v2 per-field validator
//!   message override

use serde::{Deserialize, Serialize};

use crate::SpanRef;

/// A `@translation.<key>` reference used by feature-level errors blocks,
/// per-command/per-policy `when_denied` overrides, and (post-pilot) per-
/// field validator errors. The key resolves against the surrounding
/// feature's `Translation.keys[]` at analyze time; doctor cross-checks
/// the key against the resolved feature's catalog
/// (`translation_key_unknown` + ERR-VOCAB-002).
///
/// Complements the legacy `Rule.message_ref: Option<String>` — the
/// string form stays for back-compat in v1; v2 migrates rules onto this
/// struct.
///
/// See `docs/proposals/ir-error-messages-vocab.md` §3.1.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranslationKeyRef {
    /// The key name, e.g. `must_be_signed_in`.
    pub key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// Closed catalog of feature-level `errors default ...` resolutions.
/// `Hide` means error envelope fields default to suppressed unless an
/// `expose client 4xx/5xx ...` line opts them in; `Expose` flips the
/// default. Mirrors the pre-existing LSP validator at
/// `valid_error_exposure_line`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorExposureDefault {
    Hide,
    Expose,
}

/// Feature-level error contract lowered from the `errors` block.
/// Subsumes both the pre-existing LSP-validated `default hide / expose
/// client 4xx ...` exposure surface (now lowered into IR) and the new
/// typed per-code message overrides introduced by the error-vocab
/// proposal.
///
/// Resolution-chain step 3 (see proposal §2.E): when neither the
/// per-command `policy_when_denied` nor the per-policy `when_denied`
/// resolves, the runtime falls through to `messages[].message` for the
/// matching closed-catalog `code`.
///
/// See `docs/proposals/ir-error-messages-vocab.md` §3.4.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatureErrors {
    /// `default hide` | `default expose`. `None` defers to the runtime
    /// default (currently `Hide`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<ErrorExposureDefault>,
    /// 4xx envelope-field exposure. Closed catalog: `message`, `code`,
    /// `data`, `message_key`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exposure_4xx: Vec<String>,
    /// 5xx envelope-field exposure. Closed catalog: `code`, `data`.
    /// (`message` deliberately not allowed for 5xx — see proposal §2.C.)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exposure_5xx: Vec<String>,
    /// Audience-scoped exposure rules from `expose to @audience <name>
    /// <fields>`. Runtime/codegen enforcement lands in a follow-up.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub audience_exposure: Vec<ErrorExposeRule>,
    /// `error_redact <pattern>` lines — regex patterns to mask from
    /// emitted error bodies before client-side delivery.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub redact_patterns: Vec<String>,
    /// Per-code message overrides; one entry per `<code> message
    /// @translation.<key>` line. Resolution-chain step 3.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub messages: Vec<FeatureErrorMessage>,
    /// Reserved for v2 — per-field validator error references
    /// (`validates field <Field>.<code> message @translation.<key>`).
    /// v1 parser leaves this empty; codegen ignores. The slot lives in
    /// IR so v2 promotion is purely additive.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub field_messages: Vec<FeatureFieldError>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorExposeRule {
    /// Optional audience target. `None` is the legacy client-wide
    /// exposure shape; `Some("operator")` comes from
    /// `expose to @audience operator ...`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audience: Option<String>,
    /// Envelope fields exposed by this rule.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// One `<code> message @translation.<key>` row inside a feature's
/// `errors` block. `code` is constrained to the closed catalog of
/// overridable error families (`policy_denied`, `validation_failed`,
/// `tenant_mismatch`, `not_found`, `rate_limited`, `bad_request`,
/// `method_not_allowed`, `integration_error`) — doctor diagnostic
/// `ERR-VOCAB-CODE-UNKNOWN` rejects unknown codes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatureErrorMessage {
    pub code: String,
    pub message: TranslationKeyRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// Reserved-for-v2 per-field validator error reference. v1 parser leaves
/// this slot empty; the IR shape exists so v2 lowering and codegen
/// promotion can land additively without an IR-ABI churn.
///
/// `resource` + `field` identify the target (e.g. `Customer.email`);
/// `code` is the per-field validator error code (`format_invalid`,
/// `required_missing`, ...); `message` is the typed key reference into
/// the surrounding feature's `Translation.keys[]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatureFieldError {
    pub resource: String,
    pub field: String,
    pub code: String,
    pub message: TranslationKeyRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exposure_default_serializes_snake_case() {
        assert_eq!(
            serde_json::to_string(&ErrorExposureDefault::Hide).unwrap(),
            "\"hide\""
        );
        assert_eq!(
            serde_json::to_string(&ErrorExposureDefault::Expose).unwrap(),
            "\"expose\""
        );
    }

    #[test]
    fn feature_errors_default_skips_empty_slots_in_json() {
        let errs = FeatureErrors::default();
        let json = serde_json::to_string(&errs).unwrap();
        // Default ⇒ every slot is empty/None ⇒ JSON is the empty object.
        assert_eq!(json, "{}");
    }

    #[test]
    fn translation_key_ref_round_trip() {
        let tk = TranslationKeyRef {
            key: "must_be_signed_in".into(),
            span_ref: None,
        };
        let json = serde_json::to_string(&tk).unwrap();
        let back: TranslationKeyRef = serde_json::from_str(&json).unwrap();
        assert_eq!(tk, back);
        assert!(json.contains("\"key\":\"must_be_signed_in\""));
    }

    #[test]
    fn feature_error_message_carries_typed_key() {
        let msg = FeatureErrorMessage {
            code: "policy_denied".into(),
            message: TranslationKeyRef {
                key: "you_are_not_allowed".into(),
                span_ref: None,
            },
            span_ref: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"code\":\"policy_denied\""));
        assert!(json.contains("\"key\":\"you_are_not_allowed\""));
    }
}
