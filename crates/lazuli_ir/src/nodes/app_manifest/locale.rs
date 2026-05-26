//! App-level locale IR — supported tags, fallback graph, request-time
//! negotiation, and per-feature translation catalogs.
//!
//! Lazuli treats the framework itself as locale-agnostic: the language only
//! declares which BCP-47 tags an app is willing to admit, how the runtime
//! walks the fallback graph when a translation is missing, and how it picks
//! the active locale from an incoming request. Concrete catalog files
//! (`./i18n/<feature>.<locale>.json`) live in the app, not the language.
//!
//! ## Catalog
//!
//! - [`AppLocale`] — `default` + `supported` + `fallbacks`. Lifted onto
//!   `AppManifest.locale`; supersedes the bare scalar `default_locale`
//!   when both are present.
//! - [`LocaleFallback`] — directed `from -> to` edge in the fallback graph.
//! - [`LocaleNegotiate`] — request-axis + matching-strategy decorator.
//!   Lives on `AppRuntimeUnit` (global default) and `Api`
//!   (per-endpoint override).
//! - [`Translation`] / [`TranslationKey`] / [`TranslationVariant`] /
//!   [`TranslationPluralArm`] — per-feature translation surface.
//!
//! See the i18n bucket cycle proposal for the design and the LZ-locale
//! discipline memo for the framework-vs-app split.

use serde::{Deserialize, Serialize};

/// i18n bucket cycle — `app.locale` block. Declares supported BCP-47
/// tags and the fallback graph the runtime walks when a translation
/// is missing.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppLocale {
    /// BCP-47 tag, e.g. "pt-BR".
    pub default: String,
    /// BCP-47 tags the app is willing to negotiate against. Must
    /// include `default`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub supported: Vec<String>,
    /// Fallback edges: when `from` is requested but no translation
    /// resolves, the runtime walks the chain to `to` before defaulting
    /// to `default`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fallbacks: Vec<LocaleFallback>,
}

/// One `<from> -> <to>` fallback entry inside [`AppLocale`]. Resolves
/// per-locale routing when the requested tag is unavailable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocaleFallback {
    pub from: String,
    pub to: String,
}

/// i18n bucket cycle — `locale_negotiate` decorator. Sits on
/// `AppRuntimeUnit` (global default) and `Api` (per-endpoint override).
/// Declares the request axis the runtime reads to populate
/// `ctx.locale` and the matching strategy. All slots optional; the
/// runtime defaults to `accept_language` + `best_match` when omitted.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocaleNegotiate {
    /// `source <axis>` — closed catalog: `accept_language`,
    /// `query_param`, `cookie`, `user_profile`, `subdomain`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// `strategy <name>` — closed catalog: `best_match`, `prefix_match`,
    /// `exact_match`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strategy: Option<String>,
    /// `fallback <tag>` — BCP-47 tag in `app.locale.supported`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback: Option<String>,
}

/// i18n bucket cycle — `translation` block lifted onto `Feature`.
/// Declares a per-locale catalog path and typed translation keys with
/// optional CLDR plural arms.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Translation {
    /// Catalog path with `<locale>` placeholder, e.g.
    /// `./i18n/customer.<locale>.json`.
    pub catalog: String,
    pub keys: Vec<TranslationKey>,
}

/// One typed translation key inside a [`Translation`]. Carries the
/// per-locale text variants and optional CLDR plural arms.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranslationKey {
    pub name: String,
    /// One entry per BCP-47 tag; must cover `app.locale.supported`.
    pub variants: Vec<TranslationVariant>,
    /// CLDR plural arms (`zero`/`one`/`two`/`few`/`many`/`other`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub plurals: Vec<TranslationPluralArm>,
}

/// One `<locale>: "<text>"` entry inside a [`TranslationKey`]. The
/// locale is a BCP-47 tag; the text is the canonical translation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranslationVariant {
    pub locale: String,
    pub text: String,
}

/// One CLDR plural arm under a [`TranslationKey`]. Carries the arm
/// name and its per-locale text variants.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranslationPluralArm {
    /// `zero` | `one` | `two` | `few` | `many` | `other` per CLDR.
    pub arm: String,
    pub variants: Vec<TranslationVariant>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_locale_round_trips_with_fallbacks() {
        let v = AppLocale {
            default: "pt-BR".into(),
            supported: vec!["pt-BR".into(), "en-US".into()],
            fallbacks: vec![LocaleFallback {
                from: "pt-PT".into(),
                to: "pt-BR".into(),
            }],
        };
        let s = serde_json::to_string(&v).expect("serialize");
        let back: AppLocale = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(v, back);
    }

    #[test]
    fn locale_negotiate_omits_unset_slots() {
        let v = LocaleNegotiate {
            source: Some("accept_language".into()),
            ..Default::default()
        };
        let s = serde_json::to_string(&v).expect("serialize");
        assert!(s.contains("source"));
        assert!(!s.contains("strategy"));
    }

    #[test]
    fn translation_carries_keys_and_plural_arms() {
        let v = Translation {
            catalog: "./i18n/customer.<locale>.json".into(),
            keys: vec![TranslationKey {
                name: "cart.items".into(),
                variants: vec![TranslationVariant {
                    locale: "pt-BR".into(),
                    text: "{count} itens".into(),
                }],
                plurals: vec![TranslationPluralArm {
                    arm: "one".into(),
                    variants: vec![TranslationVariant {
                        locale: "pt-BR".into(),
                        text: "1 item".into(),
                    }],
                }],
            }],
        };
        let s = serde_json::to_string(&v).expect("serialize");
        let back: Translation = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(v, back);
    }
}
