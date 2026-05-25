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

/// Phase L Tier 2 — walk a feature's source lines, find every
/// `@cap.File(...)` site, parse it via the analyzer's typed pass, and
/// project the result. Two site shapes are recognised:
///
/// - `<field>: @cap.File(...)` inside a `resource <Name>` block.
/// - `output @cap.File(...)` inside an `api <name>` block.
///
/// Unparseable shapes are skipped silently so the LSP's existing
/// file-local diagnostics remain the canonical source of shape errors.
fn inspect_storage_projection(lines: &[String]) -> InspectStorage {
    let mut fields: Vec<InspectStorageField> = Vec::new();
    let mut api_outputs: Vec<InspectStorageApiOutput> = Vec::new();

    let mut current_resource: Option<String> = None;
    let mut current_api: Option<String> = None;
    let mut resource_indent: usize = 0;
    let mut api_indent: usize = 0;

    for raw in lines.iter() {
        let trimmed = raw.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = leading_spaces(raw);

        // Detect entering / exiting resource and api blocks. The
        // existing canonical fixture uses 4-space resource headers
        // (inside `domain`) and 2-space api headers; we close the
        // block as soon as the indent retreats to the header level
        // or shallower.
        if let Some(rest) = trimmed.strip_prefix("resource ") {
            current_resource = Some(rest.split_whitespace().next().unwrap_or("").to_owned());
            current_api = None;
            resource_indent = indent;
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("api ") {
            current_api = Some(rest.split_whitespace().next().unwrap_or("").to_owned());
            current_resource = None;
            api_indent = indent;
            continue;
        }
        if current_resource.is_some() && indent <= resource_indent && !trimmed.is_empty() {
            current_resource = None;
        }
        if current_api.is_some() && indent <= api_indent && !trimmed.is_empty() {
            current_api = None;
        }

        // Try a resource-field shape: `<field>: @cap.File(...)`.
        if let Some(resource) = current_resource.as_deref() {
            if let Some((field_name, cap_text)) = extract_cap_file_field(trimmed) {
                if let lazuli_ir::TypeRef::Capability(lazuli_ir::CapabilityRef::File(file)) =
                    lazuli_analyzer::type_ref_from_syntax_public(&cap_text)
                {
                    fields.push(InspectStorageField {
                        resource: resource.to_owned(),
                        field: field_name,
                        file_capability: project_file_capability(&file),
                    });
                }
            }
        }

        // Try an api-output shape: `output @cap.File(...)`.
        if let Some(api) = current_api.as_deref() {
            if let Some(cap_text) = trimmed
                .strip_prefix("output ")
                .map(str::trim)
                .filter(|rest| rest.starts_with("@cap.File("))
            {
                if let lazuli_ir::TypeRef::Capability(lazuli_ir::CapabilityRef::File(file)) =
                    lazuli_analyzer::type_ref_from_syntax_public(cap_text)
                {
                    api_outputs.push(InspectStorageApiOutput {
                        api: api.to_owned(),
                        file_capability: project_file_capability(&file),
                    });
                }
            }
        }
    }

    InspectStorage {
        fields,
        api_outputs,
    }
}

/// Extract `(field_name, "@cap.File(...)")` from a `<field>: @cap.File(...) [required]`
/// resource line. Returns `None` if the line is not a `@cap.File` field.
fn extract_cap_file_field(trimmed: &str) -> Option<(String, String)> {
    let (name_part, type_part) = trimmed.split_once(':')?;
    let name = name_part.trim();
    if name.is_empty() || name.contains(char::is_whitespace) {
        return None;
    }
    // Drop trailing `required` / `optional` / annotation keywords so the
    // analyzer parses the bare type expression.
    let type_tokens = type_part.trim();
    let cap_start = type_tokens.find("@cap.File(")?;
    let from_cap = &type_tokens[cap_start..];
    let close = from_cap.find(')')?;
    let cap_text = &from_cap[..=close];
    Some((name.to_owned(), cap_text.to_owned()))
}

fn project_file_capability(file: &lazuli_ir::FileCapability) -> InspectFileCapability {
    InspectFileCapability {
        max_size: InspectFileSize {
            bytes: file.max_size.bytes,
            literal: format_file_size_literal(file.max_size.literal),
        },
        accept: file
            .accept
            .iter()
            .map(|m| InspectMimeType {
                family: m.family.clone(),
                subtype: m.subtype.clone(),
            })
            .collect(),
        visibility: file
            .visibility
            .map(|v| format_file_visibility(v).to_owned()),
        signed_ttl: file.signed_ttl.clone(),
    }
}

/// Phase L — project a lowered `ir::Auth` into the inspect-shaped
/// `InspectAuth`. Mirrors the IR structure 1:1; the only translation is
/// joining `FieldRef` back into a `<Resource>.<field>` string so the
/// json projection reads exactly like the source surface.
fn project_auth(feature_name: &str, auth: &lazuli_ir::Auth) -> InspectAuth {
    let origin = inspect_origin(feature_name, auth.span_ref);
    InspectAuth {
        origin: origin.clone(),
        identity: InspectAuthIdentity {
            field: format!(
                "{}.{}",
                auth.identity.field.resource.name, auth.identity.field.field
            ),
            resource: auth.identity.field.resource.name.clone(),
            origin: origin.clone(),
        },
        password: auth.password.as_ref().map(|p| InspectAuthPassword {
            algorithm: p.algorithm.clone(),
            hash: p.hash.clone(),
            verify: p.verify.clone(),
            // `ir-rate-limit-env-aware` cell 1 — inspect shim: surface
            // the default literal. Cell 3 extends the projection with
            // the env-qualified shape.
            rate_limit: p.rate_limit.as_ref().map(|spec| spec.default.clone()),
            origin: origin.clone(),
        }),
        sessions: auth.sessions.as_ref().map(|s| InspectAuthSessions {
            resource: s.resource.name.clone(),
            ttl: s.ttl.clone(),
            refresh: s.refresh,
            access_ttl: s.access_ttl.clone(),
            rotation: s.rotation.clone(),
            origin: origin.clone(),
        }),
        mfa: auth.mfa.as_ref().map(|m| InspectAuthMfa {
            method: m.method.clone(),
            enroll: m.enroll.clone(),
            verify: m.verify.clone(),
            adapter: m.adapter.clone(),
            origin: origin.clone(),
        }),
        oauth: auth
            .oauth
            .iter()
            .map(|o| InspectAuthOAuthProvider {
                provider: o.provider.clone(),
                adapter: o.adapter.clone(),
                origin: origin.clone(),
            })
            .collect(),
    }
}

fn inspect_origin(feature_name: &str, span_ref: Option<lazuli_ir::SpanRef>) -> InspectOrigin {
    InspectOrigin {
        feature: feature_name.to_owned(),
        line: span_ref.map(|span| span.start),
    }
}

/// Materialise the per-feature unified HTTP route table for
/// `--expand=expose`. Walks every agent's `expose_http` and every
/// `api <name>` block in the feature body, emitting one entry per
/// declaration with stable `<feature>.<kind>.<name>` origins so
/// cross-feature collation downstream (doctor or external tools)
/// composes cleanly.
fn inspect_expose_projection(
    feature_name: &str,
    agents: &[InspectAgent],
    lines: &[String],
) -> Vec<InspectExposeEntry> {
    let mut entries: Vec<InspectExposeEntry> = Vec::new();

    for agent in agents {
        if let Some(expose) = agent.expose_http.as_ref() {
            entries.push(InspectExposeEntry {
                kind: "agent",
                origin: format!("{feature_name}.agent.{}", agent.name),
                method: expose.method.clone(),
                path: expose.path.clone(),
                route_slots: expose.route_slots.clone(),
                audience: expose.audience.clone(),
                rate_limit_override: expose.rate_limit_override.clone(),
            });
        }
    }

    for block in top_level_blocks(lines, "api ") {
        let name = named_top_block_name(block[0].trim_start())
            .unwrap_or("unknown")
            .to_owned();
        let method = direct_child_value(block, "method ").map(|m| m.to_ascii_uppercase());
        let path = direct_child_value(block, "path ")
            .as_deref()
            .map(strip_quotes);
        let audience = direct_child_value(block, "audience ");
        let rate_limit_override = direct_child_value(block, "rate_limit ")
            .as_deref()
            .map(strip_quotes);
        // Walk `route <name>:` children for slots.
        let mut route_slots: Vec<String> = Vec::new();
        let block_indent = block.first().map(|l| leading_spaces(l)).unwrap_or(0);
        let child_indent = block_indent + 2;
        for inner in block.iter().skip(1) {
            let trimmed = inner.trim_start();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            if leading_spaces(inner) != child_indent {
                continue;
            }
            if let Some(rest) = trimmed.strip_prefix("route ") {
                if let Some((slot, _)) = rest.split_once(':') {
                    route_slots.push(slot.trim().to_owned());
                }
            }
        }

        let (Some(method), Some(path)) = (method, path) else {
            continue;
        };
        entries.push(InspectExposeEntry {
            kind: "api",
            origin: format!("{feature_name}.api.{}", name),
            method,
            path,
            route_slots,
            audience,
            rate_limit_override,
        });
    }

    entries
}

/// Materialise the per-agent dispatch graph for `--expand=tools`.
/// Cross-feature resolution of effects / policies / PII lives in doctor;
/// this projection records the structural facts visible from the file
/// alone (kind, scope, derived effect from the local categorisation).
fn inspect_agent_tools_projection(agents: &[InspectAgent]) -> Vec<InspectAgentToolsEntry> {
    agents
        .iter()
        .filter(|agent| !agent.tools.is_empty())
        .map(|agent| InspectAgentToolsEntry {
            agent: agent.name.clone(),
            tools: agent
                .tools
                .iter()
                .map(|reference| tool_binding_for_reference(reference))
                .collect(),
        })
        .collect()
}

fn tool_binding_for_reference(reference: &str) -> InspectAgentToolBinding {
    let trimmed = reference.trim();
    if trimmed.starts_with("@tool.") {
        return InspectAgentToolBinding {
            reference: trimmed.to_owned(),
            kind: "adapter",
            scope: "adapter",
            derived_effect: "unknown",
        };
    }

    let segments: Vec<&str> = trimmed.split('.').collect();
    let (kind, scope) = match segments.as_slice() {
        ["query", "list", _] => ("query.list", "local"),
        ["query", "lookup", _] => ("query.lookup", "local"),
        ["query", "sql", _] => ("query.sql", "local"),
        ["query", "view", _] => ("query.view", "local"),
        ["query", _] => ("query", "local"),
        ["command", _] => ("command", "local"),
        ["api", _] => ("api", "local"),
        [_feature, "query", "list", _] => ("query.list", "cross_feature"),
        [_feature, "query", "lookup", _] => ("query.lookup", "cross_feature"),
        [_feature, "query", "sql", _] => ("query.sql", "cross_feature"),
        [_feature, "query", "view", _] => ("query.view", "cross_feature"),
        [_feature, "query", _] => ("query", "cross_feature"),
        [_feature, "command", _] => ("command", "cross_feature"),
        [_feature, "api", _] => ("api", "cross_feature"),
        _ => ("unknown", "unknown"),
    };

    let derived_effect = match kind {
        "command" => "write",
        "query.list" | "query.lookup" | "query.sql" | "query.view" | "query" => "read",
        _ => "unknown",
    };

    InspectAgentToolBinding {
        reference: trimmed.to_owned(),
        kind,
        scope,
        derived_effect,
    }
}

fn inspect_requirements(lines: &[String]) -> Vec<InspectRequirement> {
    let mut requirements = Vec::new();
    let mut in_requires_block = false;

    for line in lines {
        let trimmed = line.trim_start();
        let leading = leading_spaces(line);

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if leading == 2 {
            in_requires_block = trimmed == "requires";
            if let Some(requirement) = trimmed.strip_prefix("requires ") {
                if let Some(parsed) = parse_inspect_requirement(requirement, "requires inline") {
                    requirements.push(parsed);
                }
            }
            continue;
        }

        if leading <= 2 {
            in_requires_block = false;
        }

        if in_requires_block && leading == 4 {
            if let Some(parsed) = parse_inspect_requirement(trimmed, "requires block") {
                requirements.push(parsed);
            }
        }
    }

    requirements
}

fn inspect_external_calls(feature: &str, lines: &[String]) -> Vec<InspectExternalCall> {
    let mut calls = Vec::new();
    let mut index = 0;

    while index < lines.len() {
        let trimmed = lines[index].trim_start();
        let leading = leading_spaces(&lines[index]);

        if leading == 2 && (trimmed.starts_with("command ") || trimmed.starts_with("job ")) {
            let (kind, name) = if let Some(name) = named_block_name(trimmed, "command") {
                ("command", name)
            } else if let Some(name) = named_block_name(trimmed, "job") {
                ("job", name)
            } else {
                index += 1;
                continue;
            };

            let start = index;
            index += 1;
            while index < lines.len() && leading_spaces(&lines[index]) > 2 {
                index += 1;
            }

            calls.extend(inspect_external_calls_in_block(
                feature,
                kind,
                name,
                &lines[start..index],
            ));
        } else {
            index += 1;
        }
    }

    calls
}

fn inspect_external_calls_in_block(
    feature: &str,
    kind: &str,
    name: &str,
    lines: &[String],
) -> Vec<InspectExternalCall> {
    let timeout = block_scalar_value(lines, "timeout").map(strip_quotes);
    let retry = block_scalar_value(lines, "retry").map(str::to_owned);
    let idempotency = block_prefixed_value(lines, "idempotency by ").map(str::to_owned);
    let audit = block_has_exact_line(lines, "audit required");
    let subject = format!("{feature}.{kind}.{name}");
    let mut calls = Vec::new();
    let mut index = 1;

    while index < lines.len() {
        let trimmed = lines[index].trim_start();

        if leading_spaces(&lines[index]) == 4
            && let Some((slot, operation)) = parse_external_call_header(trimmed)
        {
            let mut args = Vec::new();
            index += 1;

            while index < lines.len() && leading_spaces(&lines[index]) > 4 {
                let child = lines[index].trim_start();
                if leading_spaces(&lines[index]) == 6
                    && let Some((name, value)) = child.split_once('=')
                {
                    args.push(InspectCallArg {
                        name: name.trim().to_owned(),
                        value: value.trim().to_owned(),
                    });
                }
                index += 1;
            }

            calls.push(InspectExternalCall {
                subject: subject.clone(),
                slot: slot.to_owned(),
                operation: operation.to_owned(),
                args,
                timeout: timeout.clone(),
                retry: retry.clone(),
                idempotency: idempotency.clone(),
                audit,
                origin: "calls",
            });
        } else {
            index += 1;
        }
    }

    calls
}

fn parse_external_call_header(trimmed: &str) -> Option<(&str, &str)> {
    let rest = trimmed.strip_prefix("calls ")?;
    let (slot, operation) = rest.trim().split_once('.')?;
    let slot = slot.trim();
    let operation = operation.trim();

    if is_identifier(slot) && is_identifier(operation) {
        Some((slot, operation))
    } else {
        None
    }
}

fn named_block_name<'a>(trimmed: &'a str, keyword: &str) -> Option<&'a str> {
    let rest = trimmed.strip_prefix(keyword)?.trim_start();
    let name = rest.split_whitespace().next()?;
    is_identifier(name).then_some(name)
}

