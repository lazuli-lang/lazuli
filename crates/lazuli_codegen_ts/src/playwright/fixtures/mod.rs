//! Playwright fixture emitter — generates the per-role `as<Role>()`
//! helpers and lifecycle-state coercion that app authors use in
//! handwritten specs.
//!
//! The output is one TS file co-imported with the app's own
//! `e2e/helpers/*` modules. Whether the app has those helpers is
//! signalled by [`PlaywrightFixtureConfig::helpers`]: when present, we
//! re-export the helper-typed surface; when absent we fall back to a
//! self-contained TODO-stub so the file still type-checks.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

use lazuli_ir::{
    AppManifest, AppRoute, Experience, Feature, Module, Platform, PlatformSurface, Resource,
    SurfaceTarget, View, ViewGuard,
};

mod naming;
mod policy;

use naming::{
    canonical, escape_ts_single_quoted, lifecycle_type_name, pascal_case, route_feature_from_name,
    route_target_feature, route_target_view_name, surface_feature, view_route,
};
use policy::{PolicyAtom, build_policy_lookup, roles_from_atoms, roles_from_policy_refs};

include!("mod_p1.rs");
include!("mod_p2.rs");
