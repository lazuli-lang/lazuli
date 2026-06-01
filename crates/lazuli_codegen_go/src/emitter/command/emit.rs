//! Per-command emission. `emit_command` lowers a single `Command` into
//! the Input struct + `lazuli.Command[I, O]` value + handler wrapper.
//! The file-level walker (`emit_command_file` in `mod.rs`) is the only
//! caller.
//!
//! Layers (from outer to inner):
//!
//! 1. `emit_command` — section banner, optional Input struct, the
//!    `Command[I, O]` value with its kv block (Name/Resource/Policy/
//!    RateLimit/Audit/Approval/ErrorKeys), effect block, emits,
//!    invalidates, Tier 4 fields.
//! 2. `emit_command_handler_wrapper` — the wire-level `HandleX`
//!    function (source-tag context wiring, semantic validation
//!    prelude, plan-gate prelude, dispatch to the registered
//!    `lazuli.Command[I, O]` value).
//! 3. `emit_input_struct` — the `<Verb><Resource>Input` shape
//!    (URL-path route slots first, then typed body slots).

use std::collections::BTreeMap;

use lazuli_ir::{
    Command, CommandEffect, CommandInput, Expr, Feature, FieldConstraints, Gate, RouteSlot,
    TypedSlot,
};

use super::super::error_resolver::{
    command_error_keys_var, command_has_error_keys, emit_command_error_keys,
};
use super::super::module::EmitContext;
use super::super::patterns::{
    PATTERN_COMMAND_PGX_INSERT, PATTERN_COMMAND_PGX_UPDATE, emit_pattern_header,
};
use super::super::printer::GoPrinter;
use super::super::types::{self, TypeCtx};
use super::effects::emit_effect;
use super::format::{escape_string, pascal_case, write_section_banner};
use super::lifecycle::{
    command_trigger_names, emit_transition_advances, lifecycle_transition_for,
    transition_advances_for_triggers,
};
use super::naming::{
    command_handler_func_name, command_input_struct_name, command_output_type, command_var_name,
    effect_resource_pascal, effect_resource_var, zero_value_for_go_type,
};
use super::policy::format_policy_with_expr;
use super::scope::{owner_scope_binding, resolve_scope_bindings, resolve_where_keys};
use super::semantic::emit_semantic_validate_prelude;
use super::tier4::{
    build_outbox_index, emit_emits, emit_invalidates, emit_tier4_fields, format_approval,
    format_rate_limit_struct,
};

include!("emit_p1.rs");
include!("emit_p2.rs");