fn block_scalar_value<'a>(lines: &'a [String], keyword: &str) -> Option<&'a str> {
    lines.iter().skip(1).find_map(|line| {
        (leading_spaces(line) == 4)
            .then(|| line.trim_start().strip_prefix(keyword))
            .flatten()
            .map(str::trim)
            .filter(|value| !value.is_empty())
    })
}

fn block_prefixed_value<'a>(lines: &'a [String], prefix: &str) -> Option<&'a str> {
    lines.iter().skip(1).find_map(|line| {
        (leading_spaces(line) == 4)
            .then(|| line.trim_start().strip_prefix(prefix))
            .flatten()
            .map(str::trim)
            .filter(|value| !value.is_empty())
    })
}

fn block_has_exact_line(lines: &[String], expected: &str) -> bool {
    lines
        .iter()
        .skip(1)
        .any(|line| leading_spaces(line) == 4 && line.trim_start() == expected)
}

fn strip_quotes(value: &str) -> String {
    value
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or(value)
        .to_owned()
}

fn parse_inspect_requirement(source: &str, origin: &'static str) -> Option<InspectRequirement> {
    let rest = source.trim().strip_prefix("integration ")?;
    let (name, contract) = rest.split_once(':')?;
    let name = name.trim();
    let contract = contract.trim();

    if !is_identifier(name) || !is_type_name(contract) {
        return None;
    }

    Some(InspectRequirement {
        kind: "integration".to_owned(),
        name: name.to_owned(),
        contract: contract.to_owned(),
        origin,
    })
}

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

fn collect_declared_ref_groups(lines: &[String]) -> Vec<InspectRefGroup> {
    let mut groups = Vec::new();
    let mut in_refs = false;

    for line in lines {
        let trimmed = line.trim_start();

        if leading_spaces(line) == 2 {
            in_refs = trimmed == "refs";
            continue;
        }

        if !in_refs || leading_spaces(line) != 4 || trimmed.is_empty() {
            continue;
        }

        let Some((group, namespaces)) = trimmed.split_once(':') else {
            continue;
        };

        groups.push(InspectRefGroup {
            group: group.trim().to_owned(),
            namespaces: namespaces
                .split(',')
                .map(str::trim)
                .filter(|namespace| namespace.starts_with('@') && !namespace.is_empty())
                .map(str::to_owned)
                .collect(),
            origin: "authored",
        });
    }

    groups
}

