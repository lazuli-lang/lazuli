//! `lazuli inspect` — typed IR projector and canonical-source expander.
//!
//! Carved out of `main.rs` as part of Wave R3-D (Rails-style refactor).
//! This module owns every artifact `lazuli inspect` produces:
//!
//! - **Entry point** (`inspect_command`): dispatches on `--format`
//!   (Json vs Lazuli), `--expand=<axes>` (typed projection axes), and
//!   `--include=<lists>` (sidecar resources like the lifted manifest).
//! - **Symbol mode** (`inspect_symbol_*`): the per-symbol lookup
//!   surface (`lazuli inspect symbol:<name>`) used by the LSP for
//!   go-to-definition. Resolves cross-file, cross-feature, and
//!   plugin-aliased references.
//! - **Canonical source + expansions** (`inspect_canonical_source`,
//!   `expand_canonical_source`): the text-level expansion engine that
//!   surfaces inferred targets, expand-set projections, and event
//!   group decomposition.
//! - **Per-feature projections**: every `inspect_<axis>` (storage,
//!   auth, agents, jobs, webhooks, security, locators, defaults,
//!   tests, requirements, external_calls, notifications, ...). Each
//!   projection is a pure function `&[String] -> Vec<Inspect<Axis>>`
//!   that walks the trimmed source lines for a feature block.
//! - **IR projectors** (`project_job`, `project_webhook`,
//!   `project_event_group`, `project_aggregate`, `project_invariant`,
//!   `project_file_capability`, `project_auth`,
//!   `project_defaults_from_ir`): convert lifted `lazuli_ir::*`
//!   shapes into the JSON-stable `Inspect<X>` shapes serialized by
//!   `inspect_json_value`.
//! - **Helpers**: predicate / target-expr / let-binding / command
//!   effect renderers; type-ref + policy-ref + path printers; the
//!   `leading_spaces` / `is_identifier` / `is_type_name` /
//!   `namespace_references` line-walker primitives that the
//!   projectors share.
//!
//! ABI: `lazuli inspect --help` and `lazuli inspect ...` are
//! byte-identical with the pre-split build. The CLI flag enums
//! (`InspectFormat`, `InspectInclude`) stay in `main.rs` because
//! they're `#[derive(ValueEnum)]` on `Commands::Inspect`; the rest of
//! the cluster — including the `ExpandSet` aggregator, the
//! `Inspect<X>` serializable shapes, and all per-axis projectors —
//! lives here.
//!
//! Cross-refs:
//! - `cmd_mcp` reaches for `crate::ExpandSet`,
//!   `crate::parse_expand_set`, and `crate::inspect_json_value`;
//!   `main.rs` re-exports them so the MCP server keeps compiling
//!   without touching its import block.
//! - `tests.rs` reaches for `inspect_canonical_source`,
//!   `inspect_json_value`, `expand_canonical_source`,
//!   `parse_expand_set`, `render_inspect_symbol_lazuli`, all
//!   re-exported through main.rs.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Serialize;

use crate::casing::pascal_case;
use crate::{
    InspectFormat, InspectInclude, app_manifest, build_module_from_path,
    build_module_with_source_from_path, lazurite_manifest, plugin_manifest, plugin_semantic_resolver,
    project_root_for_input, read_package_lzi_source,
};

mod expand_set;

pub(crate) use expand_set::{ExpandSet, parse_expand_set};

