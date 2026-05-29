//! Canonical-source orchestrator.
//!
//! Phase L of the inspect refactor moved every IR-driven projection
//! onto a single canonical-indent lowering pass. This module owns that
//! pass: it lifts the source once per `lazuli inspect` invocation,
//! caches the lifted `Auth` and `Tier3FeatureSlice` lookups, and runs
//! every per-feature projector against the shared inputs.
//!
//! Entry points:
//!
//! - [`inspect_canonical_source`] — `pub(crate)` adapter that calls
//!   the with-aliases variant with an empty alias map. Consumed by
//!   `cmd_mcp`, `tests.rs`, and the inspect-feature-summary renderer.
//! - [`inspect_canonical_source_with_aliases`] — the orchestrator
//!   proper. Walks the source, decides whether to lift the
//!   canonical-indent slice, runs the per-feature projectors, and
//!   returns the typed [`InspectReport`].
//!
//! Per-axis projection helpers live in the `projections/` subtree;
//! shared text walkers live in [`super::text_walkers`]; IR-to-inspect
//! shape projectors live in [`super::projectors`].
//!
//! The orchestrator itself is intentionally dumb — every axis-specific
//! decision (which fields to populate, how to fall back, what JSON key
//! to emit) lives in the projection it gates.
//!
//! Rails-style R9 split — the file is sub-divided into:
//!
//! * `mod.rs`             — entry points, `inspect_features` walker,
//!                          the small `inspect_refs` / `inspect_summary`
//!                          tail projectors.
//! * `tier3_collect.rs`   — `Tier3FeatureSlice`,
//!                          `collect_tier3_by_feature_with_aliases`,
//!                          `collect_auth_by_feature`.
//! * `feature_projector.rs` — `inspect_feature` per-feature projection.

use std::collections::BTreeMap;
use std::path::Path;

use crate::{app_manifest, plugin_manifest};

use super::expand::leading_spaces;
use super::expand_set::ExpandSet;
use super::text_walkers::{
    collect_command_names, collect_declared_ref_groups, collect_event_names,
    collect_extends_anchors, collect_extensible_by_features, collect_named_top_blocks,
    collect_query_names, collect_record_names, collect_resource_names, collect_surface_names,
    collect_used_namespaces, collect_view_anchors, collect_workflow_summaries,
};
use super::{InspectFeature, InspectProvides, InspectRefs, InspectReport, InspectSummary};

mod feature_projector;
mod tier3_collect;

pub(in crate::commands::inspect) use tier3_collect::Tier3FeatureSlice;

use feature_projector::inspect_feature;
use tier3_collect::{collect_auth_by_feature, collect_tier3_by_feature_with_aliases};

// -----------------------------------------------------------------------------
// Public entry points.
// -----------------------------------------------------------------------------

pub(crate) fn inspect_canonical_source(
    source: &str,
    input: &Path,
    expansions: ExpandSet,
) -> InspectReport {
    inspect_canonical_source_with_aliases(source, input, expansions, &BTreeMap::new())
}