fn collect_used_namespaces(lines: &[String]) -> BTreeSet<String> {
    let mut namespaces = BTreeSet::new();
    let mut current_top = None;

    for line in lines {
        let trimmed = line.trim_start();

        if leading_spaces(line) == 2 {
            current_top = trimmed.split_whitespace().next();
        }

        if current_top == Some("refs") || trimmed.starts_with('#') {
            continue;
        }

        for namespace in namespace_references(line) {
            namespaces.insert(format!("@{namespace}"));
        }
    }

    namespaces
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

fn inspect_locators(lines: &[String]) -> Vec<InspectLocators> {
    let mut locators = Vec::new();
    let has_id_lookup = feature_has_id_lookup(lines);

    for block in query_blocks(lines) {
        let name = query_name(block[0].trim_start()).unwrap_or("unknown");
        let inferred = query_kind(block);
        let mut bindings = vec![inspect_binding(
            "ctx.*",
            "runtime",
            "request and tenant execution context",
        )];

        for param in query_param_names(block) {
            bindings.push(inspect_binding(
                format!("params.{param}"),
                "query.params",
                "read argument declared by this query",
            ));
        }

        locators.push(InspectLocators {
            subject: format!("query.{name}"),
            kind: format!("query.{inferred}"),
            bindings,
        });
    }

    for block in command_blocks(lines) {
        let name = command_name(block[0].trim_start()).unwrap_or("unknown");
        let mut bindings = vec![inspect_binding(
            "ctx.*",
            "runtime",
            "request and tenant execution context",
        )];

        for route in command_route_names(block) {
            bindings.push(inspect_binding(
                format!("route.{route}"),
                "command.route",
                "path or caller-context locator declared by this command",
            ));
        }

        for input in command_input_names(block) {
            bindings.push(inspect_binding(
                format!("input.{input}"),
                "command.input",
                "submitted command body field",
            ));
        }

        if let Some(target) = direct_child_value(block, "target ") {
            bindings.push(inspect_binding(
                "target",
                format!("explicit target {target}"),
                "entity loaded before declarative command effects",
            ));
        } else if has_id_lookup && command_needs_inferred_target(block) {
            bindings.push(inspect_binding(
                "target",
                "inferred local query.by_id(id: route.id)",
                "entity loaded before declarative command effects",
            ));
        }

        locators.push(InspectLocators {
            subject: format!("command.{name}"),
            kind: "command".to_owned(),
            bindings,
        });
    }

    for block in top_level_blocks(lines, "job ") {
        let name = named_top_block_name(block[0].trim_start()).unwrap_or("unknown");
        let trigger = direct_child_value(block, "trigger ");
        let mut bindings = vec![inspect_binding("ctx.*", "runtime", "job execution context")];
        let kind = if trigger
            .as_deref()
            .is_some_and(|trigger| trigger.starts_with("event "))
        {
            bindings.push(inspect_binding(
                "envelope.*",
                "event trigger",
                "event-bus metadata such as envelope.id",
            ));
            bindings.push(inspect_binding(
                "payload.*",
                "event trigger",
                "producer event payload fields",
            ));
            "event_job"
        } else if trigger
            .as_deref()
            .is_some_and(|trigger| trigger.starts_with("schedule "))
        {
            bindings.push(inspect_binding(
                "schedule.*",
                "schedule trigger",
                "scheduler metadata such as run time",
            ));
            "schedule_job"
        } else {
            "job"
        };

        if let Some(target) = direct_child_value(block, "target ") {
            bindings.push(inspect_binding(
                "target",
                format!("explicit target {target}"),
                "entity loaded before declarative job effects",
            ));
        }

        locators.push(InspectLocators {
            subject: format!("job.{name}"),
            kind: kind.to_owned(),
            bindings,
        });
    }

    for block in top_level_blocks(lines, "webhook ") {
        let name = named_top_block_name(block[0].trim_start()).unwrap_or("unknown");
        locators.push(InspectLocators {
            subject: format!("webhook.{name}"),
            kind: "webhook".to_owned(),
            bindings: vec![
                inspect_binding(
                    "payload.*",
                    "webhook payload",
                    "verified inbound request body fields",
                ),
                inspect_binding("ctx.*", "runtime", "webhook execution context"),
            ],
        });
    }

    for block in top_level_blocks(lines, "rule ") {
        let name = block[0]
            .trim_start()
            .trim_start_matches("rule ")
            .trim_matches('"');
        locators.push(InspectLocators {
            subject: format!("rule.{name}"),
            kind: "rule".to_owned(),
            bindings: vec![
                inspect_binding(
                    "self",
                    "rule target snapshot",
                    "resource snapshot evaluated by the rule predicate",
                ),
                inspect_binding("ctx.*", "runtime", "request and tenant execution context"),
            ],
        });
    }

    locators
}

fn inspect_dependencies(lines: &[String]) -> Vec<InspectDependency> {
    let feature = lines
        .first()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("unknown");
    let mut dependencies = Vec::new();

    for line in lines {
        let trimmed = line.trim_start();
        if leading_spaces(line) == 2 && trimmed.starts_with("uses ") {
            for target in parse_ident_list(trimmed.trim_start_matches("uses ")) {
                dependencies.push(inspect_dependency("uses", feature, target, "uses"));
            }
        } else if leading_spaces(line) == 2 && trimmed.starts_with("extends @anchor.") {
            if let Some(anchor) = trimmed.split_whitespace().nth(1) {
                dependencies.push(inspect_dependency(
                    "extends_anchor",
                    feature,
                    anchor,
                    "extends",
                ));
            }
        }
    }

    for block in command_blocks(lines) {
        let name = command_name(block[0].trim_start()).unwrap_or("unknown");
        let subject = format!("{feature}.command.{name}");
        dependencies.extend(emits_dependencies(feature, &subject, block));
        dependencies.extend(query_reference_dependencies(&subject, block));
    }

    for block in top_level_blocks(lines, "workflow ") {
        let name = named_top_block_name(block[0].trim_start()).unwrap_or("unknown");
        let subject = format!("{feature}.workflow.{name}");
        dependencies.extend(emits_dependencies(feature, &subject, block));
    }

    for block in top_level_blocks(lines, "job ") {
        let name = named_top_block_name(block[0].trim_start()).unwrap_or("unknown");
        let subject = format!("{feature}.job.{name}");
        if let Some(trigger) = direct_child_value(block, "trigger ") {
            if let Some(event) = trigger.strip_prefix("event ") {
                dependencies.push(inspect_dependency(
                    "trigger_event",
                    subject.clone(),
                    qualify_event_ref(feature, event.trim()),
                    "job.trigger",
                ));
            }
        }
        dependencies.extend(emits_dependencies(feature, &subject, block));
        dependencies.extend(query_reference_dependencies(&subject, block));
    }

    for block in top_level_blocks(lines, "webhook ") {
        let name = named_top_block_name(block[0].trim_start()).unwrap_or("unknown");
        let subject = format!("{feature}.webhook.{name}");
        dependencies.extend(emits_dependencies(feature, &subject, block));
    }

    dependencies
}

mod security;

use security::inspect_security;

fn inspect_binding(
    name: impl Into<String>,
    origin: impl Into<String>,
    meaning: impl Into<String>,
) -> InspectBinding {
    InspectBinding {
        name: name.into(),
        origin: origin.into(),
        meaning: meaning.into(),
    }
}

fn inspect_dependency(
    kind: impl Into<String>,
    from: impl Into<String>,
    to: impl Into<String>,
    origin: impl Into<String>,
) -> InspectDependency {
    InspectDependency {
        kind: kind.into(),
        from: from.into(),
        to: to.into(),
        origin: origin.into(),
    }
}

/// Phase L Tier 4a — `--expand=defaults` projection. Reads the lifted
/// `Feature.defaults` block from the Tier 3 slice when available
/// (canonical-indent code path), and falls back to the text-pattern
/// walker only for legacy documents that did not lower through
/// `parse_feature_skeletons`. The query-derived language defaults
/// (`query_order`, `query_filter_index`) stay text-derived because
/// those facts originate from CLI heuristics over query bodies, not
/// from feature-state IR.
fn inspect_defaults(
    lines: &[String],
    tier3: Option<&Tier3FeatureSlice>,
) -> Vec<InspectDefault> {
    let mut defaults = match tier3 {
        Some(slice) => project_defaults_from_ir(slice, lines),
        None => inspect_defaults_legacy(lines),
    };

    for query in query_blocks(lines) {
        let header = query[0].trim_start();
        if !header.starts_with("query.list ") {
            continue;
        }
        if direct_child_value(query, "order ").is_some() {
            continue;
        }
        let name = query_name(header).unwrap_or("unknown");
        defaults.push(InspectDefault {
            name: "query_order".to_owned(),
            value: "created_at desc".to_owned(),
            origin: "language default",
            applies_to: vec![format!("query.{name}")],
        });
    }

    for generated in collect_query_filter_indexes(lines) {
        defaults.push(InspectDefault {
            name: "query_filter_index".to_owned(),
            value: generated.value,
            origin: "language default",
            applies_to: vec![
                format!("query.{}", generated.query),
                format!("filter.{}", generated.filter),
            ],
        });
    }

    defaults
}

/// Phase L Tier 4a — project `Feature.defaults` from IR into the
/// `InspectDefault` shape. `applies_to` for `tenancy`/`timestamps`
/// reads `Tier3FeatureSlice.resource_names`; for `policy`, it
/// retains the text walker over jobs/webhooks until those names are
/// also lifted to the slice (Tier 4 follow-up).
fn project_defaults_from_ir(
    slice: &Tier3FeatureSlice,
    lines: &[String],
) -> Vec<InspectDefault> {
    let mut out = Vec::new();

    if slice.defaults.timestamps {
        out.push(InspectDefault {
            name: "timestamps".to_owned(),
            value: "true".to_owned(),
            origin: "defaults",
            applies_to: slice.resource_names.clone(),
        });
    }

    if let Some(tenancy) = &slice.defaults.tenancy {
        let value = match tenancy {
            lazuli_ir::Tenancy::Org => "org".to_owned(),
            lazuli_ir::Tenancy::Team => "team".to_owned(),
            lazuli_ir::Tenancy::Custom(name) => name.clone(),
            lazuli_ir::Tenancy::None => "none".to_owned(),
        };
        out.push(InspectDefault {
            name: "tenancy".to_owned(),
            value,
            origin: "defaults",
            applies_to: slice.resource_names.clone(),
        });
    }

    // `policy` and `policy_for` retain text-derived `applies_to` until
    // jobs/webhooks have their names lifted to the slice. The IR
    // `Defaults.policy` carries the typed atom; the `applies_to`
    // projection mirrors the legacy text walker for now to keep the
    // projection JSON shape stable.
    let mut in_defaults = false;
    for line in lines {
        let trimmed = line.trim_start();
        if leading_spaces(line) == 2 {
            in_defaults = trimmed == "defaults";
            continue;
        }
        if !in_defaults || leading_spaces(line) != 4 || trimmed.is_empty() {
            continue;
        }
        if let Some(value) = trimmed.strip_prefix("policy_for ") {
            if let Some((scopes, policy)) = value.split_once(':') {
                out.push(InspectDefault {
                    name: "policy_for".to_owned(),
                    value: policy.trim().to_owned(),
                    origin: "defaults",
                    applies_to: collect_policy_for_applies_to(lines, scopes),
                });
            }
        } else if let Some(value) = trimmed.strip_prefix("policy ") {
            out.push(InspectDefault {
                name: "policy".to_owned(),
                value: value.to_owned(),
                origin: "defaults",
                applies_to: collect_job_and_webhook_names(lines),
            });
        }
    }

    out
}

/// Legacy text-pattern fallback. Retained for documents that don't
/// lower through `parse_feature_skeletons` (no Tier 3 slice). The
/// canonical-indent code path routes through `project_defaults_from_ir`.
fn inspect_defaults_legacy(lines: &[String]) -> Vec<InspectDefault> {
    let resources = collect_resource_names(lines);
    let mut defaults = Vec::new();
    let mut in_defaults = false;

    for line in lines {
        let trimmed = line.trim_start();

        if leading_spaces(line) == 2 {
            in_defaults = trimmed == "defaults";
            continue;
        }

        if !in_defaults || leading_spaces(line) != 4 || trimmed.is_empty() {
            continue;
        }

        if trimmed == "timestamps" {
            defaults.push(InspectDefault {
                name: "timestamps".to_owned(),
                value: "true".to_owned(),
                origin: "defaults",
                applies_to: resources.clone(),
            });
        } else if let Some(value) = trimmed.strip_prefix("tenancy ") {
            defaults.push(InspectDefault {
                name: "tenancy".to_owned(),
                value: value.to_owned(),
                origin: "defaults",
                applies_to: resources.clone(),
            });
        } else if let Some(value) = trimmed.strip_prefix("policy_for ") {
            if let Some((scopes, policy)) = value.split_once(':') {
                defaults.push(InspectDefault {
                    name: "policy_for".to_owned(),
                    value: policy.trim().to_owned(),
                    origin: "defaults",
                    applies_to: collect_policy_for_applies_to(lines, scopes),
                });
            }
        } else if let Some(value) = trimmed.strip_prefix("policy ") {
            defaults.push(InspectDefault {
                name: "policy".to_owned(),
                value: value.to_owned(),
                origin: "defaults",
                applies_to: collect_job_and_webhook_names(lines),
            });
        }
    }

    defaults
}

struct GeneratedFilterIndex {
    query: String,
    filter: String,
    value: String,
}

fn collect_query_filter_indexes(lines: &[String]) -> Vec<GeneratedFilterIndex> {
    let tenancy_axis = single_tenancy_axis(lines);
    let mut seen = BTreeSet::new();
    let mut indexes = Vec::new();

    for query in query_blocks(lines) {
        let header = query[0].trim_start();
        if !header.starts_with("query.list ") || query_has_scope_override(query) {
            continue;
        }
        let name = query_name(header).unwrap_or("unknown");

        for field in query_filter_index_fields(query) {
            let value = tenancy_axis
                .as_ref()
                .map(|tenant| format!("{tenant}, {field}"))
                .unwrap_or_else(|| field.clone());

            if seen.insert(value.clone()) {
                indexes.push(GeneratedFilterIndex {
                    query: name.to_owned(),
                    filter: field,
                    value,
                });
            }
        }
    }

    indexes
}

fn single_tenancy_axis(lines: &[String]) -> Option<String> {
    let axes: BTreeSet<String> = lines
        .iter()
        .filter_map(|line| {
            let trimmed = line.trim_start();
            let axis = trimmed.strip_prefix("tenancy ")?.trim();
            (!axis.is_empty() && axis != "none").then(|| axis.to_owned())
        })
        .collect();

    (axes.len() == 1).then(|| axes.into_iter().next()).flatten()
}

fn query_has_scope_override(query: &[String]) -> bool {
    query
        .iter()
        .any(|line| line.trim_start() == "scope override")
}

fn query_filter_index_fields(query: &[String]) -> Vec<String> {
    let mut fields = Vec::new();
    let mut in_filters = false;

    for line in query.iter().skip(1) {
        let trimmed = line.trim_start();
        let leading = leading_spaces(line);

        if trimmed.is_empty() {
            continue;
        }

        if leading == 6 {
            in_filters = trimmed == "filters";
            continue;
        }

        if in_filters
            && leading == 8
            && let Some(field) = filter_index_field(trimmed)
        {
            fields.push(field);
        }
    }

    fields
}

fn filter_index_field(filter: &str) -> Option<String> {
    if filter.contains(" has ")
        || filter.contains(" != ")
        || filter.contains(" = nil")
        || filter.contains(" != nil")
    {
        return None;
    }

    if let Some((field, param)) = filter.split_once(" when ") {
        let field = field.trim();
        let param = param.trim().strip_prefix("params.")?;
        if is_identifier(field) && field == param {
            return Some(field.to_owned());
        }
        return None;
    }

    if let Some((left, right)) = filter.split_once(" = ") {
        let left = left.trim();
        let param = right.trim().strip_prefix("params.")?;

        if is_identifier(left) && left == param {
            return Some(left.to_owned());
        }

        if let Some(relation) = left.strip_suffix(".id")
            && is_identifier(relation)
            && param == format!("{relation}_id")
        {
            return Some(relation.to_owned());
        }
    }

    None
}

fn collect_policy_for_applies_to(lines: &[String], scopes: &str) -> Vec<String> {
    let mut applies_to = Vec::new();

    for scope in parse_ident_list(scopes) {
        match scope.as_str() {
            "jobs" => applies_to.extend(collect_named_top_blocks(lines, "job ")),
            "webhooks" => applies_to.extend(collect_named_top_blocks(lines, "webhook ")),
            _ => {}
        }
    }

    applies_to
}

fn inspect_built_in_trace_events() -> Vec<InspectBuiltInTraceEvent> {
    lazuli_ir::built_in_trace_events()
        .into_iter()
        .map(|event| InspectBuiltInTraceEvent {
            name: event.name,
            fires_per: built_in_trace_fires_per_word(event.fires_per).to_owned(),
            payload: event
                .payload
                .into_iter()
                .map(|f| InspectBuiltInTraceField {
                    name: f.name,
                    type_text: format_type_ref(&f.type_ref),
                    optional: f.optional,
                })
                .collect(),
        })
        .collect()
}

fn built_in_trace_fires_per_word(kind: lazuli_ir::TraceFiresPer) -> &'static str {
    match kind {
        lazuli_ir::TraceFiresPer::AgentDispatch => "agent_dispatch",
        lazuli_ir::TraceFiresPer::CommandDispatch => "command_dispatch",
        lazuli_ir::TraceFiresPer::FlowStep => "flow_step",
        lazuli_ir::TraceFiresPer::JobInvocation => "job_invocation",
        lazuli_ir::TraceFiresPer::WebhookDelivery => "webhook_delivery",
    }
}