pub(crate) fn inspect_command(
    input: &Path,
    expand: &str,
    format: InspectFormat,
    include: &[InspectInclude],
) -> Result<()> {
    // Symbol-mode dispatch per docs/proposals/lsp-symbol-origin.md §5.3.
    // When `input` is a bare or dotted symbol name (not a path), look it up
    // in the SymbolOriginIndex and emit the JSON shape from §5.2 instead of
    // the path-mode inspect output.
    if let Some(symbol) = inspect_symbol_arg(input) {
        return inspect_symbol_command(symbol, format);
    }

    let expansions = parse_expand_set(expand)?;
    let source_path = inspect_source_path(input);
    let source = if input.is_dir() && expansions.any() {
        read_package_lzi_source(input)?
    } else {
        fs::read_to_string(&source_path)
            .with_context(|| format!("failed to read {}", source_path.display()))?
    };
    let report_input = if input.is_dir() && expansions.any() {
        input
    } else {
        source_path.as_path()
    };

    match format {
        InspectFormat::Json => {
            // B3 — `input` carries the directory the author passed
            // (often `.`), while `source_path` has already resolved to
            // `app/app.lzi`. The manifest lives at the *original*
            // directory; pass both so the plugin alias-map lookup
            // anchors at the right Lazurite.toml.
            let output = inspect_json_value(&source, report_input, input, expansions, include)?;
            println!("{}", serde_json::to_string_pretty(&output)?);
        }
        InspectFormat::Lazuli => {
            if expansions.any() {
                print!("{}", expand_canonical_source_with(&source, expansions));
            } else {
                // Default human projection: the C4 / M3 features-summary
                // renderer, which annotates each opted-in resource with
                // `(conventions: <bundle>)` and tags synth-derived
                // commands/queries per `ir-resource-conventions-crud.md`
                // §11 + `ir-resource-conventions-me.md` §8. Falls back to
                // the verbatim source echo when the canonical-indent
                // slice can't be parsed/lowered — `inspect` is a
                // read-only projection, not a check, so a parse failure
                // here must not flip the command into an error path.
                print!("{}", render_lazuli_features_summary(&source));
            }
        }
    }

    Ok(())
}

/// Default `--format=lazuli` human projection: parse the canonical-indent
/// slice, lower each `FeatureSkeleton` into IR (which runs the convention
/// synth pass), and render the §11 / §8 features-summary digest with
/// `(conventions: <bundle>)` resource annotations and `[conv:<bundle>]`
/// synth-origin tags.
///
/// Falls back to the verbatim source on any parse/lower failure — inspect
/// is a read-only projection per `docs/canonical-semantics.md`, so a
/// downstream parser bug must not block the human view. The fallback
/// preserves pre-features-summary behavior for any document the
/// canonical-indent slice doesn't yet understand.
fn render_lazuli_features_summary(source: &str) -> String {
    let Ok(skeletons) = lazuli_syntax::parse_feature_skeletons(source) else {
        return source.to_owned();
    };
    if skeletons.is_empty() {
        return source.to_owned();
    }
    let mut features = Vec::with_capacity(skeletons.len());
    for skeleton in &skeletons {
        match lazuli_analyzer::lower_feature_skeleton(skeleton) {
            Ok(feature) => features.push(feature),
            Err(_) => return source.to_owned(),
        }
    }
    crate::inspect::features_summary::render_features_summary(&features)
}

/// Detect symbol-mode arguments per `docs/proposals/lsp-symbol-origin.md` §5.3.
///
/// Returns `Some(arg)` when the input is a bare or dotted symbol name (e.g.
/// `Gender`, `host.Gender`), and `None` when path-mode rules apply:
/// - contains a path separator (`/` or `\`)
/// - ends in `.lzi`
/// - is `.` or `..`
/// - points to an existing file or directory
///
/// The disambiguation is lexical first (separator/extension/sentinel) and
/// filesystem-aware second (existing path → path mode). Authors who want
/// the feature-named symbol when a directory shares the name can qualify
/// via `<feature>.<Type>`.
mod symbol;

use symbol::{inspect_symbol_arg, inspect_symbol_command};
pub(crate) use symbol::render_inspect_symbol_lazuli;


pub(crate) fn inspect_source_path(input: &Path) -> PathBuf {
    if input.is_dir() {
        return lazurite_manifest::resolve_in_app_dir(input, "app.lzi");
    }

    input.to_path_buf()
}