/// B3 — variant of [`inspect_canonical_source`] that applies a plugin
/// alias map to lifted features so `--expand=resources` projections
/// surface `SemanticPluginType` carriers rather than the unresolved
/// `UserDefined` placeholders authored in `.lzi`.
pub(in crate::commands::inspect) fn inspect_canonical_source_with_aliases(
    source: &str,
    input: &Path,
    expansions: ExpandSet,
    alias_map: &BTreeMap<String, plugin_manifest::ResolvedPluginSemantic>,
) -> InspectReport {
    let lines: Vec<String> = source.lines().map(str::to_owned).collect();

    let is_lzx = input
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("lzx"))
        .unwrap_or(false);

    let lzx_module = if is_lzx {
        lazuli_syntax::parse_lzx_document(source)
            .ok()
            .map(|document| lazuli_analyzer::lower_lzx_document(&document))
    } else {
        None
    };

    let (lzx_app, routes, experiences, surfaces) = match lzx_module {
        Some(module) => (
            module.app,
            module.routes,
            module.experiences,
            module.surfaces,
        ),
        None => (None, Vec::new(), Vec::new(), Vec::new()),
    };

    // Phase L — lower the canonical-indent slice once per inspect call
    // and build a per-feature lookup. The slice is permissive about
    // unknown constructs, so a failed parse degrades gracefully into
    // an empty lookup; the text-pattern inspect path still runs.
    //
    // `cookie-sessions-child` — the `security` axis also reads this
    // lookup (to project the lowered `auth.sessions.cookie` transport
    // envelope), so populate it when EITHER `auth` OR `security` is set.
    let auth_by_feature = if (expansions.auth || expansions.security) && !is_lzx {
        collect_auth_by_feature(source)
    } else {
        BTreeMap::new()
    };

    // Phase L Tier 3 — collect the lifted `Job`/`Webhook`/
    // `EventGroup` shapes for every feature in one pass. Reuses the
    // same parse-and-lower the auth lookup runs; degradation rules
    // match (empty map on parse failure). Tier 4 follow-up also
    // surfaces typed `policies` here so `inspect_policies`/
    // `inspect_tests` consume the IR instead of a text walker.
    let tier3_by_feature = if (expansions.jobs
        || expansions.webhooks
        || expansions.event_groups
        || expansions.notifications
        || expansions.policies
        || expansions.tests
        || expansions.migrations
        || expansions.caches
        || expansions.aggregates
        || expansions.defaults
        || expansions.commands
        || expansions.apis
        || expansions.resources
        || expansions.queries
        || expansions.records
        || expansions.errors)
        && !is_lzx
    {
        collect_tier3_by_feature_with_aliases(source, alias_map)
    } else {
        BTreeMap::new()
    };

    let registry = app_manifest::parse_app_registry(source);
    let webhook_events = expansions.webhook_events.then(|| {
        registry
            .as_ref()
            .map(|registry| registry.webhook_events.clone())
            .unwrap_or_default()
    });

    let app = app_manifest::parse_app_manifest(source).or(lzx_app);
    // Roadmap §1.2 — unified HTTP hygiene projection. Only populated
    // when the flag is set; the typed blocks still surface via `app`
    // either way.
    let http = if expansions.http {
        crate::inspect::expand_http::expand_http(app.as_ref())
    } else {
        None
    };

    InspectReport {
        schema: "lazuli.inspect.v0",
        source: input.display().to_string(),
        expand: expansions.labels(),
        workspace: app_manifest::parse_app_workspace(source),
        contracts: app_manifest::parse_app_contracts(source),
        app,
        registry,
        webhook_events,
        profiles: app_manifest::parse_app_profiles(source),
        routes,
        experiences,
        surfaces,
        http,
        features: inspect_features(&lines, expansions, &auth_by_feature, &tier3_by_feature),
    }
}

// -----------------------------------------------------------------------------
// Per-feature orchestration.
// -----------------------------------------------------------------------------

fn inspect_features(
    lines: &[String],
    expansions: ExpandSet,
    auth_by_feature: &BTreeMap<String, lazuli_ir::Auth>,
    tier3_by_feature: &BTreeMap<String, Tier3FeatureSlice>,
) -> Vec<InspectFeature> {
    let mut features = Vec::new();
    let mut index = 0;

    while index < lines.len() {
        if leading_spaces(&lines[index]) == 0 && lines[index].trim_start().starts_with("feature ") {
            let start = index;
            index += 1;

            while index < lines.len() {
                if leading_spaces(&lines[index]) == 0
                    && lines[index].trim_start().starts_with("feature ")
                {
                    break;
                }
                index += 1;
            }

            features.push(inspect_feature(
                &lines[start..index],
                expansions,
                auth_by_feature,
                tier3_by_feature,
            ));
        } else {
            index += 1;
        }
    }

    features
}

// -----------------------------------------------------------------------------
// Feature-level `refs` and `summary` projections.
// -----------------------------------------------------------------------------

pub(super) fn inspect_refs(lines: &[String]) -> InspectRefs {
    let declared = collect_declared_ref_groups(lines);
    let declared_namespaces: std::collections::BTreeSet<String> = declared
        .iter()
        .flat_map(|group| group.namespaces.iter().cloned())
        .collect();
    let used_namespaces = collect_used_namespaces(lines);
    let used: Vec<String> = used_namespaces.iter().cloned().collect();
    let (missing, unused) = if declared_namespaces.is_empty() {
        (Vec::new(), Vec::new())
    } else {
        (
            used_namespaces
                .difference(&declared_namespaces)
                .cloned()
                .collect(),
            declared_namespaces
                .difference(&used_namespaces)
                .cloned()
                .collect(),
        )
    };

    InspectRefs {
        declared,
        used,
        missing,
        unused,
    }
}

pub(super) fn inspect_summary(lines: &[String]) -> InspectSummary {
    let resources = collect_resource_names(lines);
    let records = collect_record_names(lines);
    let queries = collect_query_names(lines);
    let events = collect_event_names(lines);
    let anchors = collect_view_anchors(lines);
    let mut types = resources.clone();
    types.extend(records.clone());

    InspectSummary {
        provides: InspectProvides {
            types,
            queries: queries.clone(),
            events: events.clone(),
            anchors: anchors.clone(),
        },
        resources,
        records,
        queries,
        commands: collect_command_names(lines),
        workflows: collect_workflow_summaries(lines),
        jobs: collect_named_top_blocks(lines, "job"),
        webhooks: collect_named_top_blocks(lines, "webhook"),
        events,
        surfaces: collect_surface_names(lines),
        anchors,
        extends: collect_extends_anchors(lines),
        extended_by: collect_extensible_by_features(lines),
    }
}
