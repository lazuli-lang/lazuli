//! Typed references — the closed catalog of types the IR understands.
//!
//! Three layers:
//!
//! - **`TypeRef`** — what a field, parameter, or return slot points
//!   to. Variants discriminate between builtins, user-defined records,
//!   declared enums, capability decorators, collection wrappers, and
//!   the safety hatch (`Unresolved`).
//! - **`BuiltinType`** — the closed catalog of primitive + semantic
//!   builtins. Adding a variant is an IR breaking change requiring
//!   doctor / codegen / proposal updates. Semantic builtins
//!   (`SemanticEmail`, `SemanticPhone`, …) carry their own analyser
//!   diagnostics and codegen tags.
//! - **`CurrencyCode`** — ISO 4217 codes the language recognises at
//!   IR time. Pilot-driven catalog: new codes land here when a pilot
//!   demands them. Unknown codes surface as analyser diagnostics and
//!   never reach IR.
//!
//! Strings are not a typed reference; the analyser decides which
//! variant a syntactic type name resolves to. Unrecognised names
//! become `TypeRef::Unresolved` so downstream consumers can surface a
//! targeted diagnostic without crashing.

use serde::{Deserialize, Serialize};

use crate::QualifiedName;
use crate::nodes::capability::CapabilityRef;

/// Closed catalog of type references. Strings are forbidden; the analyzer
/// decides which variant a syntactic type name resolves to. Unrecognised
/// names become `TypeRef::Unresolved` so downstream consumers can surface a
/// targeted diagnostic without crashing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum TypeRef {
    Builtin(BuiltinType),
    UserDefined(QualifiedName),
    EnumRef(QualifiedName),
    Many(Box<TypeRef>),
    Unresolved(String),
    /// Phase L Tier 2 — capability decorators with structured
    /// arguments (`@cap.File(max_size:...,accept:...)`). Today only
    /// `File` is typed; other `@cap.*` decorators (`Hashed`,
    /// `Encrypted`, `Token`) stay as text-pattern in LSP and project
    /// through `Unresolved`/`UserDefined` until the cycle that types
    /// them lands.
    Capability(CapabilityRef),
}

/// Closed catalog of language-level builtin and semantic types.
/// Expansion is additive: new entries land here as pilots demand them
/// and proposals approve them. Plugin-contributed semantics enter via
/// the `SemanticPluginType` variant rather than dedicated entries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BuiltinType {
    /// `ID` — primary-key identifier.
    Id,
    /// `Text` — UTF-8 string.
    Text,
    /// `Boolean` — true/false.
    Boolean,
    /// `Integer` — 64-bit signed integer.
    Integer,
    /// `Decimal` — arbitrary-precision decimal.
    Decimal,
    /// `Date` — calendar date (no time).
    Date,
    /// `DateTime` — instant with timezone.
    DateTime,
    /// `Json` — opaque JSON blob.
    Json,
    /// `@semantic.Email` — RFC-shaped email address.
    SemanticEmail,
    /// Per `docs/proposals/semantic-types-money-brazilian.md` v0.3 +
    /// MONEY-1 §3.2 of the the canonical pilot roadmap. Carries the declared ISO
    /// 4217 currency so downstream doctor checks (MONEY-COMPARE-001,
    /// MONEY-ARITHMETIC-001) can reject mixed-currency operations at
    /// analyse time without re-walking surface text. The default
    /// authoring shorthand `Money` lowers to `SemanticMoney { currency:
    /// BRL }` (canonical-pilot reality); explicit
    /// `@semantic.Money(currency: <ISO>)` overrides.
    SemanticMoney {
        currency: CurrencyCode,
    },
    /// Phase L Tier 4 follow-up — `@semantic.Phone`. Closed catalog
    /// addition so auth-identity diagnostics can read the shape
    /// without text-walking.
    SemanticPhone,
    /// Phase L Tier 4 follow-up — `@semantic.Url`.
    SemanticUrl,
    /// Phase L Tier 4 follow-up — `@semantic.Uuid`.
    SemanticUuid,
    /// Currency follow-up — `@semantic.Currency`. ISO 4217 3-letter
    /// uppercase code (`USD`, `BRL`). Pairs with `SemanticMoney` for
    /// typed amount-currency tuples; emitter maps to Go `lazuli.Currency`
    /// alias (already exists in `runtime/go/lazuli/types.go`).
    SemanticCurrency,
    /// GeoPoint follow-up (2026-05-11) — `@semantic.GeoPoint`.
    /// Closed-catalog single semantic carrying `{ lat, lng }`. Required
    /// by `codegen-lazuli-go.md` §6.3/§9.1 to materialise as
    /// `postgis.Point` in generated Go + drive the `GIST` index
    /// emission in DDL migrations.
    SemanticGeoPoint,
    /// B3 — plugin-contributed `@semantic.<Name>` resolved through a
    /// plugin's `manifest.toml`. The IR layer is locale-agnostic: it
    /// knows only the declaring plugin namespace (`@lazuli/plugin-scalars-br`),
    /// the manifest-local alias terminal name (`BrazilianCPF`), the
    /// carrier built-in (currently always `Text`), and the validator
    /// function name from the manifest. Codegen reads the validator
    /// to build the `<plugin-short>.<validator>` go-playground tag
    /// without re-reading the manifest at emission time. The plugin
    /// owns checksum rules, formatting, and any upstream library. See
    /// `docs/proposals/semantic-types-plugin-locales.md`.
    SemanticPluginType {
        plugin: String,
        name: String,
        carrier: Box<BuiltinType>,
        /// Exported Go function on the plugin adapter (e.g.
        /// `ValidateCPF`). Carried so codegen can emit the validate
        /// tag without re-reading `manifest.toml`. Authoritative
        /// source is the plugin's manifest `[[semantic_types]].validator`
        /// — the resolver pass copies it here at lift time.
        validator: String,
        /// W2 (ir-semantic-auto-validate-2026-05-22): effective Go
        /// module path of the plugin (`lazuli.dev/plugin/scalars-br`).
        /// Plugin-level value or convention fallback. Empty when the
        /// IR predates W2 lift.
        #[serde(default)]
        go_module: String,
        /// W2: effective TS/npm package (`@lazuli/plugin-scalars-br`).
        #[serde(default)]
        ts_package: String,
        /// W2: effective error code surfaced on validation_failed
        /// (`cpf_invalid`).
        #[serde(default)]
        error_code: String,
        /// W2: optional i18n message key. Empty when not declared.
        #[serde(default)]
        message_key: String,
        /// W2: TS validator function (`validateCPF`). Empty when not
        /// declared — TS preflight emission is skipped.
        #[serde(default)]
        ts_validator: String,
    },
    CapSecret,
    /// Deprecated: the flat `CapFile` variant never carried arguments.
    /// Phase L Tier 2 introduces `TypeRef::Capability(CapabilityRef::File(...))`
    /// which carries the parsed `max_size`/`accept`/`visibility`/`signed_ttl`
    /// slots. Kept for back-compat with serialized payloads predating the
    /// typed shape.
    CapFile,
}

