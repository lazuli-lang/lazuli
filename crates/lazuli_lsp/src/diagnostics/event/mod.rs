//! Diagnostics for the `event` family — declaration shape, trace
//! triggers, rule self-bindings, required-field nil-checks, payload
//! references, consumer payloads, triggered-job payloads, tenant-from
//! / scheduled tenancy, and the cross-feature reachability graph.
//!
//! This module is the entry point. All producers and shared helpers
//! live in concern-shaped sub-modules; mod.rs only re-exports them
//! through `pub(crate) use` so the crate-root
//! `pub(crate) use diagnostics::event::*;` glob in `lib.rs` keeps
//! resolving `crate::*` references from outside the cluster.
//!
//! | Sub-module | Concern |
//! |---|---|
//! | [`parsers`] | leaf line-shape extractors (`event_decl_name`, `event_group_prefix`, `tenancy_axis`, `payload_assignment_field`, ...) |
//! | [`facts`] | source-wide fact collectors + `CanonicalFeatureFacts` / `EventPayloadGroup` |
//! | [`decl`] | event-kind / event-locator / target-binding shape |
//! | [`trace`] | `event.trace` trigger reachability |
//! | [`derived`] | `emits ... from`, rule-self, required-field-nil |
//! | [`payload`] | payload-reference + consumer-payload diagnostics |
//! | [`job`] | event-job tenant-from + scheduled-job tenancy |
//!
//! Cross-feature reachability (broker registration, dead-letter
//! routing) is *not* file-local; that lives in doctor.

mod decl;
mod derived;
mod facts;
mod job;
mod parsers;
mod payload;
mod trace;

// ABI: every helper exposed by the pre-split monolithic `event.rs`
// is re-exported here. `#[allow(unused_imports)]` covers helpers
// consumed only via the crate-root `pub(crate) use diagnostics::event::*;`
// glob in `lib.rs`.
#[allow(unused_imports)]
pub(crate) use decl::{
    event_kind_diagnostics, event_locator_diagnostics, target_binding_diagnostics,
};
#[allow(unused_imports)]
pub(crate) use derived::{
    emits_derived_diagnostics, legacy_rule_subject_alias, predicate_references_nil_self_field,
    required_field_nil_rule_diagnostics, rule_self_diagnostics,
};
#[allow(unused_imports)]
pub(crate) use facts::{
    collect_canonical_feature_facts, collect_event_contracts, collect_feature_tenant_axes,
    collect_required_resource_fields, collect_trace_events, insert_tenant_axis,
    CanonicalFeatureFacts, CanonicalResourceFacts, EventPayloadGroup,
};
#[allow(unused_imports)]
pub(crate) use job::{
    event_job_tenant_from_diagnostic, event_job_tenant_from_diagnostics,
    scheduled_job_tenancy_diagnostics, scheduled_job_tenancy_facts_diagnostics,
    EventJobTenantFacts, ScheduledJobFacts,
};
#[allow(unused_imports)]
pub(crate) use parsers::{
    event_decl_name, event_group_prefix, events_resource_name, field_name,
    payload_assignment_field, payload_assignment_rhs, payload_field_references,
    qualify_group_event_name, resource_field_reference, resource_name, tenancy_axis,
};
#[allow(unused_imports)]
pub(crate) use payload::{
    event_consumer_payload_diagnostic, event_consumer_payload_diagnostics,
    event_payload_reference_diagnostic, event_payload_reference_diagnostics,
    event_triggered_job_payload_diagnostics, EventTriggeredJobFacts, JobPayloadReference,
};
#[allow(unused_imports)]
pub(crate) use trace::event_trace_trigger_diagnostics;
