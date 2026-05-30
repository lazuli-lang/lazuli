/// Emit the `type <Name>Input struct` block for a command.
///
/// Field order: route slots first (URL path params — `route id: ID`),
/// then typed body slots. Route params always emit `validate:"required"`
/// because the path is the addressing key; without them the Effect's
/// `Bindings{...: FromInput("ID")}` resolves against nothing.
fn emit_input_struct(
    p: &mut GoPrinter,
    name: &str,
    route_slots: &[RouteSlot],
    slots: &[TypedSlot],
    ctx: &TypeCtx<'_>,
) {
    p.line(&format!("type {name} struct {{"));
    p.indent();
    let mut rows: Vec<(String, String, String)> =
        Vec::with_capacity(route_slots.len() + slots.len());
    // Route slots come first — `route id: ID` becomes `Id ID
    // \`json:"id" validate:"required"\``. Route slots have no inline
    // constraints in the IR (`RouteSlot` carries no `FieldConstraints`),
    // and are always required by definition (the URL path can't be
    // optional).
    let empty_constraints = FieldConstraints::default();
    for slot in route_slots {
        let (go_type, _import) = types::go_type_for(&slot.type_ref, ctx);
        let validate_body = super::super::validator_tag_body(&empty_constraints, true);
        let tag = if validate_body.is_empty() {
            format!("`json:\"{}\"`", slot.name)
        } else {
            format!("`json:\"{}\" validate:\"{}\"`", slot.name, validate_body)
        };
        rows.push((pascal_case(&slot.name), go_type, tag));
    }
    for slot in slots {
        let (go_type, _import) = types::go_type_for(&slot.type_ref, ctx);
        let optional = !slot.required;
        let final_type = if optional {
            format!("*{}", go_type)
        } else {
            go_type
        };
        let json_suffix = if optional {
            format!("{},omitempty", slot.name)
        } else {
            slot.name.clone()
        };
        // L0 #3 §10 — pick up inline constraints (Cells D.1+D.3). The
        // tag chain stays deterministic: `json:"…"` then optional
        // `validate:"…"` (only when the slot is required OR carries
        // at least one constraint).
        let validate_body = super::super::validator_tag_body(&slot.constraints, slot.required);
        let tag = if validate_body.is_empty() {
            format!("`json:\"{}\"`", json_suffix)
        } else {
            format!("`json:\"{}\" validate:\"{}\"`", json_suffix, validate_body)
        };
        rows.push((pascal_case(&slot.name), final_type, tag));
    }
    let row_refs: Vec<(&str, &str, &str)> = rows
        .iter()
        .map(|(n, t, g)| (n.as_str(), t.as_str(), g.as_str()))
        .collect();
    p.aligned_struct_rows(&row_refs);
    p.dedent();
    p.line("}");
}

// Test-host siblings — `emit.rs`'s inline `mod tests` was 377 LOC of
// integration tests that exercised `emit_command_file` through every
// `CommandEffect` variant and every binding-source shape. Wave R8-2c
// split them by sub-concern of the production code into two sibling
// files (`emit_effect_dispatch_tests.rs` + `emit_bindings_and_handlers_tests.rs`),
// wired in from `command/mod.rs`. See each sibling's `//!` header for
// the per-file coverage map.
