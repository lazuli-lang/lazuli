//! Slot/arg collectors for commands and queries.
//!
//! Three entry points: [`command_sdk_slots`] collapses
//! `Command::route` + `Command::input` into a uniform [`TsSlot`]
//! vector; [`command_input_slots`] handles input-only collection (used
//! by code paths that already accounted for route slots);
//! [`query_args`] walks `Query::{List,Lookup,Sql}` keys/filters and
//! produces the matching slot vector.
//!
//! Lifted out of the `types` god-file in the rails-style R9 split.

use super::TsSlot;
use super::resources::find_resource;

pub(crate) fn command_sdk_slots(
    feature: &lazuli_ir::Feature,
    command: &lazuli_ir::Command,
    module: &lazuli_ir::Module,
) -> Vec<TsSlot> {
    let mut slots = Vec::new();
    for route in &command.route {
        slots.push(TsSlot {
            name: route.name.clone(),
            type_ref: route.type_ref.clone(),
            required: route.from.is_none(),
            constraints: lazuli_ir::FieldConstraints::default(),
        });
    }
    slots.extend(command_input_slots(feature, command, module));
    slots
}

pub(crate) fn command_input_slots(
    feature: &lazuli_ir::Feature,
    command: &lazuli_ir::Command,
    module: &lazuli_ir::Module,
) -> Vec<TsSlot> {
    match &command.input {
        lazuli_ir::CommandInput::Empty => Vec::new(),
        lazuli_ir::CommandInput::Typed(slots) => slots
            .iter()
            .map(|slot| TsSlot {
                name: slot.name.clone(),
                type_ref: slot.type_ref.clone(),
                required: slot.required,
                constraints: slot.constraints.clone(),
            })
            .collect(),
        lazuli_ir::CommandInput::Short(names) => {
            let resource = command_resource(feature, command, module);
            names
                .iter()
                .map(|name| {
                    let field = resource.and_then(|r| r.fields.iter().find(|f| f.name == *name));
                    TsSlot {
                        name: name.clone(),
                        type_ref: field
                            .map(|f| f.type_ref.clone())
                            .unwrap_or(lazuli_ir::TypeRef::Builtin(lazuli_ir::BuiltinType::Text)),
                        required: field.map(|f| f.required).unwrap_or(true),
                        constraints: field.map(|f| f.constraints.clone()).unwrap_or_default(),
                    }
                })
                .collect()
        }
    }
}

pub(crate) fn query_args(
    feature: &lazuli_ir::Feature,
    query: &lazuli_ir::Query,
    module: &lazuli_ir::Module,
) -> Vec<TsSlot> {
    match query {
        lazuli_ir::Query::List(q) => q.params.iter().map(ts_slot_from_typed).collect(),
        lazuli_ir::Query::Sql(q) => q.params.iter().map(ts_slot_from_typed).collect(),
        lazuli_ir::Query::Lookup(q) => {
            let mut slots: Vec<TsSlot> = q.params.iter().map(ts_slot_from_typed).collect();
            if slots.is_empty() {
                for key in &q.keys {
                    if let lazuli_ir::Expr::Path(path) = &key.equals {
                        if path.segments.first().is_some_and(|s| s == "input") {
                            if let Some(name) = path.segments.get(1) {
                                slots.push(query_input_slot(feature, module, name));
                            }
                        }
                    }
                }
            }
            if slots.is_empty() {
                collect_input_slots_from_filters(feature, module, &q.filters, &mut slots);
            }
            if slots.is_empty() {
                if let Some(name) = q.name.strip_prefix("by_") {
                    slots.push(query_input_slot(feature, module, name));
                }
            }
            slots
        }
    }
}

fn collect_input_slots_from_filters(
    feature: &lazuli_ir::Feature,
    module: &lazuli_ir::Module,
    filters: &[lazuli_ir::Filter],
    slots: &mut Vec<TsSlot>,
) {
    for filter in filters {
        collect_input_slots_from_predicate(feature, module, &filter.predicate, slots);
    }
}

fn collect_input_slots_from_predicate(
    feature: &lazuli_ir::Feature,
    module: &lazuli_ir::Module,
    predicate: &lazuli_ir::Predicate,
    slots: &mut Vec<TsSlot>,
) {
    match predicate {
        lazuli_ir::Predicate::Comparison { left, right, .. } => {
            collect_input_slot_from_expr(feature, module, left, slots);
            collect_input_slot_from_expr(feature, module, right, slots);
        }
        lazuli_ir::Predicate::Has {
            collection,
            element,
        } => {
            collect_input_slot_from_expr(feature, module, collection, slots);
            collect_input_slot_from_expr(feature, module, element, slots);
        }
        lazuli_ir::Predicate::And(predicates) | lazuli_ir::Predicate::Or(predicates) => {
            for predicate in predicates {
                collect_input_slots_from_predicate(feature, module, predicate, slots);
            }
        }
    }
}

fn collect_input_slot_from_expr(
    feature: &lazuli_ir::Feature,
    module: &lazuli_ir::Module,
    expr: &lazuli_ir::Expr,
    slots: &mut Vec<TsSlot>,
) {
    let lazuli_ir::Expr::Path(path) = expr else {
        return;
    };
    if !path
        .segments
        .first()
        .is_some_and(|segment| segment == "input")
    {
        return;
    }
    let Some(name) = path.segments.get(1) else {
        return;
    };
    if slots.iter().any(|slot| slot.name == *name) {
        return;
    }
    slots.push(query_input_slot(feature, module, name));
}

fn query_input_slot(
    feature: &lazuli_ir::Feature,
    module: &lazuli_ir::Module,
    name: &str,
) -> TsSlot {
    let field = feature
        .resources
        .first()
        .and_then(|resource| resource.fields.iter().find(|field| field.name == name))
        .or_else(|| {
            module
                .features
                .iter()
                .flat_map(|feature| feature.resources.iter())
                .flat_map(|resource| resource.fields.iter())
                .find(|field| field.name == name)
        });
    TsSlot {
        name: name.to_owned(),
        type_ref: field
            .map(|field| field.type_ref.clone())
            .or_else(|| {
                name.eq_ignore_ascii_case("id")
                    .then_some(lazuli_ir::TypeRef::Builtin(lazuli_ir::BuiltinType::Id))
            })
            .unwrap_or(lazuli_ir::TypeRef::Builtin(lazuli_ir::BuiltinType::Text)),
        required: true,
        constraints: field
            .map(|field| field.constraints.clone())
            .unwrap_or_default(),
    }
}

fn ts_slot_from_typed(slot: &lazuli_ir::TypedSlot) -> TsSlot {
    TsSlot {
        name: slot.name.clone(),
        type_ref: slot.type_ref.clone(),
        required: slot.required,
        constraints: slot.constraints.clone(),
    }
}

fn command_resource<'a>(
    feature: &'a lazuli_ir::Feature,
    command: &lazuli_ir::Command,
    module: &'a lazuli_ir::Module,
) -> Option<&'a lazuli_ir::Resource> {
    match &command.effect {
        lazuli_ir::CommandEffect::Creates(effect) => find_resource(module, &effect.resource),
        lazuli_ir::CommandEffect::Updates(effect) => find_resource(module, &effect.resource),
        lazuli_ir::CommandEffect::Deletes(effect) => find_resource(module, &effect.resource),
        // W4 GAP-REORDER-01 — reorder targets a resource's table.
        lazuli_ir::CommandEffect::Reorders(effect) => find_resource(module, &effect.resource),
        lazuli_ir::CommandEffect::Returns(_) | lazuli_ir::CommandEffect::None => {
            feature.resources.first()
        }
    }
}
