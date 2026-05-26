//! i18n bucket cycle — `translation` block + `locale_negotiate` surface AST.
//!
//! The analyzer copies `TranslationDecl` into `ir::Translation`. The
//! `locale_negotiate` shape lives inside `api <name>` (per-endpoint
//! override); the `app.runtime unit api locale_negotiate` form is parsed
//! by `app_manifest.rs` since it lives on `app.lzi`. Both forms lower to
//! `ir::LocaleNegotiate`.

use serde::{Deserialize, Serialize};

use super::super::Span;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranslationDecl {
    /// `catalog "<path>"` — required. Carries `<locale>` placeholder.
    pub catalog: String,
    pub keys: Vec<TranslationKeyDecl>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranslationKeyDecl {
    pub name: String,
    pub variants: Vec<TranslationVariantDecl>,
    pub plurals: Vec<TranslationPluralArmDecl>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranslationVariantDecl {
    /// BCP-47 tag, e.g. `pt-BR`.
    pub locale: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranslationPluralArmDecl {
    /// CLDR plural category: `zero`, `one`, `two`, `few`, `many`, `other`.
    pub arm: String,
    pub variants: Vec<TranslationVariantDecl>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocaleNegotiateDecl {
    pub source: Option<String>,
    pub strategy: Option<String>,
    pub fallback: Option<String>,
    pub span: Span,
}