fn inspect_events(lines: &[String]) -> Vec<InspectEvent> {
    let event_groups = collect_event_groups(lines);
    collect_event_decls(lines)
        .into_iter()
        .map(|event| {
            let mut payload = Vec::new();
            for group in &event_groups {
                if event.name.starts_with(&group.prefix) {
                    for entry in &group.payload {
                        payload.push(inspect_inherited_payload_field(
                            entry,
                            format!("event_group:{}", group.pattern),
                        ));
                    }
                }
            }

            for field in &event.payload {
                if let Some(field) = inspect_explicit_payload_field(field, &event.name) {
                    payload.push(field);
                }
            }

            InspectEvent {
                name: event.name,
                payload,
            }
        })
        .collect()
}

fn inspect_targets(lines: &[String]) -> Vec<InspectTarget> {
    let mut targets = Vec::new();
    let has_id_lookup = feature_has_id_lookup(lines);

    for command in command_blocks(lines) {
        let name = command_name(command[0].trim_start()).unwrap_or("unknown");
        let explicit = command.iter().find_map(|line| {
            if leading_spaces(line) == 4 {
                line.trim_start().strip_prefix("target ").map(str::to_owned)
            } else {
                None
            }
        });

        if let Some(target) = explicit {
            targets.push(InspectTarget {
                command: name.to_owned(),
                target,
                origin: "explicit".to_owned(),
            });
        } else if has_id_lookup && command_needs_inferred_target(command) {
            targets.push(InspectTarget {
                command: name.to_owned(),
                target: "query.by_id(id: route.id)".to_owned(),
                origin: "inferred from local route id and query.lookup by_id".to_owned(),
            });
        }
    }

    targets
}

