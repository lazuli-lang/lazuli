//! Per-axis projections that materialise the `--expand=<axis>` slices
//! of an `InspectFeature`.
//!
//! Each submodule owns exactly one expand axis (the JSON key emitted
//! under `inspect.features[].<axis>`) and surfaces one public-to-`super`
//! entry point. The orchestrator in `commands/inspect/mod.rs` calls
//! these as part of `inspect_feature` after the Tier 3 IR slice has
//! been lifted; the projections are pure functions over
//! `(&[String], Option<&Tier3FeatureSlice>, ...)` so they can be unit
//! tested without an `InspectReport` round-trip.
//!
//! Why the per-axis split: each projection has a different blend of
//! IR-driven and text-walker logic, with axis-specific helpers that
//! aren't shared. Keeping every axis in one file was viable when the
//! inspect module was the cluster's only home (R3-D); the Rails-style
//! ≤500 LOC budget forces them apart. The shared low-level walkers
//! still live in `commands/inspect/text_walkers.rs`; only the
//! axis-specific shaping moves here.
//!
//! Axes covered:
//!
//! - `storage` — `@cap.File(...)` projection per resource field / api
//!   output. Includes the auth-block projection (`project_auth`) and
//!   the `inspect_origin` helper because both share the
//!   capability-shaping utilities with storage.
//! - `locators` — execution context bindings per command / query /
//!   job / webhook / rule.
//! - `dependencies` — `uses`, `extends`, `emits_event`, `query_ref`
//!   edges between feature constructs.
//! - `defaults` — IR-driven `Feature.defaults` (timestamps, tenancy,
//!   policy) plus the legacy text walker for documents that don't
//!   lower; `query_order` / `query_filter_index` derivations stay
//!   text-driven because they're heuristics over query bodies.
//! - `events` — event-decl payload projection (explicit + inherited
//!   from `event_group`s) plus the built-in trace event list.
//! - `targets` — explicit / inferred `target` slot per command.
//! - `policies` — per-command / per-query / per-workflow-transition
//!   policy projection with `when_denied` resolution-chain lookups.
//! - `tests` — declared and authz-derived test assertions per
//!   subject.
//! - `notifications` — text-walker for the scalar fields plus
//!   IR-lifted `digest` / `throttle` from the Tier 3 slice.
//! - `agents` — agent text walker, tool-binding projection, expose-http
//!   block extractor.
//! - `expose` — unified HTTP route table (agent expose + `api` blocks).
//! - `requirements` — `requires integration <slot>: <Capability>`.
//! - `external_calls` — `calls <slot>.<op>` per command / job, plus
//!   the surrounding `timeout` / `retry` / `idempotency` envelope.
//!
//! ABI: nothing in this subtree is `pub(crate)`. The orchestrator
//! consumes each entry point via `pub(super) use`.

mod agents;
mod defaults;
mod dependencies;
mod events;
mod expose;
mod external_calls;
mod locators;
mod notifications;
mod policies;
mod requirements;
mod storage;
mod targets;
mod tests;

pub(in crate::commands::inspect) use agents::{inspect_agent_tools_projection, inspect_agents};
pub(in crate::commands::inspect) use defaults::inspect_defaults;
pub(in crate::commands::inspect) use dependencies::inspect_dependencies;
pub(in crate::commands::inspect) use events::{inspect_built_in_trace_events, inspect_events};
pub(in crate::commands::inspect) use expose::inspect_expose_projection;
pub(in crate::commands::inspect) use external_calls::inspect_external_calls;
pub(in crate::commands::inspect) use locators::inspect_locators;
pub(in crate::commands::inspect) use notifications::inspect_notifications;
pub(in crate::commands::inspect) use policies::inspect_policies;
pub(in crate::commands::inspect) use requirements::inspect_requirements;
pub(in crate::commands::inspect) use storage::{inspect_storage_projection, project_auth};
pub(in crate::commands::inspect) use targets::inspect_targets;
pub(in crate::commands::inspect) use tests::inspect_tests;
