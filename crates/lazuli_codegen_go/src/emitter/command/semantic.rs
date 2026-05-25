//! LAZ-SEMANTIC-AUTO-VALIDATE — semantic-scalar validator wiring.
//!
//! Extracted from `command/mod.rs` as part of the rails-style split.
//! This module owns the pre-handler validation prelude that the Go
//! emitter writes into `Handle*` for every command whose typed input
//! carries `@semantic.*` slots backed by a plugin manifest:
//!
//! - `SemanticValidatorSlot` / `SemanticValidatorPlugin` — IR-derived
//!   views collected per command.
//! - `semantic_validator_plugins` — deduplicated import roster used
//!   by the file-level `ImportSet`.
//! - `semantic_validator_slots` — per-input-slot validator descriptors,
//!   driven by `BuiltinType::SemanticPluginType`.
//! - `emit_semantic_validate_prelude` — emits the `if __vfields := ...`
//!   block that short-circuits with `lazuli.CodeValidationFailed`
//!   carrying the offending field map + optional `field_message_keys`.
//!
//! Proposal: `docs/proposals/ir-semantic-auto-validate-2026-05-22.md`.

use lazuli_ir::{Command, CommandInput};

use super::super::printer::GoPrinter;
use super::{pascal_case, zero_value_for_go_type};

/// One @semantic.X field on a command input that needs a runtime
/// validator call before the user handler runs.
#[derive(Debug, Clone)]
pub(super) struct SemanticValidatorSlot {
    /// `pascal_case(slot.name)` — Go field name on the input struct.
    pub(super) go_field: String,
    /// Raw slot name — used as the `fields[...]` key (matches JSON shape).
    pub(super) json_field: String,
    /// True when the slot is `*T` (optional). Validation skips nil/empty.
    pub(super) optional: bool,
    /// `<alias>.<Validator>` — full Go call site (e.g.
    /// `scalarsbr.ValidateCPF`).
    pub(super) call: String,
    /// Stable code surfaced to the client (e.g. `cpf_invalid`). Sourced
    /// from the plugin manifest's `[[semantic_types]].error_code` or
    /// the convention fallback computed at resolver time.
    pub(super) error_code: String,
    /// Optional i18n key from the plugin manifest. Empty when not
    /// declared; runtime falls back to the per-feature catalog.
    pub(super) message_key: String,
    /// Plugin's Go import alias (`scalarsbr`).
    pub(super) plugin_alias: String,
    /// Plugin's Go module path from manifest (`lazuli.dev/plugin/scalars-br`).
    pub(super) plugin_go_module: String,
}

/// One plugin Go package referenced by a command's semantic
/// validators. Used to register `add_aliased` imports.
#[derive(Debug, Clone, PartialEq, Eq, Ord, PartialOrd)]
pub(super) struct SemanticValidatorPlugin {
    pub(super) alias: String,
    pub(super) import_path: String,
}

/// Walk a command's typed input slots and collect every plugin whose
/// semantic-validator must be imported. Deduplicated by (alias, path).
pub(super) fn semantic_validator_plugins(command: &Command) -> Vec<SemanticValidatorPlugin> {
    let mut seen = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for slot in semantic_validator_slots(command) {
        let key = (slot.plugin_alias.clone(), slot.plugin_go_module.clone());
        if !seen.insert(key) {
            continue;
        }
        out.push(SemanticValidatorPlugin {
            alias: slot.plugin_alias,
            import_path: slot.plugin_go_module,
        });
    }
    out
}