fn inspect_policies(
    lines: &[String],
    policy_atoms: &BTreeMap<String, Vec<String>>,
    tier3: Option<&Tier3FeatureSlice>,
) -> Vec<InspectPolicy> {
    let mut policies = Vec::new();

    // IR Error-Vocab (Cell PARSE-1) — build name -> when_denied
    // lookups so the text walker can attach the per-command override
    // (resolution-chain step 1) and the per-policy default
    // (resolution-chain step 2) onto each InspectPolicy row.
    let command_when_denied: BTreeMap<String, String> = tier3
        .map(|t| {
            t.commands
                .iter()
                .filter_map(|c| {
                    c.policy_when_denied
                        .as_ref()
                        .map(|k| (c.name.clone(), k.key.clone()))
                })
                .collect()
        })
        .unwrap_or_default();
    let policy_when_denied: BTreeMap<String, String> = tier3
        .map(|t| {
            t.policies
                .categories
                .iter()
                .filter_map(|cat| {
                    cat.when_denied
                        .as_ref()
                        .map(|k| (cat.name.clone(), k.key.clone()))
                })
                .collect()
        })
        .unwrap_or_default();

    // Helper — resolve the effective `when_denied` for a given policy
    // string. Walks the resolution chain: prefer the per-command
    // override (caller-supplied) over the per-policy category default.
    let resolve_when_denied = |policy_text: &str, override_key: Option<&str>| -> Option<String> {
        if let Some(k) = override_key {
            return Some(k.to_owned());
        }
        // The `policy_text` carries `@policy.<name>` for named
        // categories; strip the prefix to look up the category default.
        if let Some(name) = policy_text
            .trim()
            .strip_prefix("@policy.")
            .map(|s| s.split_whitespace().next().unwrap_or(""))
        {
            return policy_when_denied.get(name).cloned();
        }
        None
    };

    for command in command_blocks(lines) {
        let name = command_name(command[0].trim_start()).unwrap_or("unknown");

        if let Some(policy) = direct_child_value(command, "policy ") {
            let override_key = command_when_denied.get(name).map(String::as_str);
            let when_denied = resolve_when_denied(&policy, override_key);
            policies.push(InspectPolicy {
                subject: format!("command.{name}"),
                atoms: resolve_policy_atoms(&policy, policy_atoms),
                policy,
                origin: "explicit".to_owned(),
                requires: Vec::new(),
                when_denied,
            });
        }
    }

    for query in query_blocks(lines) {
        let name = query_name(query[0].trim_start()).unwrap_or("unknown");

        if let Some(policy) = direct_child_value(query, "policy ") {
            let when_denied = resolve_when_denied(&policy, None);
            policies.push(InspectPolicy {
                subject: format!("query.{name}"),
                atoms: resolve_policy_atoms(&policy, policy_atoms),
                policy,
                origin: "explicit".to_owned(),
                requires: Vec::new(),
                when_denied,
            });
        }
    }

    let mut workflow_name = None;
    let mut workflow_policy = None;

    for line in lines {
        let trimmed = line.trim_start();

        if trimmed.is_empty() {
            continue;
        }

        if leading_spaces(line) == 2 && trimmed.starts_with("workflow ") {
            workflow_name = trimmed.split_whitespace().nth(1).map(str::to_owned);
            workflow_policy = None;
        } else if leading_spaces(line) == 4 && workflow_name.is_some() {
            if let Some(policy) = trimmed.strip_prefix("policy ") {
                workflow_policy = Some(policy.to_owned());
            } else if is_transition_line(trimmed) {
                let transition = transition_name(trimmed).unwrap_or("unknown");
                let policy = workflow_policy.clone().unwrap_or_else(|| "none".to_owned());
                let mut requires = Vec::new();

                if let Some(required) = transition_requires(trimmed) {
                    requires.push(InspectPolicyRequirement {
                        atoms: resolve_policy_atoms(&required, policy_atoms),
                        policy: required,
                        origin: "transition.requires".to_owned(),
                    });
                }

                let when_denied = resolve_when_denied(&policy, None);
                policies.push(InspectPolicy {
                    subject: format!(
                        "workflow.{}.{}",
                        workflow_name.as_deref().unwrap_or("unknown"),
                        transition
                    ),
                    atoms: resolve_policy_atoms(&policy, policy_atoms),
                    policy,
                    origin: "workflow.policy".to_owned(),
                    requires,
                    when_denied,
                });
            }
        } else if leading_spaces(line) <= 2 {
            workflow_name = None;
            workflow_policy = None;
        }
    }

    policies
}

fn inspect_tests(
    lines: &[String],
    policy_atoms: &BTreeMap<String, Vec<String>>,
) -> Vec<InspectTests> {
    let mut tests = Vec::new();
    let mut subject_stack: Vec<(usize, String)> = Vec::new();
    let mut index = 0;

    for command in command_blocks(lines) {
        let name = command_name(command[0].trim_start()).unwrap_or("unknown");
        let Some(policy) = direct_child_value(command, "policy ") else {
            continue;
        };
        let atoms = resolve_policy_atoms(&policy, policy_atoms);
        if atoms.is_empty() {
            continue;
        }
        let subject = format!("command.{name}");
        push_inspect_test_assertion(
            &mut tests,
            &subject,
            "authz",
            format!("permits {}", atoms.join(", ")),
            format!("generated from command policy {policy}"),
        );
        push_inspect_test_assertion(
            &mut tests,
            &subject,
            "authz",
            format!("forbids actors outside {policy}"),
            format!("generated from closed-world command policy {policy}"),
        );
    }

    while index < lines.len() {
        let line = &lines[index];
        let trimmed = line.trim_start();
        let leading = leading_spaces(line);

        if trimmed.is_empty() {
            index += 1;
            continue;
        }

        while subject_stack
            .last()
            .is_some_and(|(indent, _)| *indent >= leading)
        {
            subject_stack.pop();
        }

        if let Some(subject) = inspect_subject(trimmed) {
            subject_stack.push((leading, subject));
        }

        if trimmed == "tests" {
            let subject = subject_stack
                .last()
                .map(|(_, subject)| subject.clone())
                .unwrap_or_else(|| "unknown".to_owned());
            let mut groups: BTreeMap<String, Vec<InspectTestAssertion>> = BTreeMap::new();
            let mut child_index = index + 1;

            while child_index < lines.len() && leading_spaces(&lines[child_index]) > leading {
                let assertion = lines[child_index].trim_start();
                if !assertion.is_empty() {
                    groups
                        .entry(test_group(assertion).to_owned())
                        .or_default()
                        .push(InspectTestAssertion {
                            assertion: assertion.to_owned(),
                            origin: "authored".to_owned(),
                        });
                }
                child_index += 1;
            }

            merge_inspect_tests(&mut tests, InspectTests { subject, groups });
            index = child_index;
            continue;
        }

        index += 1;
    }

    tests
}

fn push_inspect_test_assertion(
    tests: &mut Vec<InspectTests>,
    subject: &str,
    group: &str,
    assertion: String,
    origin: String,
) {
    let Some(existing) = tests.iter_mut().find(|entry| entry.subject == subject) else {
        tests.push(InspectTests {
            subject: subject.to_owned(),
            groups: BTreeMap::from([(
                group.to_owned(),
                vec![InspectTestAssertion { assertion, origin }],
            )]),
        });
        return;
    };

    existing
        .groups
        .entry(group.to_owned())
        .or_default()
        .push(InspectTestAssertion { assertion, origin });
}

fn merge_inspect_tests(tests: &mut Vec<InspectTests>, incoming: InspectTests) {
    let Some(existing) = tests
        .iter_mut()
        .find(|entry| entry.subject == incoming.subject)
    else {
        tests.push(incoming);
        return;
    };

    for (group, assertions) in incoming.groups {
        existing.groups.entry(group).or_default().extend(assertions);
    }
}

fn collect_resource_names(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .filter_map(|line| {
            let trimmed = line.trim_start();
            if leading_spaces(line) == 4 && trimmed.starts_with("resource ") {
                trimmed.split_whitespace().nth(1).map(str::to_owned)
            } else {
                None
            }
        })
        .collect()
}

fn collect_record_names(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .filter_map(|line| {
            let trimmed = line.trim_start();
            if leading_spaces(line) == 4 && trimmed.starts_with("record ") {
                trimmed.split_whitespace().nth(1).map(str::to_owned)
            } else {
                None
            }
        })
        .collect()
}

fn collect_query_names(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .filter_map(|line| {
            let trimmed = line.trim_start();
            if leading_spaces(line) == 4 && trimmed.starts_with("query.") {
                trimmed.split_whitespace().nth(1).map(str::to_owned)
            } else {
                None
            }
        })
        .collect()
}

fn collect_command_names(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .filter_map(|line| {
            let trimmed = line.trim_start();
            if leading_spaces(line) == 2 && trimmed.starts_with("command ") {
                command_name(trimmed).map(str::to_owned)
            } else {
                None
            }
        })
        .collect()
}

fn collect_workflow_summaries(lines: &[String]) -> Vec<InspectWorkflowSummary> {
    let mut workflows = Vec::new();
    let mut current: Option<InspectWorkflowSummary> = None;

    for line in lines {
        let trimmed = line.trim_start();

        if leading_spaces(line) == 2 {
            if let Some(workflow) = current.take() {
                workflows.push(workflow);
            }

            current = if trimmed.starts_with("workflow ") {
                trimmed
                    .split_whitespace()
                    .nth(1)
                    .map(|name| InspectWorkflowSummary {
                        name: name.to_owned(),
                        transitions: Vec::new(),
                    })
            } else {
                None
            };
            continue;
        }

        if leading_spaces(line) == 4 && is_transition_line(trimmed) {
            if let Some(workflow) = current.as_mut() {
                if let Some(transition) = transition_name(trimmed) {
                    workflow.transitions.push(transition.to_owned());
                }
            }
        }
    }

    if let Some(workflow) = current {
        workflows.push(workflow);
    }

    workflows
}

fn collect_named_top_blocks(lines: &[String], keyword: &str) -> Vec<String> {
    lines
        .iter()
        .filter_map(|line| {
            let trimmed = line.trim_start();
            if leading_spaces(line) == 2 && trimmed.starts_with(keyword) {
                trimmed.split_whitespace().nth(1).map(str::to_owned)
            } else {
                None
            }
        })
        .collect()
}