pub(crate) fn inspect_json_value(
    source: &str,
    input: &Path,
    project_root_hint: &Path,
    expansions: ExpandSet,
    include: &[InspectInclude],
) -> Result<serde_json::Value> {
    // Prefer the caller-supplied project root (the directory the
    // author passed on the command line). When the hint isn't a
    // directory (typical for single-file `lazuli inspect host.lzi`
    // invocations), walk upward from the input's parent to find a
    // directory that contains `Lazurite.toml` — without this, the
    // single-file path never sees the manifest and B3's plugin
    // alias map stays empty.
    let project_root = if project_root_hint.is_dir() {
        project_root_hint.to_path_buf()
    } else {
        let mut candidate: PathBuf = project_root_for_input(input);
        // Bounded walk-up so we don't escape the workspace; 8 levels
        // is generous for `app/features/<name>/<file>.lzi` layouts.
        for _ in 0..8 {
            if candidate.join("Lazurite.toml").is_file()
                || candidate.join("lazurite.toml").is_file()
            {
                break;
            }
            let Some(parent) = candidate.parent().map(Path::to_path_buf) else {
                break;
            };
            if parent == candidate {
                break;
            }
            candidate = parent;
        }
        candidate
    };
    // B3 — build the plugin alias map up front so the inspect report
    // and the optional `plugin_semantic_types` manifest projection
    // share a single source of truth. The map is read once per
    // inspect invocation per
    // `docs/proposals/semantic-types-plugin-locales.md` §IR and resolution.
    let alias_map = lazurite_manifest::load(&project_root)
        .ok()
        .flatten()
        .and_then(|manifest| {
            plugin_manifest::build_alias_map(Some(&manifest), &project_root).ok()
        })
        .unwrap_or_default();
    let report =
        inspect_canonical_source_with_aliases(source, input, expansions, &alias_map);
    let manifest = lazurite_manifest::load(&project_root).with_context(|| {
        format!(
            "failed to read {}",
            project_root.join("Lazurite.toml").display()
        )
    })?;

    if let Some(manifest) = manifest {
        // B3 — surface the plugin-contributed `@semantic.<Name>` alias
        // map alongside the existing manifest projection so agents
        // reading `lazuli inspect --include=manifest --format=json`
        // discover which aliases are active and where each resolves.
        // The per-alias entry carries the proposal-mandated keys:
        // `kind`, `plugin`, `name`, `alias`, `carrier`, `origin`.
        let plugin_semantic_types =
            inspect_plugin_semantic_types(&manifest, &project_root);
        return Ok(serde_json::json!({
            "ir": report,
            "manifest": manifest.inspect_view(),
            "plugin_semantic_types": plugin_semantic_types,
        }));
    }

    if include.contains(&InspectInclude::Manifest) {
        return Ok(serde_json::json!({
            "ir": report,
            "manifest": serde_json::Value::Null,
            "plugin_semantic_types": serde_json::Value::Array(Vec::new()),
        }));
    }

    Ok(serde_json::to_value(report)?)
}

/// B3 — flatten the resolved plugin semantic alias map into the
/// proposal §IR-and-resolution shape: each entry exposes
/// `{ kind, plugin, name, alias, carrier, origin, validator,
/// formatter }`. Sorted by alias.
fn inspect_plugin_semantic_types(
    manifest: &lazurite_manifest::Manifest,
    project_root: &Path,
) -> serde_json::Value {
    let map = match plugin_manifest::build_alias_map(Some(manifest), project_root) {
        Ok(map) => map,
        Err(err) => {
            // Surface the failure so consumers see something rather
            // than silently emitting an empty array.
            return serde_json::json!({ "error": err.to_string() });
        }
    };
    let entries: Vec<serde_json::Value> = map
        .into_iter()
        .map(|(alias, resolved)| {
            serde_json::json!({
                "kind": "semantic_plugin",
                "plugin": resolved.plugin_namespace,
                "name": resolved.name,
                "alias": alias,
                "carrier": format!("{:?}", resolved.carrier),
                "validator": resolved.validator,
                "formatter": resolved.formatter,
                "origin": format!("plugin manifest:{}", resolved.plugin_namespace),
            })
        })
        .collect();
    serde_json::Value::Array(entries)
}

mod report_types;

// Re-export inside the inspect tree so sibling submodules (projectors, etc.)
// can `use super::InspectJob;` etc. The structs are `pub(super)` inside
// report_types — their effective visibility stays at inspect-internal scope.
pub(in crate::commands::inspect) use report_types::*;
pub(crate) use report_types::{InspectReport, InspectFeature};


pub(crate) fn inspect_canonical_source(source: &str, input: &Path, expansions: ExpandSet) -> InspectReport {
    inspect_canonical_source_with_aliases(source, input, expansions, &std::collections::BTreeMap::new())
}

