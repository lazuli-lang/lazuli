//! Per-route grouping key + the resolved-guard / route-object records
//! that flow between context resolution and TS emission.
//!
//! Routes that share `(feature, platform, audience)` end up in the same
//! generated SDK file (e.g.
//! `dist/ts-web/<feature>/<feature>.<platform>.<audience>.gen.ts`).
//! `RouteGroupKey` owns the filename + registry-import path so the
//! shape of those generated paths lives in exactly one place.
//!
//! The remaining records (`ResolvedGuard`, `RouteObject`,
//! `RouteRegistryEntry`) are the post-resolution shapes the emitter
//! consumes — kept here so they're alongside the key they're keyed by.

use super::RouteGuardTarget;
use super::policy::ResolvedPolicy;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct RouteGroupKey {
    pub(super) feature: String,
    pub(super) platform: String,
    pub(super) audience: String,
}

impl RouteGroupKey {
    pub(super) fn file_path(&self, target: RouteGuardTarget) -> String {
        format!(
            "dist/{}/{}/{}.{}.{}.gen.ts",
            target.dist_prefix(),
            self.feature,
            self.feature,
            self.platform,
            self.audience
        )
    }

    pub(super) fn registry_import_path(&self) -> String {
        format!(
            "../{}/{}.{}.{}.gen.js",
            self.feature, self.feature, self.platform, self.audience
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ResolvedGuard {
    pub(super) policies: Vec<ResolvedPolicy>,
    pub(super) on_unauthenticated: Option<String>,
    pub(super) on_unauthorized: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RouteObject {
    pub(super) path: String,
    pub(super) const_name: String,
    pub(super) component: String,
    pub(super) guard: ResolvedGuard,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RouteRegistryEntry {
    pub(super) path: String,
    pub(super) const_name: String,
    pub(super) group: RouteGroupKey,
}

/// JSON-safe TS string literal — `serde_json::to_string` escapes
/// embedded quotes, backslashes, and control chars exactly the way TS
/// expects.
pub(super) fn ts_string(value: &str) -> String {
    serde_json::to_string(value).expect("string literal must serialize")
}
