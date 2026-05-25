//! Command-naming axis. `Command` short names (`create`, `update_email`)
//! lower to Go identifiers in three matched shapes — the input struct
//! type (`CreateCustomerInput`), the package-level command var
//! (`createCustomer`), and the public handler function
//! (`HandleCreate`). All three stay byte-equivalent with the spike at
//! `dist/go/customer/customer.gen.go` so codegen regressions surface as
//! diff noise on existing capsules.

use lazuli_ir::{CommandEffect, QualifiedName};

use super::super::types::{self, TypeCtx};
use super::format::{lower_camel, pascal_case};

/// `customer.create` -> `CreateCustomerInput`. Multi-word commands
/// like `update_email` slot the resource between the verb and the
/// modifier: `UpdateCustomerEmailInput`. Mirrors the spike's
/// `command_input_struct_name` so generated names stay stable.
pub(crate) fn command_input_struct_name(short_name: &str, resource_pascal: &str) -> String {
    let mut parts = short_name.split('_');
    let verb = parts.next().unwrap_or("");
    let modifier_words: Vec<&str> = parts.collect();

    let mut out = pascal_case(verb);
    out.push_str(resource_pascal);
    for w in modifier_words {
        out.push_str(&pascal_case(w));
    }
    out.push_str("Input");
    out
}

/// Command var name: lowerCamel mirror of the input struct without the
/// `Input` suffix. `create` -> `createCustomer`; `update_email` ->
/// `updateCustomerEmail`. Mirrors the spike for byte-equivalence.
pub(crate) fn command_var_name(short_name: &str, resource_pascal: &str) -> String {
    let mut parts = short_name.split('_');
    let verb = parts.next().unwrap_or("");
    let modifier_words: Vec<&str> = parts.collect();

    let mut out = verb.to_ascii_lowercase();
    out.push_str(resource_pascal);
    for w in modifier_words {
        out.push_str(&pascal_case(w));
    }
    out
}

pub(crate) fn command_handler_func_name(short_name: &str) -> String {
    format!("Handle{}", pascal_case(short_name))
}

/// Resolve the Go-side `<resource>Resource` variable name from a
/// qualified IR resource name. The resource emitter declared this var
/// in the same package using the lowerCamel form of `Resource.name`;
/// we mirror the convention here.
pub(crate) fn resource_var_for_qname(qname: &QualifiedName) -> String {
    // Cross-feature resource references would need a cross-package
    // dotted form, but Command.Effect carries `feature: None` today
    // (commands write to same-feature resources by language rule).
    // When the IR ever lifts cross-feature writes the typed slot lands
    // on this branch; until then we emit a bare lower-camel ref.
    format!("{}Resource", lower_camel(&qname.name))
}

/// Resolve the Output type for the `Command[I, O]` generic. Effects
/// pin the type to the resource pascal name; `Returns` consumes the
/// declared `TypeRef`; `None` falls back to an empty struct so the
/// generic still parses.
///
/// For `Returns`, we use `go_return_type_for` (not `go_type_for`) so
/// resource refs render as the full struct (`User`) rather than the
/// FK collapse (`lazuli.ID`). The FK collapse is correct for field
/// positions (BIGINT column) and wrong for return positions (handler
/// returns the typed row, not the id).
pub(crate) fn command_output_type(effect: &CommandEffect, ctx: &TypeCtx<'_>) -> String {
    match effect {
        CommandEffect::Creates(c) => pascal_case(&c.resource.name),
        CommandEffect::Updates(u) => pascal_case(&u.resource.name),
        CommandEffect::Deletes(d) => pascal_case(&d.resource.name),
        CommandEffect::Returns(r) => {
            let (ty, _import) = types::go_return_type_for(&r.return_type, ctx);
            ty
        }
        CommandEffect::None => "struct{}".to_owned(),
    }
}

/// Returns the resource pascal name pinned by the command's effect.
/// Used for the input struct naming axis.
pub(crate) fn effect_resource_pascal(effect: &CommandEffect) -> String {
    match effect {
        CommandEffect::Creates(c) => pascal_case(&c.resource.name),
        CommandEffect::Updates(u) => pascal_case(&u.resource.name),
        CommandEffect::Deletes(d) => pascal_case(&d.resource.name),
        CommandEffect::Returns(r) => match &r.return_type {
            lazuli_ir::TypeRef::UserDefined(q) | lazuli_ir::TypeRef::EnumRef(q) => {
                pascal_case(&q.name)
            }
            _ => "Result".to_owned(),
        },
        CommandEffect::None => "Result".to_owned(),
    }
}

/// `Some(resource_var)` when the command has a resource-bound effect,
/// otherwise `None` so we skip emitting the `Resource:` field
/// entirely.
pub(crate) fn effect_resource_var(effect: &CommandEffect) -> Option<String> {
    match effect {
        CommandEffect::Creates(c) => Some(resource_var_for_qname(&c.resource)),
        CommandEffect::Updates(u) => Some(resource_var_for_qname(&u.resource)),
        CommandEffect::Deletes(d) => Some(resource_var_for_qname(&d.resource)),
        CommandEffect::Returns(_) | CommandEffect::None => None,
    }
}

/// PG.C.1 helper — best-effort zero literal for a Go return type.
/// Used by the gate prelude when it has to short-circuit before the
/// wrapped handler runs. Falls back to `*new(T)` when the type is too
/// shaped to write a literal for (named structs, generics).
pub(crate) fn zero_value_for_go_type(ty: &str) -> String {
    let trimmed = ty.trim();
    match trimmed {
        "string" => "\"\"".to_owned(),
        "bool" => "false".to_owned(),
        "int" | "int8" | "int16" | "int32" | "int64" => "0".to_owned(),
        "uint" | "uint8" | "uint16" | "uint32" | "uint64" => "0".to_owned(),
        "float32" | "float64" => "0".to_owned(),
        "any" => "nil".to_owned(),
        "error" => "nil".to_owned(),
        "struct{}" => "struct{}{}".to_owned(),
        _ if trimmed.starts_with('*')
            || trimmed.starts_with('[')
            || trimmed.starts_with("map[")
            || trimmed.starts_with("chan ") =>
        {
            "nil".to_owned()
        }
        _ => format!("*new({trimmed})"),
    }
}