/// MONEY-1 §3.2 — closed-catalog ISO 4217 codes the language understands
/// at IR time. Expansion is additive: new currencies land here when a
/// pilot demands them. Other ISO codes the user might type fall through
/// to the analyzer's "unknown currency" diagnostic and never reach IR.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CurrencyCode {
    BRL,
    USD,
    EUR,
    GBP,
    JPY,
    CHF,
}

impl CurrencyCode {
    /// Canonical 3-letter ISO 4217 form (`"BRL"`, `"USD"`...). Used by
    /// codegen to emit the `CHECK (<col> = '<ISO>')` constraint and by
    /// doctor diagnostics when interpolating into messages.
    ///
    /// ## Examples
    ///
    /// ```
    /// use lazuli_ir::CurrencyCode;
    ///
    /// assert_eq!(CurrencyCode::BRL.as_iso(), "BRL");
    /// ```
    pub fn as_iso(&self) -> &'static str {
        match self {
            Self::BRL => "BRL",
            Self::USD => "USD",
            Self::EUR => "EUR",
            Self::GBP => "GBP",
            Self::JPY => "JPY",
            Self::CHF => "CHF",
        }
    }

    /// Parse a 3-letter ISO 4217 code into the closed catalog. Returns
    /// `None` for unknown codes; the analyzer surfaces that as a typed
    /// diagnostic rather than silently accepting it.
    ///
    /// ## Examples
    ///
    /// ```
    /// use lazuli_ir::CurrencyCode;
    ///
    /// assert_eq!(CurrencyCode::from_iso("BRL"), Some(CurrencyCode::BRL));
    /// assert_eq!(CurrencyCode::from_iso("XYZ"), None);
    /// ```
    pub fn from_iso(raw: &str) -> Option<Self> {
        match raw {
            "BRL" => Some(Self::BRL),
            "USD" => Some(Self::USD),
            "EUR" => Some(Self::EUR),
            "GBP" => Some(Self::GBP),
            "JPY" => Some(Self::JPY),
            "CHF" => Some(Self::CHF),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn currency_code_round_trips_iso() {
        assert_eq!(CurrencyCode::from_iso("USD"), Some(CurrencyCode::USD));
        assert_eq!(CurrencyCode::USD.as_iso(), "USD");
    }
}