fn collect_event_names(lines: &[String]) -> Vec<String> {
    collect_event_decls(lines)
        .into_iter()
        .map(|event| event.name)
        .collect()
}

fn collect_surface_names(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .filter_map(|line| {
            let trimmed = line.trim_start();
            if leading_spaces(line) == 2 && trimmed.starts_with("surface ") {
                let parts: Vec<_> = trimmed.split_whitespace().skip(1).collect();
                (!parts.is_empty()).then(|| parts.join("/"))
            } else {
                None
            }
        })
        .collect()
}

fn collect_view_anchors(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .filter_map(|line| {
            let trimmed = line.trim_start();
            let (_, anchor) = trimmed.split_once(" id @anchor.")?;
            let name = anchor.split_whitespace().next()?;
            Some(format!("@anchor.{name}"))
        })
        .collect()
}

fn collect_extends_anchors(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .filter_map(|line| {
            let trimmed = line.trim_start();
            if leading_spaces(line) == 2 && trimmed.starts_with("extends @anchor.") {
                trimmed.split_whitespace().nth(1).map(str::to_owned)
            } else {
                None
            }
        })
        .collect()
}

fn collect_extensible_by_features(lines: &[String]) -> Vec<String> {
    let mut features = Vec::new();

    for line in lines {
        let trimmed = line.trim_start();
        if leading_spaces(line) == 6 && trimmed.starts_with("extensible_by ") {
            features.extend(
                trimmed
                    .trim_start_matches("extensible_by ")
                    .split(',')
                    .map(str::trim)
                    .filter(|feature| !feature.is_empty())
                    .map(str::to_owned),
            );
        }
    }

    features
}

fn collect_job_and_webhook_names(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .filter_map(|line| {
            let trimmed = line.trim_start();
            if leading_spaces(line) == 2
                && (trimmed.starts_with("job ") || trimmed.starts_with("webhook "))
            {
                trimmed.split_whitespace().nth(1).map(str::to_owned)
            } else {
                None
            }
        })
        .collect()
}

pub(super) fn top_level_blocks<'a>(lines: &'a [String], prefix: &str) -> Vec<&'a [String]> {
    let mut blocks = Vec::new();
    let mut index = 0;

    while index < lines.len() {
        if leading_spaces(&lines[index]) == 2 && lines[index].trim_start().starts_with(prefix) {
            let start = index;
            index += 1;

            while index < lines.len() {
                let trimmed = lines[index].trim_start();
                if leading_spaces(&lines[index]) == 2 && !trimmed.is_empty() {
                    break;
                }
                index += 1;
            }

            blocks.push(&lines[start..index]);
        } else {
            index += 1;
        }
    }

    blocks
}

pub(super) fn query_blocks(lines: &[String]) -> Vec<&[String]> {
    let mut blocks = Vec::new();
    let mut index = 0;

    while index < lines.len() {
        if leading_spaces(&lines[index]) == 4 && lines[index].trim_start().starts_with("query.") {
            let start = index;
            index += 1;

            while index < lines.len() {
                let trimmed = lines[index].trim_start();
                if leading_spaces(&lines[index]) <= 4 && !trimmed.is_empty() {
                    break;
                }
                index += 1;
            }

            blocks.push(&lines[start..index]);
        } else {
            index += 1;
        }
    }

    blocks
}

pub(super) fn command_blocks(lines: &[String]) -> Vec<&[String]> {
    let mut blocks = Vec::new();
    let mut index = 0;

    while index < lines.len() {
        if leading_spaces(&lines[index]) == 2 && lines[index].trim_start().starts_with("command ") {
            let start = index;
            index += 1;

            while index < lines.len() {
                let trimmed = lines[index].trim_start();
                if leading_spaces(&lines[index]) == 2 && !trimmed.is_empty() {
                    break;
                }
                index += 1;
            }

            blocks.push(&lines[start..index]);
        } else {
            index += 1;
        }
    }

    blocks
}

pub(super) fn query_name(trimmed_line: &str) -> Option<&str> {
    let mut parts = trimmed_line.split_whitespace();
    if parts.next()?.starts_with("query.") {
        parts.next()
    } else {
        None
    }
}

fn query_kind(block: &[String]) -> &'static str {
    let header = block[0].trim_start();
    let qualifier = header.strip_prefix("query.").unwrap_or("");
    match qualifier.split_whitespace().next().unwrap_or("") {
        "lookup" => "lookup",
        "sql" => "sql",
        _ => "list",
    }
}

pub(super) fn named_top_block_name(trimmed_line: &str) -> Option<&str> {
    trimmed_line.split_whitespace().nth(1)
}

pub(super) fn command_name(trimmed_line: &str) -> Option<&str> {
    let mut parts = trimmed_line.split_whitespace();
    if parts.next()? == "command" {
        parts.next()
    } else {
        None
    }
}

fn command_needs_inferred_target(lines: &[String]) -> bool {
    let has_route_id = lines
        .iter()
        .any(|line| leading_spaces(line) == 4 && line.trim_start() == "route id: ID");
    let mutates_existing = lines.iter().any(|line| {
        leading_spaces(line) == 4
            && (line.trim_start().starts_with("updates ")
                || line.trim_start().starts_with("deletes "))
    });

    has_route_id && mutates_existing
}

fn query_param_names(lines: &[String]) -> Vec<String> {
    let mut params = Vec::new();
    let mut in_params = false;

    for line in lines {
        let trimmed = line.trim_start();

        if leading_spaces(line) == 6 {
            in_params = trimmed == "params";
            continue;
        }

        if in_params && leading_spaces(line) == 8 {
            if let Some((name, _)) = typed_declaration(trimmed) {
                params.push(name.to_owned());
            }
        } else if leading_spaces(line) <= 6 {
            in_params = false;
        }
    }

    if params.is_empty() {
        if let Some(key) = lines
            .first()
            .and_then(|line| line.trim_start().split(" by ").nth(1))
            .and_then(|rest| rest.split_once(':').map(|(name, _)| name.trim()))
            .filter(|name| !name.is_empty())
        {
            params.push(key.to_owned());
        }
    }

    params
}

fn command_route_names(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .filter_map(|line| {
            if leading_spaces(line) == 4 {
                let trimmed = line.trim_start();
                let mut parts = trimmed.split_whitespace();
                if parts.next()? == "route" {
                    return parts
                        .next()
                        .map(|name| name.trim_end_matches(':').to_owned());
                }
            }
            None
        })
        .collect()
}

fn command_input_names(lines: &[String]) -> Vec<String> {
    let mut inputs = Vec::new();
    let mut in_input = false;

    for line in lines {
        let trimmed = line.trim_start();

        if leading_spaces(line) == 4 {
            in_input = trimmed == "input";

            if let Some(rest) = trimmed.strip_prefix("input ") {
                inputs.extend(parse_ident_list(rest));
            }
            continue;
        }

        if in_input && leading_spaces(line) == 6 {
            if let Some((name, _)) = typed_declaration(trimmed) {
                inputs.push(name.to_owned());
            }
        } else if leading_spaces(line) <= 4 {
            in_input = false;
        }
    }

    inputs
}

fn typed_declaration(trimmed_line: &str) -> Option<(&str, &str)> {
    let (name, rest) = trimmed_line.split_once(':')?;
    let name = name.trim();
    let ty = rest.trim().split_whitespace().next()?;

    if name.is_empty() || ty.is_empty() {
        None
    } else {
        Some((name, ty))
    }
}

fn emits_dependencies(feature: &str, subject: &str, lines: &[String]) -> Vec<InspectDependency> {
    let mut dependencies = Vec::new();

    for line in lines {
        let trimmed = line.trim_start();

        if let Some(events) = trimmed.strip_prefix("emits ") {
            let origin = if emits_derived_effect(events).is_some() {
                "emits.derived"
            } else {
                "emits"
            };
            for event in parse_event_list(events) {
                dependencies.push(inspect_dependency(
                    "emits_event",
                    subject,
                    qualify_event_ref(feature, &event),
                    origin,
                ));
            }
        } else if is_transition_line(trimmed) {
            if let Some(event) = trailing_scalar_value_after(trimmed, "emits") {
                dependencies.push(inspect_dependency(
                    "emits_event",
                    subject,
                    qualify_event_ref(feature, event),
                    "transition.emits",
                ));
            }
        }
    }

    dependencies
}

fn emits_derived_effect(emits_rest: &str) -> Option<&'static str> {
    let mut tokens = emits_rest.split_whitespace();
    tokens.next()?;
    if tokens.next()? != "from" {
        return None;
    }
    match tokens.next()? {
        "creates" => Some("creates"),
        "updates" => Some("updates"),
        "deletes" => Some("deletes"),
        _ => None,
    }
}

fn query_reference_dependencies(subject: &str, lines: &[String]) -> Vec<InspectDependency> {
    let mut dependencies = Vec::new();

    for line in lines {
        let trimmed = line.trim_start();

        for prefix in ["target ", "source "] {
            if let Some(value) = trimmed.strip_prefix(prefix) {
                if let Some(query) = value
                    .split_once('(')
                    .map(|(query, _)| query)
                    .or_else(|| value.split_whitespace().next())
                    .filter(|query| query.contains("query."))
                {
                    dependencies.push(inspect_dependency(
                        "query_ref",
                        subject,
                        query.trim(),
                        prefix.trim(),
                    ));
                }
            }
        }
    }

    dependencies
}

