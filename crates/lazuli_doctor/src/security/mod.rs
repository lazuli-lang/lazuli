//! Doctor security rules — HTTP-edge configuration that the runtime would
//! refuse (or that the CORS / cookie / CSRF spec forbids).
//!
//! Distinct from `correctness/` (concrete wiring bugs): security rules
//! surface configuration that ships a footgun — a credential leak, a
//! disabled isolation boundary, a wildcard the runtime refuses to boot
//! with. They map to `RuleCategory::Security` (the `CORS-`/`SECURITY-`/
//! `AUTH-`/`SESSION-` prefix family in `rule_category::from_code_prefix`).
//!
//! ## Catalog
//!
//! - [`cors_wildcard_prod_001`] — `CORS-WILDCARD-PROD-001`: a wildcard
//!   (`"*"`) CORS allow-origin in a production-targeted environment. The
//!   compile-time companion to the runtime `Mux()` boot refusal
//!   (`ErrCSRFWildcardProd`).

pub mod cors_wildcard_prod_001;
