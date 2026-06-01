//! Feature-level `defaults` block surface AST (Phase L Tier 4a).
//!
//! The `defaults` block declares feature-level inheritance for tenancy,
//! timestamps, and policy. Resource-local declarations override these.
//! The IR already carries `ir::Defaults`; this AST mirrors that shape so
//! lowering is structural.

use serde::{Deserialize, Serialize};

use super::super::Span;

/// Feature-level `defaults` block (Phase L Tier 4a).
///
/// Sets the inherited tenancy axis, timestamp convention, and per-kind
/// policy fallbacks for every construct in the feature. Resource-local
/// declarations win — this block only fills the gaps. Mirrors the IR
/// shape so lowering is structural.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatureDefaults {
    /// `tenancy org`, `tenancy team`, `tenancy none`, or a custom axis.
    pub tenancy: Option<DefaultsTenancy>,
    /// `timestamps` declared verbatim. Absent when not authored.
    pub timestamps: bool,
    /// `policy_for jobs, webhooks: @actor.system` style entries. Each
    /// entry binds a list of construct kinds (`jobs`, `webhooks`,
    /// `commands`, ...) to a single policy atom.
    pub policy_for: Vec<DefaultsPolicyFor>,
    /// 0004 — `defaults rate_limit "<spec>"` hoisted to every command in
    /// the feature. Each command inherits this string spec unless it
    /// declares its own `rate_limit`. Stays a string (the string→struct
    /// axis is a separate, deferred change). Absent when not authored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rate_limit: Option<String>,
    /// 0004 — `defaults audit default` hoisted to every command in the
    /// feature. Each command inherits this audit mode unless it declares
    /// its own `audit ...` / `audit none`. Today the only hoistable mode
    /// is `default` (subjects: `["default"]`); the [`DefaultsAudit`]
    /// catalog stays closed so a future `audit <subjects>` hoist can be
    /// added without a wire break. Absent when not authored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audit: Option<DefaultsAudit>,
    pub span: Span,
}

/// 0004 — closed catalog for the hoistable `defaults audit <mode>` value.
///
/// Only `default` is hoistable today (`defaults audit default`), mirroring
/// the per-command `audit default` form. The enum keeps the surface
/// closed so an unknown mode (`defaults audit everything`) is a parse
/// error rather than a silent no-op, and leaves room for a future
/// `audit <subjects>` hoist arm without breaking the wire shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum DefaultsAudit {
    /// `defaults audit default` — every command audits its default fields
    /// unless it opts out with `audit none` or overrides with explicit
    /// `audit <subjects>`.
    Default,
}

/// Tenancy axis catalog declared via `tenancy <axis>` on
/// [`FeatureDefaults`]. Closed catalog with a single open `Custom` arm
/// for user-defined axes (e.g. `workspace`, `project`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum DefaultsTenancy {
    /// `tenancy org`.
    Org,
    /// `tenancy team`.
    Team,
    /// `tenancy none` — explicit opt-out.
    None,
    /// `tenancy workspace` and similar custom identifiers.
    Custom(String),
}

/// One `policy_for <kinds>: <atom>` row inside a [`FeatureDefaults`] block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DefaultsPolicyFor {
    /// Construct kinds the policy applies to (`jobs`, `webhooks`,
    /// `commands`, `apis`, etc.). Comma-separated in source.
    pub kinds: Vec<String>,
    /// The policy atom literal, e.g. `@actor.system`. Captured verbatim
    /// so the analyzer can decide between `PolicyRef::Atom` and other
    /// variants without re-parsing surface text.
    pub atom: String,
    pub span: Span,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_tenancy_custom_serde_roundtrip() {
        let t = DefaultsTenancy::Custom("workspace".into());
        let s = serde_json::to_string(&t).unwrap();
        let back: DefaultsTenancy = serde_json::from_str(&s).unwrap();
        assert_eq!(back, t);
    }

    #[test]
    fn feature_defaults_minimal_construct() {
        let d = FeatureDefaults {
            tenancy: Some(DefaultsTenancy::Org),
            timestamps: true,
            policy_for: vec![],
            rate_limit: None,
            audit: None,
            span: Span::new(0, 0),
        };
        assert!(matches!(d.tenancy, Some(DefaultsTenancy::Org)));
        assert!(d.timestamps);
    }
}
