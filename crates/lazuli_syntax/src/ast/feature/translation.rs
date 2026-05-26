//! i18n bucket cycle — `translation` block + `locale_negotiate` surface AST.
//!
//! The analyzer copies `TranslationDecl` into `ir::Translation`. The
//! `locale_negotiate` shape lives inside `api <name>` (per-endpoint
//! override); the `app.runtime unit api locale_negotiate` form is parsed
//! by `app_manifest.rs` since it lives on `app.lzi`. Both forms lower to
//! `ir::LocaleNegotiate`.

use serde::{Deserialize, Serialize};

use super::super::Span;

/// Feature-scope `translation` block — i18n catalog binding + key map.
///
/// The catalog path tells the runtime where to load locale bundles; the
/// `keys` list carries the per-key text + plural variants authored
/// inline. Empty `keys` is legal (sidecar JSON catalog drives runtime).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranslationDecl {
    /// `catalog "<path>"` — required. Carries `<locale>` placeholder.
    pub catalog: String,
    pub keys: Vec<TranslationKeyDecl>,
    pub span: Span,
}

/// One `<key>` row inside a [`TranslationDecl`].
///
/// A key may carry locale variants directly ([`TranslationVariantDecl`])
/// and/or CLDR plural arms ([`TranslationPluralArmDecl`]) — both lists
/// are empty when the key is sourced exclusively from the sidecar
/// catalog JSON.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranslationKeyDecl {
    pub name: String,
    pub variants: Vec<TranslationVariantDecl>,
    pub plurals: Vec<TranslationPluralArmDecl>,
    pub span: Span,
}

/// One `<locale>: "<text>"` row inside a [`TranslationKeyDecl`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranslationVariantDecl {
    /// BCP-47 tag, e.g. `pt-BR`.
    pub locale: String,
    /// Translation text verbatim (quotes stripped).
    pub text: String,
}

/// One CLDR plural arm inside a [`TranslationKeyDecl`].
///
/// Arms hold a closed-catalog name (`zero`/`one`/.../`other`) plus the
/// locale variants that fire for that arm.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranslationPluralArmDecl {
    /// CLDR plural category: `zero`, `one`, `two`, `few`, `many`, `other`.
    pub arm: String,
    pub variants: Vec<TranslationVariantDecl>,
}

/// `locale_negotiate` sub-block — picks how the runtime selects a locale
/// for a request. Authored either feature-scoped (per-api override) or
/// on `app.runtime unit api locale_negotiate`. Both forms lower to
/// `ir::LocaleNegotiate`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocaleNegotiateDecl {
    /// `source <accept_language|header|query|cookie|...>` — where to read.
    pub source: Option<String>,
    /// `strategy <best_match|prefix|exact>` — match algorithm.
    pub strategy: Option<String>,
    /// `fallback <locale>` — default when negotiation fails.
    pub fallback: Option<String>,
    pub span: Span,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translation_variant_serde_roundtrip() {
        let v = TranslationVariantDecl {
            locale: "pt-BR".into(),
            text: "Olá".into(),
        };
        let s = serde_json::to_string(&v).unwrap();
        let back: TranslationVariantDecl = serde_json::from_str(&s).unwrap();
        assert_eq!(back, v);
    }

    #[test]
    fn locale_negotiate_all_optional() {
        let n = LocaleNegotiateDecl {
            source: None,
            strategy: None,
            fallback: None,
            span: Span::new(0, 0),
        };
        assert!(n.source.is_none() && n.strategy.is_none() && n.fallback.is_none());
    }
}