fn parse_event_list(source: &str) -> Vec<String> {
    let first = source.split_whitespace().next().unwrap_or(source);
    first
        .split(',')
        .map(str::trim)
        .filter(|event| {
            !event.is_empty()
                && event
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'.')
        })
        .map(str::to_owned)
        .collect()
}

fn qualify_event_ref(feature: &str, event: &str) -> String {
    if event.contains('.') {
        event.to_owned()
    } else {
        format!("{feature}.{event}")
    }
}

fn trailing_scalar_value_after<'a>(trimmed_line: &'a str, keyword: &str) -> Option<&'a str> {
    let mut tokens = trimmed_line.split_whitespace();
    while let Some(token) = tokens.next() {
        if token == keyword {
            return tokens.next();
        }
    }
    None
}

pub(super) fn direct_child_value(lines: &[String], prefix: &str) -> Option<String> {
    let child_indent = lines.first().map(|line| leading_spaces(line) + 2)?;

    lines.iter().find_map(|line| {
        if leading_spaces(line) == child_indent {
            line.trim_start().strip_prefix(prefix).map(str::to_owned)
        } else {
            None
        }
    })
}

fn inspect_notifications(
    lines: &[String],
    tier3: Option<&Tier3FeatureSlice>,
) -> Vec<InspectNotification> {
    let mut notifications = Vec::new();

    for block in top_level_blocks(lines, "notification ") {
        let name = named_top_block_name(block[0].trim_start())
            .unwrap_or("unknown")
            .to_owned();
        let channels = direct_child_value(block, "channel ")
            .map(|raw| {
                raw.split(',')
                    .map(|c| c.trim().to_owned())
                    .filter(|c| !c.is_empty())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let recipient = direct_child_value(block, "recipient ");
        let trigger = direct_child_value(block, "trigger ");
        let template = direct_child_value(block, "template ")
            .as_deref()
            .map(strip_quotes);
        let policy = direct_child_value(block, "policy ");
        let tenant_from = direct_child_value(block, "tenant_from ");
        let idempotency = direct_child_value(block, "idempotency ");
        let retry = direct_child_value(block, "retry ");
        let rate_limit = direct_child_value(block, "rate_limit ")
            .as_deref()
            .map(strip_quotes);

        // Notifications expanded bucket cycle — typed `digest` /
        // `throttle` come from the lifted IR slice. The text-walker
        // owns the scalar fields above (legacy notation), but the
        // structured sub-blocks must surface typed so consumers can
        // read every-window / per-recipient / burst / strategy cold.
        let (digest, throttle) = tier3
            .and_then(|slice| slice.notifications.iter().find(|n| n.name == name))
            .map(|n| {
                let digest = n.digest.as_ref().map(|d| InspectNotificationDigest {
                    every: d.every.clone(),
                    group_by: d.group_by.clone(),
                    max_size: d.max_size,
                    template_strategy: d.template_strategy.map(|s| match s {
                        lazuli_ir::DigestStrategy::Merge => "merge".to_owned(),
                        lazuli_ir::DigestStrategy::Append => "append".to_owned(),
                    }),
                });
                let throttle = n.throttle.as_ref().map(|t| InspectNotificationThrottle {
                    max_per: t.max_per.clone(),
                    per_recipient: t.per_recipient,
                    per_channel: t.per_channel,
                    burst: t.burst,
                });
                (digest, throttle)
            })
            .unwrap_or((None, None));

        notifications.push(InspectNotification {
            name,
            channels,
            recipient,
            trigger,
            template,
            policy,
            tenant_from,
            idempotency,
            retry,
            rate_limit,
            digest,
            throttle,
            origin: "notification",
        });
    }

    notifications
}

fn inspect_agents(lines: &[String]) -> Vec<InspectAgent> {
    let mut agents = Vec::new();

    for block in top_level_blocks(lines, "agent ") {
        let name = named_top_block_name(block[0].trim_start())
            .unwrap_or("unknown")
            .to_owned();
        let inputs = command_input_names(block);
        let context = direct_child_value(block, "context ")
            .as_deref()
            .map(strip_quotes);
        let policy = direct_child_value(block, "policy ");
        let rate_limit = direct_child_value(block, "rate_limit ")
            .as_deref()
            .map(strip_quotes);
        let output_raw = direct_child_value(block, "output ");
        let (output_kind, output_discriminator) = classify_agent_output(output_raw.as_deref());
        let model = direct_child_value(block, "model ");
        let prompt = direct_child_value(block, "prompt ")
            .as_deref()
            .map(strip_quotes);
        // Cut A — tool entries live as indent-6 lines under a `tools`
        // child block. The legacy `tools <comma-list>` shorthand never
        // existed in the canonical syntax; the previous text extractor
        // returned `None` for the canonical form. This walker handles
        // both for safety while older fixtures linger.
        let tools = collect_agent_block_entries(block, "tools");
        let evals = collect_agent_eval_case_names(block);
        let safety = direct_child_value(block, "safety ");

        let temperature = direct_child_value(block, "temperature ");
        let max_tokens = direct_child_value(block, "max_tokens ");
        let top_p = direct_child_value(block, "top_p ");
        let seed = direct_child_value(block, "seed ");

        let eval_determinism = if evals.is_empty() {
            None
        } else {
            let temp_zero = temperature.as_deref().and_then(|s| s.parse::<f64>().ok()) == Some(0.0);
            let seed_present = seed.is_some();
            Some(if temp_zero && seed_present {
                "pinned"
            } else {
                "nondeterministic"
            })
        };

        let expose_http = collect_agent_expose(block);

        agents.push(InspectAgent {
            name,
            inputs,
            context,
            policy,
            rate_limit,
            output: output_raw,
            output_kind,
            output_discriminator,
            model,
            temperature,
            max_tokens,
            top_p,
            seed,
            prompt,
            tools,
            evals,
            eval_determinism,
            safety,
            expose_http,
            origin: "agent",
        });
    }

    agents
}

/// Derive `(output_kind, output_discriminator)` from the raw text after
/// `output `. The discriminator name surfaces for the two discriminated
/// shapes plus the bare-record form (lowering disambiguates record vs
/// text via the workspace IR; we record the symbol verbatim).
fn classify_agent_output(raw: Option<&str>) -> (Option<&'static str>, Option<String>) {
    let Some(raw) = raw else { return (None, None) };
    let trimmed = raw.trim();
    if let Some(rest) = trimmed.strip_prefix("stream ") {
        return (Some("stream"), Some(rest.trim().to_owned()));
    }
    if let Some(rest) = trimmed.strip_prefix("discriminator ") {
        return (Some("discriminated_enum"), Some(rest.trim().to_owned()));
    }
    if trimmed.is_empty() {
        return (None, None);
    }
    // Bare type ref. Text builtins keep `text`; PascalCase identifiers
    // (likely an author-defined record/enum) carry the symbol forward so
    // doctor's `agent_discriminator_target_invalid_diagnostics` and the
    // expand pass can interpret. The `text` label stays — lowering
    // promotes to `discriminated_record` when records resolve.
    let first = trimmed.chars().next();
    let looks_like_symbol = first.is_some_and(|c| c.is_ascii_uppercase());
    let kind = if matches!(
        trimmed,
        "Text" | "Integer" | "Boolean" | "Decimal" | "Date" | "DateTime" | "Json" | "ID"
    ) {
        "text"
    } else if looks_like_symbol {
        // Could be a record-with-discriminator (DiscriminatedRecord) or
        // a plain record reference; expand-pass disambiguates. We label
        // as `text` here to keep the file-local pass single-pass; the
        // symbol is surfaced via `output_discriminator`.
        "text"
    } else {
        "text"
    };
    let discriminator = if looks_like_symbol {
        Some(trimmed.to_owned())
    } else {
        None
    };
    (Some(kind), discriminator)
}

/// Walk the agent body for `<block> NEWLINE\n   <entry>\n   ...` and
/// return the indent-6 children as their raw trimmed source. Used for
/// both the `tools` and a future cut's other list-shaped children.
fn collect_agent_block_entries(block: &[String], parent: &str) -> Vec<String> {
    let Some(parent_indent) = block.first().map(|line| leading_spaces(line)) else {
        return Vec::new();
    };
    let child_indent = parent_indent + 2;
    let grandchild_indent = child_indent + 2;

    let mut entries = Vec::new();
    let mut in_block = false;
    for line in block.iter().skip(1) {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let leading = leading_spaces(line);
        if leading <= parent_indent {
            break;
        }
        if leading == child_indent {
            in_block = trimmed == parent;
            continue;
        }
        if in_block && leading == grandchild_indent {
            entries.push(trimmed.to_owned());
        }
    }
    entries
}

/// Walk the agent body for an `expose http` block and surface the
/// declared method/path/route/audience/rate_limit. Cut A.7's
/// inspect-side observable; doctor handles cross-feature resolution.
fn collect_agent_expose(block: &[String]) -> Option<InspectAgentExpose> {
    let parent_indent = block.first().map(|line| leading_spaces(line))?;
    let child_indent = parent_indent + 2;
    let grandchild_indent = child_indent + 2;

    let mut in_expose = false;
    let mut method: Option<String> = None;
    let mut path: Option<String> = None;
    let mut route_slots: Vec<String> = Vec::new();
    let mut audience: Option<String> = None;
    let mut rate_limit_override: Option<String> = None;

    for line in block.iter().skip(1) {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let leading = leading_spaces(line);
        if leading <= parent_indent {
            break;
        }
        if leading == child_indent {
            in_expose = trimmed == "expose http";
            continue;
        }
        if in_expose && leading == grandchild_indent {
            if let Some(rest) = trimmed.strip_prefix("method ") {
                method = Some(rest.trim().to_ascii_uppercase());
            } else if let Some(rest) = trimmed.strip_prefix("path ") {
                path = Some(strip_quotes(rest.trim()).to_owned());
            } else if let Some(rest) = trimmed.strip_prefix("route ") {
                if let Some((name_part, _)) = rest.split_once(':') {
                    route_slots.push(name_part.trim().to_owned());
                }
            } else if let Some(rest) = trimmed.strip_prefix("audience ") {
                audience = Some(rest.trim().to_owned());
            } else if let Some(rest) = trimmed.strip_prefix("rate_limit ") {
                rate_limit_override = Some(strip_quotes(rest.trim()).to_owned());
            }
        }
    }

    let method = method?;
    let path = path?;
    Some(InspectAgentExpose {
        method,
        path,
        route_slots,
        audience,
        rate_limit_override,
    })
}

/// Walk the agent body for `evals` and return the list of eval `case`
/// names declared inside.
fn collect_agent_eval_case_names(block: &[String]) -> Vec<String> {
    let Some(parent_indent) = block.first().map(|line| leading_spaces(line)) else {
        return Vec::new();
    };
    let child_indent = parent_indent + 2;
    let grandchild_indent = child_indent + 2;

    let mut cases = Vec::new();
    let mut in_block = false;
    for line in block.iter().skip(1) {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let leading = leading_spaces(line);
        if leading <= parent_indent {
            break;
        }
        if leading == child_indent {
            in_block = trimmed == "evals";
            continue;
        }
        if in_block && leading == grandchild_indent {
            if let Some(rest) = trimmed.strip_prefix("case ") {
                let name = rest.split_whitespace().next().unwrap_or("").to_owned();
                if !name.is_empty() {
                    cases.push(name);
                }
            }
        }
    }
    cases
}

pub(super) fn parse_audit(lines: &[String], origin: &'static str) -> Option<InspectAudit> {
    let child_indent = lines.first().map(|line| leading_spaces(line) + 2)?;
    let audit_grandchild_indent = child_indent + 2;

    let mut hit_index: Option<usize> = None;
    let mut audit: Option<InspectAudit> = None;
    for (offset, line) in lines.iter().enumerate().skip(1) {
        if leading_spaces(line) != child_indent {
            continue;
        }
        let trimmed = line.trim_start();
        if trimmed == "audit" {
            audit = Some(InspectAudit {
                fields: Vec::new(),
                emit_to: None,
                origin,
            });
            hit_index = Some(offset);
            break;
        }
        if let Some(rest) = trimmed.strip_prefix("audit ") {
            let rest = rest.trim();
            if rest == "none" {
                return None;
            }
            let fields: Vec<String> = rest
                .split(',')
                .map(|part| part.trim().to_owned())
                .filter(|part| !part.is_empty())
                .collect();
            audit = Some(InspectAudit {
                fields,
                emit_to: None,
                origin,
            });
            hit_index = Some(offset);
            break;
        }
    }

    // Observability bucket cycle row 37 — scan grandchildren of the
    // `audit` line for an `emit_to <target>` slot. The slot lives one
    // indent step deeper than `audit` and stops at the next
    // sibling-or-shallower line.
    if let (Some(start), Some(audit_value)) = (hit_index, audit.as_mut()) {
        for line in lines.iter().skip(start + 1) {
            let leading = leading_spaces(line);
            if leading <= child_indent {
                break;
            }
            if leading != audit_grandchild_indent {
                continue;
            }
            let trimmed = line.trim_start();
            if let Some(rest) = trimmed.strip_prefix("emit_to ") {
                audit_value.emit_to = Some(rest.trim().to_owned());
                break;
            }
        }
    }

    audit
}

pub(super) fn direct_child_values(lines: &[String], prefix: &str) -> Vec<String> {
    let Some(child_indent) = lines.first().map(|line| leading_spaces(line) + 2) else {
        return Vec::new();
    };

    lines
        .iter()
        .filter_map(|line| {
            if leading_spaces(line) == child_indent {
                line.trim_start()
                    .strip_prefix(prefix)
                    .map(str::trim)
                    .map(str::to_owned)
            } else {
                None
            }
        })
        .collect()
}

pub(super) fn field_name_from_typed_line(trimmed_line: &str) -> Option<&str> {
    let (head, _) = trimmed_line.split_once(':')?;
    let name = head.trim().split_whitespace().next()?;

    if name
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        Some(name)
    } else {
        None
    }
}

pub(super) fn security_markers(line: &str) -> impl Iterator<Item = String> + '_ {
    namespace_references(line)
        .into_iter()
        .filter(|namespace| matches!(*namespace, "pii" | "cap" | "key"))
        .filter_map(|namespace| full_marker_reference(line, namespace))
}

