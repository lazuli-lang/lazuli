//! App-level security IR — headers, cookies, proxy trust, request limits, CORS.
//!
//! These five blocks share one editorial intent: declare what the runtime is
//! willing to admit at the HTTP edge before any feature logic runs. The
//! language only fixes intent (closed catalogs for `same_site`,
//! `referrer_policy`, `x_frame_options`, etc.); the runtime materialises the
//! middleware stack.
//!
//! All slots are independently optional. `None` everywhere means "adapter
//! default applies" — authors only declare values they want to override.
//!
//! See Roadmap §1.2 (cookie/proxy/limits) and §1.10 (security headers +
//! secret rotation) for design notes, plus `docs/proposals/` Cut A.11 for
//! the CORS surface.
//!
//! ## Catalog
//!
//! - [`AppHeaders`] — CSP / HSTS / referrer / frame / content-type / permissions.
//! - [`AppHsts`] — `max_age` + `include_subdomains` + `preload`.
//! - [`SecretRotation`] — named cadence + overlap profile referenced from
//!   `EncryptionBinding.rotation_profile`.
//! - [`AppCookie`] / [`CookieProfile`] — named cookie hygiene profiles.
//! - [`AppProxy`] — trusted upstream proxies + the real-client headers
//!   they're allowed to set.
//! - [`AppLimits`] — request-shape ceilings (body / header / upload / timeout).
//! - [`AppCors`] / [`AppCorsOriginRule`] — per-environment CORS allowlist.

use serde::{Deserialize, Serialize};

use crate::SpanRef;

/// Roadmap §1.10 — typed `app.headers` block (CL.C.5).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppHeaders {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub csp: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hsts: Option<AppHsts>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x_frame_options: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x_content_type_options: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub referrer_policy: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permissions_policy: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// `headers.hsts { ... }` sub-block — HSTS pin shape. `max_age` is
/// seconds; `include_subdomains` / `preload` are the standard HSTS
/// extension flags.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppHsts {
    pub max_age: u64,
    #[serde(default)]
    pub include_subdomains: bool,
    #[serde(default)]
    pub preload: bool,
}

impl AppHeaders {
    /// Closed catalog of admitted values for the `Referrer-Policy`
    /// header. Mirrors the W3C catalog one-to-one.
    pub const REFERRER_POLICY_CATALOG: &'static [&'static str] = &[
        "no-referrer",
        "no-referrer-when-downgrade",
        "origin",
        "origin-when-cross-origin",
        "same-origin",
        "strict-origin",
        "strict-origin-when-cross-origin",
        "unsafe-url",
    ];
    /// Closed catalog of admitted `X-Frame-Options` values. `ALLOW-FROM
    /// <url>` is admitted dynamically via `is_x_frame_options_known`.
    pub const X_FRAME_OPTIONS_CATALOG: &'static [&'static str] = &["DENY", "SAMEORIGIN"];

    /// Returns `true` when `value` is a known `Referrer-Policy` token.
    /// Used by doctor to validate the authored string.
    pub fn is_referrer_policy_known(value: &str) -> bool {
        Self::REFERRER_POLICY_CATALOG.contains(&value)
    }
    /// Returns `true` when `value` is a known `X-Frame-Options` token.
    /// Accepts `DENY`, `SAMEORIGIN`, or `ALLOW-FROM <non-empty origin>`.
    pub fn is_x_frame_options_known(value: &str) -> bool {
        if Self::X_FRAME_OPTIONS_CATALOG.contains(&value) {
            return true;
        }
        value
            .strip_prefix("ALLOW-FROM ")
            .map(|tail| !tail.trim().is_empty())
            .unwrap_or(false)
    }
    /// Returns `true` when `value` is the only admitted
    /// `X-Content-Type-Options` token (`nosniff`).
    pub fn is_x_content_type_options_known(value: &str) -> bool {
        value == "nosniff"
    }
}

/// Roadmap §1.10 — `secret_rotation <name>` profile (CL.C.5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecretRotation {
    pub name: String,
    pub cadence: String,
    pub overlap: String,
    #[serde(default)]
    pub auto_rollback: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// Roadmap §1.2 — `app.cookie` block (CL.C.1). Named cookie hygiene
/// profiles; reserved `default` profile applies fallback.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppCookie {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub profiles: Vec<CookieProfile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// One named cookie hygiene profile under [`AppCookie`]. Each axis is
/// optional so profiles compose against the reserved `default` profile.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CookieProfile {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signed: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secure: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub http_only: Option<bool>,
    /// `lax` | `strict` | `none`. Doctor closed-catalog checked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub same_site: Option<String>,
    /// Duration literal (`"7d"`, `"30m"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_age: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// Roadmap §1.2 — `app.proxy` block (CL.C.1). Trusted upstream proxies
/// plus the headers the runtime trusts for real client signal.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppProxy {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub trusted: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub real_ip_header: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub forwarded_proto_header: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub forwarded_host_header: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// Roadmap §1.2 — `app.limits` block (CL.C.1). Request-shape ceilings.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppLimits {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body_size: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub header_size: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upload_size: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// Cut A.11 — CORS declaration. Lives in `app.lzi` alongside `urls`;
/// the runtime materialises browser-side CORS middleware from this
/// shape. Doctor cross-checks origins against `environments` and
/// declared `urls`; LSP catches shape errors at typing time.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppCors {
    /// One entry per `allow_origins <env> "<origin>"...` line.
    /// Multiple entries per environment merge.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allow_origins: Vec<AppCorsOriginRule>,
    /// `allow_credentials true | false`. Defaults to `false` (CORS
    /// spec safe default).
    #[serde(default)]
    pub allow_credentials: bool,
    /// Quoted duration string (e.g. `"1h"`, `"10 minutes"`). Adapter
    /// parses to seconds. `None` lets the adapter pick its default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_age: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// One `allow_origins <env> "<origin>"...` line inside [`AppCors`].
/// Multiple entries per environment merge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppCorsOriginRule {
    pub environment: String,
    pub origins: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn referrer_policy_catalog_recognises_strict_origin() {
        assert!(AppHeaders::is_referrer_policy_known("strict-origin"));
        assert!(!AppHeaders::is_referrer_policy_known("unknown-value"));
    }

    #[test]
    fn x_frame_options_accepts_allow_from_with_nonempty_origin() {
        assert!(AppHeaders::is_x_frame_options_known("DENY"));
        assert!(AppHeaders::is_x_frame_options_known(
            "ALLOW-FROM https://trusted.example"
        ));
        assert!(!AppHeaders::is_x_frame_options_known("ALLOW-FROM "));
    }

    #[test]
    fn app_hsts_round_trips() {
        let v = AppHsts {
            max_age: 31_536_000,
            include_subdomains: true,
            preload: false,
        };
        let s = serde_json::to_string(&v).expect("serialize");
        let back: AppHsts = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(v, back);
    }

    #[test]
    fn cookie_profile_omits_optional_fields_when_default() {
        let v = CookieProfile {
            name: "session".into(),
            ..Default::default()
        };
        let s = serde_json::to_string(&v).expect("serialize");
        assert!(s.contains("\"name\":\"session\""));
        assert!(!s.contains("same_site"));
        assert!(!s.contains("max_age"));
    }

    #[test]
    fn app_cors_default_credentials_is_false() {
        let v = AppCors::default();
        assert!(!v.allow_credentials);
    }
}
