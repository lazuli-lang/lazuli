//! App-level observability IR — logging, tracing, and panic-recovery policy.
//!
//! These three siblings declare *what* the runtime should observe and at what
//! sampling rate. Concrete wiring (slog handler stack, OTel exporter,
//! middleware ordering) lives in the Lazuli Go runtime + adapter registry
//! capabilities (`@adapter.logger`, `@adapter.tracing`). The language only
//! pins intent.
//!
//! ## Catalog
//!
//! - [`AppLogging`] — `level` / `format` / `redact` / `sample_rate` closed
//!   catalogs for slog-shaped logging.
//! - [`AppTracing`] — `propagate` / `sample_rate` / `exporter` for span
//!   propagation. Exporter wiring resolves via the registry.
//! - [`AppObservability`] — `error_source` (which environments include
//!   `*lazuli.Error.Source` in HTTP 500 bodies) and `panic_recover` for
//!   the runtime's recover middleware.
//!
//! See `docs/proposals/bucket-observability-cycle.md` §3.1–§3.3.

use serde::{Deserialize, Serialize};

use crate::SpanRef;

/// Observability bucket cycle row 36 — declarative logging contract.
/// Lives directly under `app <Name>` alongside `urls`/`runtime`/`deploy`.
/// The language fixes intent (level, format, redact strategy); the
/// runtime materialises the slog handler stack. Adapter selection
/// (slog/zap/zerolog) lives in `registry.capabilities`.
///
/// All slots are optional; `None` means "adapter default". Authors
/// only need to declare the values they intend to override.
///
/// Closed catalogs:
///   - level:  debug, info, warn, error
///   - format: json, text
///   - redact: pii, none
///
/// Doctor:
///   - `app_logging_level_invalid_diagnostics`
///   - `app_logging_format_invalid_diagnostics`
///   - `app_logging_redact_unknown_diagnostics`
///   - `app_logging_sample_rate_range_diagnostics`
///
/// See `docs/proposals/bucket-observability-cycle.md` §3.1.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AppLogging {
    /// One of the level catalog tokens. `None` means adapter default
    /// (typically `info` for production, `debug` for local).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<String>,
    /// One of `json` (production-friendly, machine-parseable) or
    /// `text` (dev-friendly, human-readable).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    /// PII redaction policy. `pii` auto-strips fields tagged with any
    /// `@pii.*` namespace; `none` disables auto-redaction (adapter
    /// may still redact). `None` defers to adapter default (`pii`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub redact: Option<String>,
    /// Optional sampling rate in `[0.0, 1.0]`. `None` means "log
    /// every record". The runtime turns this into a slog `LevelVar`
    /// or sampling handler.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sample_rate: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// Observability bucket cycle row 36 — declarative tracing contract.
/// Sibling to `AppLogging`. Declares whether trace spans are
/// propagated and at what sampling rate. The exporter wiring lives
/// in `registry.capabilities` (`tracing: @adapter.tracing`); this
/// block only declares the intent.
///
/// All slots are optional; `None` means adapter default.
///
/// Doctor:
///   - `app_tracing_sample_rate_range_diagnostics`
///   - `app_tracing_exporter_unbound_diagnostics`
///
/// See `docs/proposals/bucket-observability-cycle.md` §3.2.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AppTracing {
    /// Whether the runtime propagates trace context across the
    /// request graph. `None` is treated as `true` by the runtime
    /// (matches W3C default expectations).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub propagate: Option<bool>,
    /// Head sampling rate in `[0.0, 1.0]`. `1.0` captures every
    /// span; `0.0` disables capture (still propagates context).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sample_rate: Option<f64>,
    /// Optional adapter slot name. Resolves to a
    /// `registry.capabilities <slot>: tracing` entry. `None` lets
    /// the runtime pick the default (no-op or stdout).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exporter: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// AppObservability — authoring surface for runtime panic + error
/// projection policies. Sibling of AppLogging / AppTracing.
/// Authored as `app.observability { error_source dev,staging; panic_recover true }`.
///
/// EXPERIMENTAL: structure may grow additive fields before 1.0.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppObservability {
    /// Environments where `*lazuli.Error.Source` is included in HTTP 500
    /// response bodies. Closed catalog: any subset of {"dev","staging","prod"}.
    /// Default: ["dev", "staging"] (production strips Source).
    pub error_source: Vec<String>,

    /// Whether `observability.RecoverHTTP` / `RecoverScope` swallow panics.
    /// Default: true. Setting to false outside `dev` raises a doctor warning.
    pub panic_recover: bool,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

impl Default for AppObservability {
    fn default() -> Self {
        Self {
            error_source: vec!["dev".to_string(), "staging".to_string()],
            panic_recover: true,
            span_ref: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_logging_omits_unset_slots() {
        let v = AppLogging {
            level: Some("info".into()),
            ..Default::default()
        };
        let s = serde_json::to_string(&v).expect("serialize");
        assert!(s.contains("\"level\":\"info\""));
        assert!(!s.contains("format"));
        assert!(!s.contains("sample_rate"));
    }

    #[test]
    fn app_tracing_round_trips_with_sample_rate() {
        let v = AppTracing {
            propagate: Some(true),
            sample_rate: Some(0.25),
            exporter: Some("otlp".into()),
            span_ref: None,
        };
        let s = serde_json::to_string(&v).expect("serialize");
        let back: AppTracing = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(v, back);
    }

    #[test]
    fn app_observability_default_strips_prod_source() {
        let v = AppObservability::default();
        assert_eq!(v.error_source, vec!["dev", "staging"]);
        assert!(v.panic_recover);
    }

    #[test]
    fn app_observability_round_trips_with_prod_enabled() {
        let v = AppObservability {
            error_source: vec!["dev".into(), "staging".into(), "prod".into()],
            panic_recover: false,
            span_ref: None,
        };
        let s = serde_json::to_string(&v).expect("serialize");
        let back: AppObservability = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(v, back);
    }
}