fn full_marker_reference(line: &str, namespace: &str) -> Option<String> {
    let start = line.find(&format!("@{namespace}."))?;
    let after = &line[start..];
    let mut end = after
        .bytes()
        .position(|byte| !(byte.is_ascii_alphanumeric() || matches!(byte, b'@' | b'_' | b'.')))
        .unwrap_or(after.len());

    if after.as_bytes().get(end) == Some(&b'(') {
        end = after[end..]
            .find(')')
            .map(|relative| end + relative + 1)
            .unwrap_or(after.len());
    }

    Some(after[..end].to_owned())
}

fn resolve_policy_atoms(policy: &str, policies: &BTreeMap<String, Vec<String>>) -> Vec<String> {
    let policy = policy.strip_prefix("@policy.").unwrap_or(policy);
    policies
        .get(policy)
        .cloned()
        .unwrap_or_else(|| vec![policy.to_owned()])
}

fn inspect_inherited_payload_field(entry: &str, origin: String) -> InspectPayloadField {
    let Some((name, expression)) = entry.split_once('=') else {
        return InspectPayloadField {
            name: entry.to_owned(),
            ty: "Unknown".to_owned(),
            origin,
            expression: None,
            condition: None,
        };
    };
    let (expression, condition) = expression
        .split_once(" when ")
        .map(|(value, condition)| (value.trim(), Some(condition.trim().to_owned())))
        .unwrap_or((expression.trim(), None));

    InspectPayloadField {
        name: name.trim().to_owned(),
        ty: infer_payload_type(name.trim(), expression).to_owned(),
        origin,
        expression: Some(expression.to_owned()),
        condition,
    }
}

fn inspect_explicit_payload_field(line: &str, event_name: &str) -> Option<InspectPayloadField> {
    let (name, rest) = line.split_once(':')?;
    let ty = rest.split_whitespace().next()?;

    Some(InspectPayloadField {
        name: name.trim().to_owned(),
        ty: ty.to_owned(),
        origin: format!("event:{event_name}"),
        expression: None,
        condition: None,
    })
}

fn infer_payload_type(name: &str, expression: &str) -> &'static str {
    if name.ends_with("_id") || expression == "id" || expression.ends_with(".id") {
        "ID"
    } else {
        "Unknown"
    }
}

fn transition_name(trimmed_line: &str) -> Option<&str> {
    trimmed_line.split_once(':')?.0.split_whitespace().next()
}

fn is_transition_line(trimmed_line: &str) -> bool {
    let Some((left, right)) = trimmed_line.split_once(':') else {
        return false;
    };

    !left.trim().is_empty() && right.contains("->")
}

fn transition_requires(trimmed_line: &str) -> Option<String> {
    let (_, rhs) = trimmed_line.split_once(':')?;
    let (_, after_arrow) = rhs.trim().split_once("->")?;
    let mut tokens = after_arrow.split_whitespace();
    tokens.next()?;

    while let Some(token) = tokens.next() {
        if token == "requires" {
            return tokens.next().map(str::to_owned);
        }
    }

    None
}

fn inspect_subject(trimmed_line: &str) -> Option<String> {
    if let Some(name) = command_name(trimmed_line) {
        Some(format!("command.{name}"))
    } else if trimmed_line.starts_with("rule ") {
        Some(format!(
            "rule.{}",
            trimmed_line
                .trim_start_matches("rule ")
                .trim_matches('"')
                .to_owned()
        ))
    } else if view_anchor_line(trimmed_line) {
        trimmed_line
            .split(" id ")
            .nth(1)
            .and_then(|rest| rest.split_whitespace().next())
            .map(|anchor| format!("view.{anchor}"))
            .or_else(|| Some("view.anchor".to_owned()))
    } else if is_transition_line(trimmed_line) {
        transition_name(trimmed_line).map(|name| format!("transition.{name}"))
    } else {
        None
    }
}

fn view_anchor_line(trimmed_line: &str) -> bool {
    trimmed_line.starts_with("view ") && trimmed_line.contains(" id @anchor.")
}

fn test_group(assertion: &str) -> &'static str {
    if assertion.starts_with("permits @")
        || assertion.starts_with("forbids @")
        || assertion.contains(" as @")
    {
        "authz"
    } else if assertion.contains(" from ") {
        "transition"
    } else if assertion.contains(" when ") {
        "predicate"
    } else if assertion.starts_with("accepted by ") || assertion.starts_with("rejected by ") {
        "anchor"
    } else {
        "other"
    }
}

mod expand;

#[cfg(test)]
#[allow(unused_imports)]
pub(crate) use expand::expand_canonical_source;
use expand::{
    collect_event_decls, collect_event_groups, expand_canonical_source_with, feature_has_id_lookup,
    is_identifier, is_type_name, leading_spaces, namespace_references, parse_ident_list,
};

