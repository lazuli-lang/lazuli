//! Shared text-walking primitives consumed by every per-axis projection.
//!
//! Phase L Tier 4 and later moved most of inspect's heavy lifting onto
//! the lifted IR (via the `Tier3FeatureSlice` map), but the projections
//! still walk the trimmed source lines for the facts that don't yet
//! survive lower-pass elaboration: route slots, transition predicates,
//! command effects, security markers, audit emit targets, et cetera.
//!
//! This module is the umbrella over six concern-sized sub-modules:
//!
//! - `blocks` — indent-aware block partitioners and name extractors.
//! - `accessors` — direct-child accessors plus small expression helpers.
//! - `predicates` — line classifiers for transitions / view anchors /
//!   tests / inspect subjects.
//! - `deps` — dependency-shape constructors and command/query body
//!   inspectors.
//! - `audit` — audit / policy / security shaping helpers.
//! - `collectors` — name collectors and refs (declared / used) collectors.
//!
//! Every public-to-sibling helper is `pub(in crate::commands::inspect)`;
//! nothing escapes the `inspect` subtree.

mod accessors;
mod audit;
mod blocks;
mod collectors;
mod deps;
mod predicates;

pub(in crate::commands::inspect) use accessors::{
    block_has_exact_line, block_prefixed_value, block_scalar_value, direct_child_value,
    direct_child_values, emits_derived_effect, parse_event_list, qualify_event_ref, strip_quotes,
    trailing_scalar_value_after, typed_declaration,
};
pub(in crate::commands::inspect) use audit::{parse_audit, resolve_policy_atoms, security_markers};
pub(in crate::commands::inspect) use blocks::{
    command_blocks, command_name, field_name_from_typed_line, named_block_name,
    named_top_block_name, query_blocks, query_kind, query_name, top_level_blocks,
};
pub(in crate::commands::inspect) use collectors::{
    collect_command_names, collect_declared_ref_groups, collect_event_names,
    collect_extends_anchors, collect_extensible_by_features, collect_job_and_webhook_names,
    collect_named_top_blocks, collect_query_names, collect_record_names, collect_resource_names,
    collect_surface_names, collect_used_namespaces, collect_view_anchors,
    collect_workflow_summaries,
};
pub(in crate::commands::inspect) use deps::{
    command_input_names, command_needs_inferred_target, command_route_names, emits_dependencies,
    inspect_binding, inspect_dependency, query_param_names, query_reference_dependencies,
};
pub(in crate::commands::inspect) use predicates::{
    inspect_subject, is_transition_line, test_group, transition_name, transition_requires,
    view_anchor_line,
};
