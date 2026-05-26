//! Platform/audience surface IR — lowered shape of `*.web.lzx` and
//! `*.mobile.lzx` files.
//!
//! A `PlatformSurface` projects one experience onto one target platform
//! (web or mobile). Each surface carries one or more `AudienceSurface`s —
//! the scope-gated sections that decide *who* sees *which* view variant.
//! Per-view shape (list / detail / create columns, sections, search, etc.)
//! lives on [`PlatformView`].
//!
//! ## Catalog
//!
//! - [`PlatformSurface`] — root projection.
//! - [`Platform`] — closed catalog `Web` / `Mobile`.
//! - [`AudienceSurface`] — `audience` block with qualifiers + per-view
//!   shapes + audience-level guard defaults.
//! - [`PlatformView`] — concrete platform-specific view shape (columns,
//!   fields, sections, cells, actions, blocks).

use serde::{Deserialize, Serialize};

use crate::PolicyAtom;
use crate::SpanRef;
use crate::nodes::experience::guard::ViewGuard;

/// Root projection of one [`crate::Experience`] onto one platform
/// target. Carries the platform discriminator, optional `uses
/// <experience>` reference, and the per-audience surface entries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlatformSurface {
    pub experience: String,
    pub platform: Platform,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uses_experience: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub audiences: Vec<AudienceSurface>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// Closed catalog of `.lzx` platform targets. `Web` and `Mobile`
/// each get their own codegen pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Platform {
    /// React Web target.
    Web,
    /// React Native mobile target.
    Mobile,
}

/// One audience entry inside a [`PlatformSurface`]. Carries the
/// scope qualifiers, the views the audience sees, and an optional
/// audience-level [`ViewGuard`] default that nested views can inherit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudienceSurface {
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub qualifiers: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub views: Vec<PlatformView>,
    /// `ir-route-guards` §3.4 — per-audience default guard. Layer 2 of
    /// the resolution chain (view → audience → app → built-in).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guard: Option<ViewGuard>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// Platform-specific shape for one view inside an audience. Carries
/// columns / fields / sections / search / filter / cells / actions /
/// blocks plus the optional per-view guard.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlatformView {
    pub name: String,
    pub view_type: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub columns: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sections: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub search: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub filter: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cells: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub submit: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blocks: Vec<String>,
    /// `ir-route-guards` §3.3 — per-platform-view policy + redirects.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guard: Option<ViewGuard>,
    /// LAZ-67 — analyzer-cached guard policy after walking
    /// view/route -> audience -> app -> built-in defaults.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_guard_policy: Option<Vec<PolicyAtom>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_serialises_snake_case() {
        let v = Platform::Web;
        let s = serde_json::to_string(&v).expect("serialize");
        assert_eq!(s, "\"web\"");
        let v = Platform::Mobile;
        let s = serde_json::to_string(&v).expect("serialize");
        assert_eq!(s, "\"mobile\"");
    }

    #[test]
    fn platform_surface_round_trips_with_one_audience() {
        let v = PlatformSurface {
            experience: "Customer".into(),
            platform: Platform::Web,
            uses_experience: None,
            audiences: vec![AudienceSurface {
                name: "public".into(),
                qualifiers: vec![],
                views: vec![],
                guard: None,
                span_ref: None,
            }],
            span_ref: None,
        };
        let s = serde_json::to_string(&v).expect("serialize");
        let back: PlatformSurface = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(v, back);
    }

    #[test]
    fn platform_view_round_trips_with_columns_and_actions() {
        let v = PlatformView {
            name: "all".into(),
            view_type: "list".into(),
            columns: vec!["name".into(), "status".into()],
            fields: vec![],
            sections: vec![],
            search: vec!["name".into()],
            filter: vec![],
            cells: vec![],
            actions: vec!["edit".into()],
            submit: None,
            blocks: vec![],
            guard: None,
            resolved_guard_policy: None,
            span_ref: None,
        };
        let s = serde_json::to_string(&v).expect("serialize");
        let back: PlatformView = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(v, back);
    }

    #[test]
    fn platform_view_minimal_omits_empty_vecs() {
        let v = PlatformView {
            name: "all".into(),
            view_type: "list".into(),
            columns: vec![],
            fields: vec![],
            sections: vec![],
            search: vec![],
            filter: vec![],
            cells: vec![],
            actions: vec![],
            submit: None,
            blocks: vec![],
            guard: None,
            resolved_guard_policy: None,
            span_ref: None,
        };
        let s = serde_json::to_string(&v).expect("serialize");
        assert_eq!(s, "{\"name\":\"all\",\"view_type\":\"list\"}");
    }
}
