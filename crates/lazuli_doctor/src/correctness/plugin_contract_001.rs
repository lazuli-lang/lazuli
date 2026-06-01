//! PLUGIN-CONTRACT-001 — declared adapter contract is broken.
//!
//! Doctor correctness rule (spec 0022). Fires when an adapter plugin
//! declared in `Lazurite.toml [plugins]` declares a Go bucket interface
//! (`implements = ["payments.PaymentGateway"]` or `[binds].interface`)
//! that the framework cannot wire:
//!
//! 1. **Unknown interface** — the declared interface is not a member of
//!    the closed known-bucket-interface catalog (a typo like
//!    `payments.PaymentGatway`, or a legacy/self-referential spelling that
//!    no runtime bucket exports).
//! 2. **Unbound capability** — the declared (known) interface's capability
//!    is bound by the app's registry to a DIFFERENT plugin ref.
//!
//! ## Severity
//!
//! Severity: `error` in every profile. A misdeclared adapter compiles
//! clean today (adapter binding is a runtime string lookup) and only
//! surfaces as `ErrAdapterMissing` at the first live request — exactly the
//! "declared but silently inert" class this check kills at build time.
//!
//! ## Fires when (trigger cue)
//!
//! Fires when an adapter manifest's `implements` / `[binds].interface`
//! names an interface absent from `KNOWN_BUCKET_INTERFACES`, or when the
//! app binds that capability to another plugin. Example: a plugin whose
//! `manifest.toml` carries `implements = ["payments.PaymentGatway"]`
//! (typo) → one `PLUGIN-CONTRACT-001` naming the nearest known interface.
//!
//! ## Static limit
//!
//! This proves the DECLARED contract + the wiring graph only. Whether the
//! Go `Adapter` type actually satisfies the interface's method set is the
//! plugin's runtime `var _ <Interface> = (*Adapter)(nil)` assertion in
//! `adapter.go` under `go build` — the message tail restates this so a
//! PASS is never mistaken for a method-set guarantee.
//!
//! The shared classifier lives in
//! `lazuli_manifest::plugin_contract::classify_adapter_contract`; the
//! `lazuli_doctor_run` aggregator maps its `ContractStatus` into the
//! [`Finding`] below (this crate cannot depend on `lazuli_manifest`). The
//! `verify_and_doctor_agree_drift_guard` test pins that the same fixture
//! is flagged here and by `lazuli plugin verify`'s L3 link.

use std::path::PathBuf;

/// The honest static-limit note appended to every PLUGIN-CONTRACT-001
/// message. Kept in sync with
/// `lazuli_manifest::plugin_contract::HONEST_LIMIT_NOTE` (mirrored here so
/// this crate stays free of the `lazuli_manifest` dependency).
pub const HONEST_LIMIT_NOTE: &str = "(method-set conformance is verified at runtime by 'var _ <Interface> = (*Adapter)(nil)' in the plugin's adapter.go under go build)";

/// One PLUGIN-CONTRACT-001 finding — an adapter's declared contract is
/// broken in one of two ways.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Plugin ref as declared in `Lazurite.toml [plugins]`
    /// (e.g. `@lazuli/plugin-mercadopago`).
    pub plugin_ref: String,
    /// Path the diagnostic anchors at — the plugin's `manifest.toml`
    /// (unknown-interface case) or `Lazurite.toml` (binding case).
    pub anchor: PathBuf,
    /// Why the contract failed.
    pub reason: Reason,
}

/// Sub-classification of the contract violation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reason {
    /// The declared interface is not a known bucket interface.
    /// `declared` is the normalised offending interface; `nearest` is the
    /// closest known interface for a "did you mean" hint.
    UnknownInterface {
        declared: String,
        nearest: String,
    },
    /// A known interface bound by the app to a DIFFERENT plugin ref.
    UnboundCapability {
        capability: String,
        bound_to: String,
    },
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "PLUGIN-CONTRACT-001";

    /// Render the per-reason message — names the plugin, the offending
    /// interface/capability, the nearest known interface on a near-miss,
    /// the fix, and the honest static-limit tail.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::correctness::plugin_contract_001::{Finding, Reason};
    ///
    /// let f = Finding {
    ///     plugin_ref: "@lazuli/plugin-x".into(),
    ///     anchor: PathBuf::from("manifest.toml"),
    ///     reason: Reason::UnknownInterface {
    ///         declared: "payments.PaymentGatway".into(),
    ///         nearest: "payments.PaymentGateway".into(),
    ///     },
    /// };
    /// assert!(f.message().contains("did you mean"));
    /// ```
    pub fn message(&self) -> String {
        match &self.reason {
            Reason::UnknownInterface { declared, nearest } => format!(
                "plugin `{}` declares `implements`/`[binds]` interface `{}`, which is not a known framework bucket interface (did you mean `{}`?). \
                 Declare an interface from a shipped runtime bucket (`payments.PaymentGateway`, `storage.ObjectStore`, `maps.Geocoder`, …) or remove the contract declaration. \
                 {}",
                self.plugin_ref, declared, nearest, HONEST_LIMIT_NOTE
            ),
            Reason::UnboundCapability {
                capability,
                bound_to,
            } => format!(
                "plugin `{}` declares capability `{}`, but the app's registry binds that capability to `{}`. \
                 Bind `{}` to `{}` in the registry, or remove the declaration. \
                 {}",
                self.plugin_ref, capability, bound_to, capability, self.plugin_ref, HONEST_LIMIT_NOTE
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_is_stable() {
        assert_eq!(Finding::CODE, "PLUGIN-CONTRACT-001");
    }

    #[test]
    fn unknown_interface_message_names_plugin_nearest_and_limit() {
        let f = Finding {
            plugin_ref: "@lazuli/plugin-x".into(),
            anchor: PathBuf::from("manifest.toml"),
            reason: Reason::UnknownInterface {
                declared: "payments.PaymentGatway".into(),
                nearest: "payments.PaymentGateway".into(),
            },
        };
        let m = f.message();
        assert!(m.contains("@lazuli/plugin-x"));
        assert!(m.contains("payments.PaymentGatway"));
        assert!(m.contains("did you mean `payments.PaymentGateway`"));
        assert!(m.contains("var _ <Interface> = (*Adapter)(nil)"));
    }

    #[test]
    fn unbound_capability_message_names_both_refs() {
        let f = Finding {
            plugin_ref: "@lazuli/plugin-x".into(),
            anchor: PathBuf::from("Lazurite.toml"),
            reason: Reason::UnboundCapability {
                capability: "payments.PaymentGateway".into(),
                bound_to: "@lazuli/plugin-other".into(),
            },
        };
        let m = f.message();
        assert!(m.contains("@lazuli/plugin-x"));
        assert!(m.contains("@lazuli/plugin-other"));
        assert!(m.contains("payments.PaymentGateway"));
    }
}
