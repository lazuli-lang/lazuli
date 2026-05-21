//! Cell A4 — `input.<field>` completion surfaces both `command.input`
//! fields and `command.route` slots so authors hit "no completion
//! offered" at edit time instead of "field not found" at codegen time.

use lazuli_lsp::input_field_completions;
use tower_lsp::lsp_types::{CompletionItem, CompletionItemKind, Position};

fn cursor_after_last(source: &str, needle: &str) -> Position {
    let offset = source
        .rfind(needle)
        .unwrap_or_else(|| panic!("needle `{needle}` not found"))
        + needle.len();
    let mut line = 0u32;
    let mut character = 0u32;
    for ch in source[..offset].chars() {
        if ch == '\n' {
            line += 1;
            character = 0;
        } else {
            character += 1;
        }
    }
    Position { line, character }
}

fn items(source: &str, position: Position) -> Vec<CompletionItem> {
    input_field_completions(source, position).unwrap_or_else(|| {
        panic!(
            "expected `input.<field>` completions at {position:?}; got None for source:\n{source}",
        )
    })
}

fn label_with_detail<'a>(items: &'a [CompletionItem], label: &str) -> Option<&'a CompletionItem> {
    items.iter().find(|i| i.label == label)
}

#[test]
fn input_dot_in_where_clause_surfaces_route_id_as_route_param() {
    // `where (id = input.<cursor>)` — authors reach for the route slot
    // here. Cell A1/A2 added `command.route id: ID`; this test pins
    // that the LSP surfaces it at this cursor and tags it as a
    // route param.
    let source = "\
feature customer
  command save_customer
    route id: ID
    input name
    updates Customer
      name = input.name
    where (id = input.
";
    let position = cursor_after_last(&source, "where (id = input.");
    let items = items(source, position);

    let id_item =
        label_with_detail(&items, "id").expect("expected `id` completion; got {items:?}");
    assert_eq!(id_item.kind, Some(CompletionItemKind::FIELD));
    assert_eq!(id_item.detail.as_deref(), Some("route param"));

    // `name` is still offered (declared as an input slot), tagged as
    // input field — proves we didn't replace input completion, we
    // extended it.
    let name_item =
        label_with_detail(&items, "name").expect("expected `name` completion; got {items:?}");
    assert_eq!(name_item.detail.as_deref(), Some("input field"));
}

#[test]
fn route_params_lead_input_fields_in_completion_order() {
    // Route params should sort to the top so authors see the
    // route-vs-input distinction at-a-glance.
    let source = "\
feature customer
  command save_customer
    route id: ID
    input name, email
    updates Customer
      email = input.
";
    let position = cursor_after_last(&source, "email = input.");
    let items = items(source, position);

    let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
    let id_idx = labels
        .iter()
        .position(|l| *l == "id")
        .expect("`id` route param must be offered");
    let name_idx = labels
        .iter()
        .position(|l| *l == "name")
        .expect("`name` input field must be offered");
    assert!(
        id_idx < name_idx,
        "route params should lead input fields; labels were {labels:?}",
    );
}

#[test]
fn typed_input_block_fields_surface_alongside_route_params() {
    // The typed `input` block form (`input` header + `<name>: <Type>`
    // children at indent 6) must also be picked up.
    let source = "\
feature billing
  command record_charge
    route order_id: ID
    input
      amount: Money
      memo: Text
    creates Charge from input
    where (order_id = input.
";
    let position = cursor_after_last(&source, "where (order_id = input.");
    let items = items(source, position);

    let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
    assert!(labels.contains(&"order_id"), "route param missing: {labels:?}");
    assert!(labels.contains(&"amount"), "typed input missing: {labels:?}");
    assert!(labels.contains(&"memo"), "typed input missing: {labels:?}");

    assert_eq!(
        label_with_detail(&items, "order_id").and_then(|i| i.detail.as_deref()),
        Some("route param"),
    );
    assert_eq!(
        label_with_detail(&items, "amount").and_then(|i| i.detail.as_deref()),
        Some("input field"),
    );
}

#[test]
fn no_completion_outside_command_block() {
    // Sanity: cursor sitting in `input.` at the top level (not inside
    // a command) returns None so the global keyword list still fires.
    let source = "input.\n";
    let position = cursor_after_last(&source, "input.");
    assert!(
        input_field_completions(source, position).is_none(),
        "must not fire outside a command block",
    );
}

#[test]
fn partial_identifier_after_input_dot_still_triggers() {
    // Author has typed `input.i` — completion still fires (client
    // filters by partial).
    let source = "\
feature customer
  command save_customer
    route id: ID
    updates Customer
      name = input.i
";
    let position = cursor_after_last(&source, "name = input.i");
    let items = items(source, position);
    let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
    assert!(labels.contains(&"id"), "expected `id` in {labels:?}");
}