/// Walk a command's typed input slots and emit one
/// `SemanticValidatorSlot` per field whose type carries a non-empty
/// validator. Effective `go_module` + `error_code` + `message_key` come
/// straight from the IR variant (populated by the plugin manifest
/// resolver). Route slots never carry semantic types in v1.
pub(super) fn semantic_validator_slots(command: &Command) -> Vec<SemanticValidatorSlot> {
    let mut out = Vec::new();
    let slots = match &command.input {
        CommandInput::Typed(slots) => slots,
        _ => return out,
    };
    for slot in slots {
        // @validate.skip opts the field out of auto-validation.
        if slot.validate_skip {
            continue;
        }
        let (plugin_ns, validator, go_module, error_code, message_key) = match &slot.type_ref {
            lazuli_ir::TypeRef::Builtin(lazuli_ir::BuiltinType::SemanticPluginType {
                plugin,
                validator,
                go_module,
                error_code,
                message_key,
                ..
            }) if !validator.is_empty() => (
                plugin.as_str(),
                validator.as_str(),
                go_module.clone(),
                error_code.clone(),
                message_key.clone(),
            ),
            _ => continue,
        };
        let plugin_short = plugin_ns
            .strip_prefix("@lazuli/plugin-")
            .unwrap_or(plugin_ns)
            .to_owned();
        // Go import alias = short name with hyphens stripped
        // (`scalars-br` → `scalarsbr`). The plugin's Go package name
        // must match by convention; future authoring docs codify it.
        let alias = plugin_short.replace('-', "");
        let call = format!("{alias}.{validator}");
        out.push(SemanticValidatorSlot {
            go_field: pascal_case(&slot.name),
            json_field: slot.name.clone(),
            optional: !slot.required,
            call,
            error_code,
            message_key,
            plugin_alias: alias,
            plugin_go_module: go_module,
        });
    }
    out
}

/// Emit the pre-handler validation block for one command. When the
/// command has no semantic-validator slots, this is a no-op.
pub(super) fn emit_semantic_validate_prelude(
    p: &mut GoPrinter,
    command: &Command,
    output_type: &str,
) {
    let slots = semantic_validator_slots(command);
    if slots.is_empty() {
        return;
    }
    let any_message_keys = slots.iter().any(|s| !s.message_key.is_empty());
    let zero = zero_value_for_go_type(output_type);
    p.line("// semantic-scalar validation (LAZ-SEMANTIC-AUTO-VALIDATE).");
    if any_message_keys {
        p.line("if __vfields, __vkeys := func() (map[string]string, map[string]string) {");
    } else {
        p.line("if __vfields := func() map[string]string {");
    }
    p.indent();
    p.line("__out := map[string]string{}");
    if any_message_keys {
        p.line("__keys := map[string]string{}");
    }
    for slot in &slots {
        let setter = if !slot.message_key.is_empty() {
            format!(
                "__out[\"{json}\"] = \"{code}\"; __keys[\"{json}\"] = \"{key}\"",
                json = slot.json_field,
                code = slot.error_code,
                key = slot.message_key,
            )
        } else {
            format!(
                "__out[\"{json}\"] = \"{code}\"",
                json = slot.json_field,
                code = slot.error_code,
            )
        };
        if slot.optional {
            p.line(&format!(
                "if input.{f} != nil && *input.{f} != \"\" {{",
                f = slot.go_field
            ));
            p.indent();
            p.line(&format!(
                "if err := {call}(*input.{f}); err != nil {{ {setter}; _ = err }}",
                call = slot.call,
                f = slot.go_field,
            ));
            p.dedent();
            p.line("}");
        } else {
            p.line(&format!("if input.{f} != \"\" {{", f = slot.go_field));
            p.indent();
            p.line(&format!(
                "if err := {call}(input.{f}); err != nil {{ {setter}; _ = err }}",
                call = slot.call,
                f = slot.go_field,
            ));
            p.dedent();
            p.line("}");
        }
    }
    if any_message_keys {
        p.line("return __out, __keys");
    } else {
        p.line("return __out");
    }
    p.dedent();
    p.line("}(); len(__vfields) > 0 {");
    p.indent();
    if any_message_keys {
        p.line("__data := map[string]any{\"fields\": __vfields}");
        p.line("if len(__vkeys) > 0 { __data[\"field_message_keys\"] = __vkeys }");
        p.line(&format!(
            "return {zero}, &lazuli.Error{{Status: 400, Code: lazuli.CodeValidationFailed, Message: \"validation failed\", Data: __data}}"
        ));
    } else {
        p.line(&format!(
            "return {zero}, &lazuli.Error{{Status: 400, Code: lazuli.CodeValidationFailed, Message: \"validation failed\", Data: map[string]any{{\"fields\": __vfields}}}}"
        ));
    }
    p.dedent();
    p.line("}");
}
