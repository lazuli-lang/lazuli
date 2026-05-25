//! Lzx (`.lzx`) fact collectors.
//!
//! Two surfaces, two collectors:
//!
//! * `collect_lzx_experience_facts` reads each `experience` block and
//!   indexes its `view.routes` + `view.actions` into the
//!   `experiences: BTreeMap<String, ExperienceFacts>` map that
//!   `policy_reachability_diagnostics` consumes (it needs to know
//!   which command target an action invokes to follow the reach
//!   chain).
//! * `collect_lzx_operational_facts` reads top-level `route` and
//!   `surface` declarations and routes them into
//!   `operational.web_routes` / `operational.mobile_routes` /
//!   `operational.web_surfaces` / `operational.mobile_surfaces`. The
//!   surface-vs-route distinction comes from
//!   `lzx_route_surface_platform`, which honors an explicit `surface
//!   web|mobile` suffix and falls back to "has path = web" for legacy
//!   declarations.
//!
//! Extracted from `doctor/mod.rs` in rails-style R5-retry-9.

use std::collections::BTreeMap;

use lazuli_syntax::{LzxDocument, LzxPlatform};

use crate::doctor::{
    DoctorFile, ExperienceFacts, OperationalFacts, SourceFact, line_col_for_offset,
    route_slot_name,
};

pub(crate) fn collect_lzx_experience_facts(
    document: &LzxDocument,
    experiences: &mut BTreeMap<String, ExperienceFacts>,
) {
    for experience in &document.experiences {
        let facts = experiences.entry(experience.name.clone()).or_default();
        for view in &experience.views {
            facts.view_routes.insert(
                view.name.clone(),
                view.routes
                    .iter()
                    .filter_map(|route| route_slot_name(route).map(str::to_owned))
                    .collect(),
            );
            let actions = facts.view_actions.entry(view.name.clone()).or_default();
            for action in &view.actions {
                actions.insert(action.name.clone(), action.target.clone());
            }
        }
    }
}

pub(crate) fn collect_lzx_operational_facts(
    file: &DoctorFile,
    document: &LzxDocument,
    operational: &mut OperationalFacts,
) {
    for route in &document.routes {
        let (line, column) = line_col_for_offset(&file.source, route.span.start);
        let fact = SourceFact {
            path: file.path.clone(),
            line,
            column,
            name: route.name.clone(),
        };
        match lzx_route_surface_platform(route.surface.as_deref()) {
            Some(LzxPlatform::Web) => operational.web_routes.push(fact),
            Some(LzxPlatform::Mobile) => operational.mobile_routes.push(fact),
            None => {
                if route.path.is_some() {
                    operational.web_routes.push(fact);
                }
            }
        }
    }

    for surface in &document.surfaces {
        let (line, column) = line_col_for_offset(&file.source, surface.span.start);
        let fact = SourceFact {
            path: file.path.clone(),
            line,
            column,
            name: surface.experience.clone(),
        };
        match surface.platform {
            LzxPlatform::Web => operational.web_surfaces.push(fact),
            LzxPlatform::Mobile => operational.mobile_surfaces.push(fact),
        }
    }
}

pub(crate) fn lzx_route_surface_platform(surface: Option<&str>) -> Option<LzxPlatform> {
    match surface?.split_whitespace().last()? {
        "web" => Some(LzxPlatform::Web),
        "mobile" => Some(LzxPlatform::Mobile),
        _ => None,
    }
}