/// B3 — variant of [`inspect_canonical_source`] that applies a plugin
/// alias map to lifted features so `--expand=resources` projections
/// surface `SemanticPluginType` carriers rather than the unresolved
/// `UserDefined` placeholders authored in `.lzi`.
fn inspect_canonical_source_with_aliases(
    source: &str,
    input: &Path,
    expansions: ExpandSet,
    alias_map: &std::collections::BTreeMap<String, plugin_manifest::ResolvedPluginSemantic>,
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
    let auth_by_feature = if expansions.auth && !is_lzx {
        collect_auth_by_feature(source)
    } else {
        std::collections::BTreeMap::new()
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
        std::collections::BTreeMap::new()
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

/// Phase L Tier 3 — lower the canonical-indent slice once per inspect
/// call and build a per-feature lookup of `(jobs, webhooks,
/// event_groups)`. Same degradation rules as `collect_auth_by_feature`:
/// failures fall through to an empty map so `--expand=jobs` etc. are
/// projections, not checks.
fn collect_tier3_by_feature(source: &str) -> std::collections::BTreeMap<String, Tier3FeatureSlice> {
    collect_tier3_by_feature_with_aliases(source, &std::collections::BTreeMap::new())
}

/// B3 — variant that applies the plugin alias map to lifted features
/// so inspect's `--expand=resources` projection surfaces
/// `SemanticPluginType` carriers rather than `UserDefined` placeholders.
fn collect_tier3_by_feature_with_aliases(
    source: &str,
    alias_map: &std::collections::BTreeMap<String, plugin_manifest::ResolvedPluginSemantic>,
) -> std::collections::BTreeMap<String, Tier3FeatureSlice> {
    let mut map = std::collections::BTreeMap::new();
    let Ok(features) = lazuli_syntax::parse_feature_skeletons(source) else {
        return map;
    };
    for feature_ast in features {
        let Ok(mut feature_ir) = lazuli_analyzer::lower_feature_skeleton(&feature_ast) else {
            continue;
        };
        if !alias_map.is_empty() {
            // Reuse the package-level resolver pass on this single
            // feature. Wrap in a transient module so the walker
            // signature is stable across both callers.
            let mut transient = lazuli_ir::Module {
                workspace: None,
                contracts: Vec::new(),
                app: None,
                registry: None,
                profiles: Vec::new(),
                design: None,
                rbac: None,
                features: vec![feature_ir],
            };
            plugin_semantic_resolver::apply_plugin_semantic_resolution(
                &mut transient,
                alias_map,
            );
            feature_ir = transient.features.pop().unwrap();
        }
        map.insert(
            feature_ir.name.clone(),
            Tier3FeatureSlice {
                jobs: feature_ir.jobs,
                webhooks: feature_ir.webhooks,
                event_groups: feature_ir.event_groups,
                tenant_migrations: feature_ir.tenant_migrations,
                notifications: feature_ir.notifications,
                policies: feature_ir.policies,
                caches: feature_ir.caches,
                aggregates: feature_ir.aggregates,
                defaults: feature_ir.defaults,
                resource_names: feature_ir
                    .resources
                    .iter()
                    .map(|r| r.name.clone())
                    .collect(),
                commands: feature_ir.commands,
                apis: feature_ir.apis,
                resources: feature_ir.resources,
                queries: feature_ir.queries,
                records: feature_ir.records,
                errors: feature_ir.errors,
            },
        );
    }
    map
}

struct Tier3FeatureSlice {
    jobs: Vec<lazuli_ir::Job>,
    webhooks: Vec<lazuli_ir::Webhook>,
    event_groups: Vec<lazuli_ir::EventGroup>,
    /// Migrations bucket cycle Route C — lifted `tenant_migration`
    /// declarations for `--expand=migrations`.
    tenant_migrations: Vec<lazuli_ir::TenantMigration>,
    /// Notifications expanded bucket cycle — lifted `notification`
    /// declarations. Powers the typed `digest`/`throttle` projection
    /// in `inspect_notifications`; the text-walker keeps owning the
    /// scalar fields so the projection stays additive.
    notifications: Vec<lazuli_ir::Notification>,
    /// Tier 4 follow-up — lifted `policies` block. Powers the typed
    /// `category -> atoms` lookup that `inspect_policies` and
    /// `inspect_tests` consume; retires the `collect_policy_atoms`
    /// text walker.
    policies: lazuli_ir::Policies,
    /// Cache bucket cycle (CL.C.3) — lifted feature-level
    /// `cache <name>` profile declarations. Powers `--expand=caches`.
    caches: Vec<lazuli_ir::CacheProfile>,
    /// CL.C.4 — lifted `aggregate <Name>` declarations. Powers
    /// `--expand=aggregates`.
    aggregates: Vec<lazuli_ir::Aggregate>,
    /// Phase L Tier 4a — lifted feature-level `defaults` block.
    /// Powers `--expand=defaults` IR-driven projection; replaces the
    /// text-pattern walker for the canonical-indent code path.
    defaults: lazuli_ir::Defaults,
    /// Phase L Tier 4a — resource names lifted from
    /// `Feature.resources`. Used by `--expand=defaults` to compute
    /// `applies_to` for `tenancy`/`timestamps` defaults without
    /// re-walking the source text.
    resource_names: Vec<String>,
    /// Phase L Tier 4b — lifted `command <name>` declarations on the
    /// feature. Powers `--expand=commands`; emitted verbatim from IR
    /// so downstream consumers see the typed Command shape (with
    /// audit, approval, invalidates, etc.) without re-deriving from
    /// text.
    commands: Vec<lazuli_ir::Command>,
    /// Phase L Tier 4b — lifted `api <name>` declarations on the
    /// feature. Powers `--expand=apis` (accepting `api` or `apis`).
    apis: Vec<lazuli_ir::Api>,
    /// Phase L Tier 4c — lifted `resource <Name>` declarations on the
    /// feature. Powers `--expand=resources`.
    resources: Vec<lazuli_ir::Resource>,
    /// Phase L Tier 4d — lifted `query.{list,lookup,sql}` declarations
    /// on the feature. Powers `--expand=queries`.
    queries: Vec<lazuli_ir::Query>,
    /// Phase L Tier 4d — lifted `record <Name>` declarations on the
    /// feature. Powers `--expand=records`.
    records: Vec<lazuli_ir::Record>,
    /// IR Error-Vocab (Cell PARSE-1) — lifted `errors` block. `None`
    /// when the feature declared no `errors` block. Powers
    /// `--expand=errors` projection.
    errors: Option<lazuli_ir::FeatureErrors>,
}

/// Phase L — run the canonical-indent slice and build a `feature_name ->
/// IR Auth` lookup. Failures in either parse or lower silently degrade
/// to an empty lookup: `--expand=auth` is a projection, not a check,
/// so it must not flip inspect into an error path.
fn collect_auth_by_feature(source: &str) -> std::collections::BTreeMap<String, lazuli_ir::Auth> {
    let mut map = std::collections::BTreeMap::new();
    let Ok(features) = lazuli_syntax::parse_feature_skeletons(source) else {
        return map;
    };
    for feature in features {
        if let Some(auth_ast) = feature.auth.as_ref() {
            if let Ok(auth_ir) = lazuli_analyzer::lower_auth(auth_ast) {
                map.insert(feature.name.clone(), auth_ir);
            }
        }
    }
    map
}

fn inspect_features(
    lines: &[String],
    expansions: ExpandSet,
    auth_by_feature: &std::collections::BTreeMap<String, lazuli_ir::Auth>,
    tier3_by_feature: &std::collections::BTreeMap<String, Tier3FeatureSlice>,
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

fn inspect_feature(
    lines: &[String],
    expansions: ExpandSet,
    auth_by_feature: &std::collections::BTreeMap<String, lazuli_ir::Auth>,
    tier3_by_feature: &std::collections::BTreeMap<String, Tier3FeatureSlice>,
) -> InspectFeature {
    let name = lines
        .first()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("unknown")
        .to_owned();
    let external_calls = inspect_external_calls(&name, lines);
    let agents = inspect_agents(lines);
    let tier3 = tier3_by_feature.get(&name);
    // Tier 4 follow-up — `policies` category lookup now reads typed
    // `Feature.policies.categories` from the Tier 3 slice. Falls back
    // to an empty map when the slice is absent (either because the
    // feature has no `policies` block, or because no expand flag
    // gated the slice collection).
    let policies: BTreeMap<String, Vec<String>> = tier3
        .map(|t| {
            t.policies
                .categories
                .iter()
                .map(|c| (c.name.clone(), c.atoms.clone()))
                .collect()
        })
        .unwrap_or_default();
    let notifications = inspect_notifications(lines, tier3);

    let tools = expansions
        .tools
        .then(|| inspect_agent_tools_projection(&agents));

    let expose = expansions
        .expose
        .then(|| inspect_expose_projection(&name, &agents, lines));

    // Phase L — auth projection is only present when `--expand=auth`
    // is set AND the feature declared an `auth` block. Features
    // without auth omit the field entirely so consumers can distinguish
    // "no auth declared" from "auth declared but empty".
    let auth = expansions
        .auth
        .then(|| {
            auth_by_feature
                .get(&name)
                .map(|auth| project_auth(&name, auth))
        })
        .flatten();

    // Phase L Tier 2 — storage projection harvests every `@cap.File(...)`
    // site from the source text and runs each through the typed
    // analyzer pass. The projection is omitted when the feature
    // declares zero file capabilities; that distinguishes "no storage"
    // from "storage declared but empty" for downstream consumers.
    let storage = expansions
        .storage
        .then(|| inspect_storage_projection(lines))
        .filter(|s| !s.fields.is_empty() || !s.api_outputs.is_empty());

    // Phase L Tier 3 — jobs/webhooks/event_groups projections. Each is
    // present only when the matching expand flag is set AND the feature
    // actually declares the construct. Empty arrays still surface so
    // consumers can distinguish "flag not set" from "no constructs
    // declared". `tier3` is bound earlier in this function so the
    // notification projection can read typed `digest`/`throttle`.
    let jobs_projection = expansions.jobs.then(|| {
        tier3
            .map(|t| t.jobs.iter().map(project_job).collect::<Vec<_>>())
            .unwrap_or_default()
    });
    let webhooks_projection = expansions.webhooks.then(|| {
        tier3
            .map(|t| t.webhooks.iter().map(project_webhook).collect::<Vec<_>>())
            .unwrap_or_default()
    });
    let event_groups_projection = expansions.event_groups.then(|| {
        tier3
            .map(|t| {
                t.event_groups
                    .iter()
                    .map(project_event_group)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    });
    // Migrations bucket cycle Route C — `--expand=migrations`. Surfaces
    // every lifted `ir::TenantMigration` on the feature.
    let tenant_migrations_projection = expansions.migrations.then(|| {
        tier3
            .map(|t| t.tenant_migrations.clone())
            .unwrap_or_default()
    });
    // CL.C.4 — `--expand=aggregates` projection.
    let aggregates_projection = expansions.aggregates.then(|| {
        tier3
            .map(|t| {
                t.aggregates
                    .iter()
                    .map(project_aggregate)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    });
    // Cache bucket cycle (CL.C.3) — `--expand=caches`. Surfaces every
    // lifted feature-level `cache <name>` profile on the feature.
    // Empty arrays still surface so consumers can distinguish "flag
    // not set" from "no profiles declared".
    let caches_projection = expansions
        .caches
        .then(|| tier3.map(|t| t.caches.clone()).unwrap_or_default());
    // Phase L Tier 4b — `--expand=commands` projects every lifted
    // `ir::Command` on the feature. Empty arrays surface so consumers
    // distinguish "flag not set" from "no commands declared".
    let commands_projection = expansions
        .commands
        .then(|| tier3.map(|t| t.commands.clone()).unwrap_or_default());
    // Phase L Tier 4b — `--expand=apis` projects every lifted
    // `ir::Api` on the feature.
    let apis_projection = expansions
        .apis
        .then(|| tier3.map(|t| t.apis.clone()).unwrap_or_default());
    // Phase L Tier 4c — `--expand=resources` projects every lifted
    // `ir::Resource` on the feature.
    let resources_projection = expansions
        .resources
        .then(|| tier3.map(|t| t.resources.clone()).unwrap_or_default());
    // Phase L Tier 4d — `--expand=queries` projects every lifted
    // `ir::Query` on the feature.
    let queries_projection = expansions
        .queries
        .then(|| tier3.map(|t| t.queries.clone()).unwrap_or_default());
    // Phase L Tier 4d — `--expand=records` projects every lifted
    // `ir::Record` on the feature.
    let records_projection = expansions
        .records
        .then(|| tier3.map(|t| t.records.clone()).unwrap_or_default());
    // IR Error-Vocab (Cell PARSE-1) — `--expand=errors` projects the
    // lifted `ir::FeatureErrors` block (None when the feature has no
    // `errors` block authored). The outer `Option` is gated by the
    // expansion flag; the inner `Option` is gated by authoring.
    let errors_projection = expansions
        .errors
        .then(|| tier3.and_then(|t| t.errors.clone()))
        .flatten();

    InspectFeature {
        name,
        requirements: inspect_requirements(lines),
        external_calls,
        agents,
        notifications,
        refs: expansions.refs.then(|| inspect_refs(lines)),
        summary: expansions.summary.then(|| inspect_summary(lines)),
        locators: expansions.locators.then(|| inspect_locators(lines)),
        dependencies: expansions.dependencies.then(|| inspect_dependencies(lines)),
        security: expansions.security.then(|| inspect_security(lines)),
        defaults: expansions.defaults.then(|| inspect_defaults(lines, tier3)),
        events: expansions.events.then(|| inspect_events(lines)),
        built_in_trace_events: expansions.events.then(inspect_built_in_trace_events),
        targets: expansions.targets.then(|| inspect_targets(lines)),
        policies: expansions
            .policies
            .then(|| inspect_policies(lines, &policies, tier3)),
        tests: expansions.tests.then(|| inspect_tests(lines, &policies)),
        tools,
        expose,
        auth,
        storage,
        jobs: jobs_projection,
        webhooks: webhooks_projection,
        event_groups: event_groups_projection,
        tenant_migrations: tenant_migrations_projection,
        caches: caches_projection,
        aggregates: aggregates_projection,
        commands: commands_projection,
        apis: apis_projection,
        resources: resources_projection,
        queries: queries_projection,
        records: records_projection,
        errors: errors_projection,
    }
}

// -----------------------------------------------------------------------------
// Phase L Tier 3 — IR -> Inspect projections.
// -----------------------------------------------------------------------------

mod projectors;

use projectors::{
    project_aggregate, project_event_group, project_invariant, project_job, project_webhook,
};

pub(in crate::commands::inspect) mod formatters;

use formatters::{
    compare_op_to_string, format_e2ee_capability, format_encrypted_capability,
    format_file_capability, format_file_size_literal, format_file_visibility,
    format_hashed_capability, format_pii_capability, format_qname, format_token_capability,
    format_type_ref, inspect_assignments_to_string, inspect_command_effect_to_string,
    inspect_expr_to_string, inspect_let_binding_to_string, inspect_target_expr_to_string,
    op_as_str, path_to_string, policy_ref_to_string, predicate_to_string, tool_kind_segment,
    tool_ref_to_string, type_ref_to_string,
};

mod projections;

use projections::{
    inspect_agent_tools_projection, inspect_agents, inspect_built_in_trace_events,
    inspect_defaults, inspect_dependencies, inspect_events, inspect_expose_projection,
    inspect_external_calls, inspect_locators, inspect_notifications, inspect_policies,
    inspect_requirements, inspect_storage_projection, inspect_targets, inspect_tests,
    project_auth,
};



fn inspect_refs(lines: &[String]) -> InspectRefs {
    let declared = collect_declared_ref_groups(lines);
    let declared_namespaces: BTreeSet<String> = declared
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

fn inspect_summary(lines: &[String]) -> InspectSummary {
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


mod security;

use security::inspect_security;

mod expand;

#[cfg(test)]
#[allow(unused_imports)]
pub(crate) use expand::expand_canonical_source;
use expand::{
    collect_event_decls, collect_event_groups, expand_canonical_source_with, feature_has_id_lookup,
    is_identifier, is_type_name, leading_spaces, namespace_references, parse_ident_list,
};

mod text_walkers;

use text_walkers::{
    block_has_exact_line, block_prefixed_value, block_scalar_value, collect_command_names,
    collect_declared_ref_groups, collect_event_names, collect_extends_anchors,
    collect_extensible_by_features, collect_job_and_webhook_names, collect_named_top_blocks,
    collect_query_names, collect_record_names, collect_resource_names, collect_surface_names,
    collect_used_namespaces, collect_view_anchors, collect_workflow_summaries, command_blocks,
    command_input_names, command_name, command_needs_inferred_target, command_route_names,
    direct_child_value, direct_child_values, emits_dependencies, field_name_from_typed_line,
    inspect_binding, inspect_dependency, inspect_subject, is_transition_line, named_block_name,
    named_top_block_name, parse_audit, parse_event_list, qualify_event_ref, query_blocks,
    query_kind, query_name, query_param_names, query_reference_dependencies, resolve_policy_atoms,
    security_markers, strip_quotes, test_group, top_level_blocks, trailing_scalar_value_after,
    transition_name, transition_requires, typed_declaration,
};

