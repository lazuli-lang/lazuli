//! Diagnostics, completions, hovers, and code-action backends for the
//! IR Lifecycle Route-Gates family (`requires_lifecycle`, `resume`,
//! `pending`, `→` arms).
//!
//! This module is the entry point. All producers, fact collectors,
//! and helpers live in concern-shaped sub-modules; `mod.rs` only
//! wires them together and re-exports through `pub(crate) use` so
//! the crate-root `pub(crate) use diagnostics::lifecycle::*;` glob
//! in `lib.rs` keeps resolving `crate::*` references from outside the
//! cluster (notably `crate::code_actions::lifecycle_gate`).
//!
//! Two public entry points are surfaced to `lib.rs::server`:
//! [`completion::lifecycle_gate_completions`] and
//! [`hover::lifecycle_gate_hover`].
//!
//! | Sub-module | Concern |
//! |---|---|
//! | [`parse`] | identifier validation, named-header recognition, `uses` enumeration, slug/snake-case |
//! | [`state`] | `LifecycleResourceInfo` + resource/lifecycle/state collectors |
//! | [`lookup`] | `LifecycleLookupQueryInfo` + `query.lookup` + command-resource resolvers |
//! | [`resume`] | `LifecycleResumeBlock` + `LifecycleResumeArm` + arm coverage helpers |
//! | [`gate`] | `LifecycleGateCandidate` + reachability + route-path inference |
//! | [`completion`] | trigger detectors + completion-item builders + `lifecycle_gate_completions` |
//! | [`hover`] | hover constants + `lifecycle_gate_hover` |

mod completion;
mod gate;
mod hover;
mod lookup;
mod parse;
mod resume;
mod state;

// ABI: every `pub(crate)` helper exposed by the pre-split monolithic
// `lifecycle.rs` is re-exported here. `#[allow(unused_imports)]`
// covers helpers consumed only via the crate-root
// `pub(crate) use diagnostics::lifecycle::*;` glob in `lib.rs`.
#[allow(unused_imports)]
pub use completion::lifecycle_gate_completions;
#[allow(unused_imports)]
pub use hover::lifecycle_gate_hover;

#[allow(unused_imports)]
pub(crate) use completion::{
    collect_lifecycle_view_names, lifecycle_after_arrow, lifecycle_identifier_prefix,
    lifecycle_keyword_completion, lifecycle_lookup_query_completion_items,
    lifecycle_reference_completion, lifecycle_resource_completion_items,
    lifecycle_resume_arm_completion_items, lifecycle_resume_arm_state_known,
    lifecycle_resume_completion_items, lifecycle_scoped_label, lifecycle_state_completion_items,
    lifecycle_view_completion_items, lifecycle_view_slot_trigger, requires_lifecycle_state_trigger,
    resume_arm_start_trigger, resume_arm_view_keyword_trigger, resume_arm_view_target_trigger,
    resume_header_source_trigger,
};
#[allow(unused_imports)]
pub(crate) use gate::{
    LifecycleGateCandidate, lifecycle_default_gate_state, lifecycle_feature_is_reachable,
    lifecycle_gate_candidate_for_view, lifecycle_gate_insertion_line,
    lifecycle_pending_resume_for_view, lifecycle_reachable_features, lifecycle_resource_for_name,
    lifecycle_resource_for_source_ref, lifecycle_resource_for_submit_ref,
    lifecycle_resources_hosted_by_view, lifecycle_state_from_view_path, lifecycle_view_path,
    view_has_requires_lifecycle,
};
#[allow(unused_imports)]
pub(crate) use hover::{
    LIFECYCLE_ARROW_HOVER, LIFECYCLE_NONE_HOVER, LIFECYCLE_PENDING_HOVER, LIFECYCLE_REQUIRES_HOVER,
    LIFECYCLE_RESUME_HOVER, LIFECYCLE_SOURCE_QUERY_HOVER, LIFECYCLE_WILDCARD_HOVER,
    lifecycle_hover_is_arrow, lifecycle_hover_is_wildcard, lifecycle_resolved_gate_hover,
};
#[allow(unused_imports)]
pub(crate) use lookup::{
    LifecycleLookupQueryInfo, collect_lifecycle_lookup_queries, resolve_lifecycle_command_resource,
    resolve_lifecycle_lookup_query,
};
#[allow(unused_imports)]
pub(crate) use parse::{
    lifecycle_ident, lifecycle_parse_uses_rest, lifecycle_top_level_named_header,
    lifecycle_uses_in_block, slug_for_lifecycle_token, snake_case,
};
#[allow(unused_imports)]
pub(crate) use resume::{
    LifecycleResumeArm, LifecycleResumeBlock, collect_lifecycle_resume_blocks,
    enclosing_lifecycle_resume_block, lifecycle_missing_resume_states, lifecycle_parse_resume_arm,
    lifecycle_resource_for_resume, lifecycle_resume_arm_insertion_line,
    lifecycle_resume_for_resource, lifecycle_stale_resume_arm_on_line,
};
#[allow(unused_imports)]
pub(crate) use state::{
    LifecycleResourceInfo, collect_lifecycle_resources, lifecycle_state_children,
    lifecycle_states_in_resource_block,
};
