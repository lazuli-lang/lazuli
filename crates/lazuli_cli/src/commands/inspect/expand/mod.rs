//! Canonical-source expander — the text-level `--expand` engine.
//!
//! `lazuli inspect --format=lazuli --expand=...` runs this module's
//! `expand_canonical_source_with`, which walks the source line by
//! line and produces the same text the author wrote *plus* the
//! inferred / expansion-set projections (targets, event groups,
//! lookup shorthands, transition clauses, payload field
//! expansions). This is distinct from the JSON projector in
//! `inspect_json_value`: this stays in surface syntax.
//!
//! Three concerns live here, now split into sibling files:
//!
//! 1. **Canonical-source dispatch** (`expand_canonical_source`,
//!    `expand_canonical_source_with`, `expand_feature_syntax`,
//!    `expand_feature_block`) — top-level entry that drives the
//!    per-feature walker. Stays in `mod.rs`.
//! 2. **Target inference** (sibling `targets.rs`) — when a command
//!    omits an explicit `target`, derive it from the surrounding
//!    feature shape.
//! 3. **Event-group + payload-field collection** (sibling `events.rs`)
//!    — flatten `event_group <prefix>*` blocks into typed event
//!    decls + their effective payload set.
//! 4. **Shorthand-form expansion** (sibling `shorthands.rs`) —
//!    rewrite `query.lookup ... by`, `creates X from input`, and
//!    one-line transition clauses into their canonical multi-line
//!    forms.
//!
//! Shared text-walking primitives live in `text_utils.rs`. Re-exported
//! on the `expand::leading_spaces` path so the per-axis projectors in
//! `inspect/mod.rs` and `inspect/canonical_source.rs` keep their
//! existing imports working.

use super::ExpandSet;

mod events;
mod shorthands;
mod targets;
mod text_utils;

pub(crate) use events::{collect_event_decls, collect_event_groups, EventDecl};
pub(crate) use targets::feature_has_id_lookup;
pub(crate) use text_utils::{
    is_identifier, is_type_name, leading_spaces, namespace_references, parse_ident_list,
};

use events::{
    event_name, expand_payload_entry, is_event_group_start, skip_nested_block,
};
use shorthands::{expand_creates_from_input, expand_lookup_shorthand, expand_transition_clauses};
use targets::infer_local_targets;

#[cfg(test)]
pub(crate) fn expand_canonical_source(source: &str) -> String {
    expand_canonical_source_with(source, ExpandSet::all())
}

pub(super) fn expand_canonical_source_with(source: &str, expansions: ExpandSet) -> String {
    let newline = if source.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let lines: Vec<String> = source.lines().map(str::to_owned).collect();
    let inferred = if expansions.targets {
        infer_local_targets(&lines)
    } else {
        lines
    };
    let expanded = expand_feature_syntax(&inferred, expansions);
    let mut output = expanded.join(newline);

    if source.ends_with('\n') {
        output.push_str(newline);
    }

    output
}

pub(super) fn expand_feature_syntax(lines: &[String], expansions: ExpandSet) -> Vec<String> {
    let mut output = Vec::new();
    let mut index = 0;

    while index < lines.len() {
        if leading_spaces(&lines[index]) == 0 && lines[index].trim_start().starts_with("feature ") {
            let start = index;
            index += 1;

            while index < lines.len() {
                if leading_spaces(&lines[index]) == 0
                    && lines[index].trim_start().starts_with("feature ")
                {
                    break;
                }
                index += 1;
            }

            output.extend(expand_feature_block(&lines[start..index], expansions));
        } else {
            output.push(lines[index].to_owned());
            index += 1;
        }
    }

    output
}

pub(super) fn expand_feature_block(lines: &[String], expansions: ExpandSet) -> Vec<String> {
    let event_groups = collect_event_groups(lines);
    let mut output = Vec::new();
    let mut index = 0;
    let mut in_command = false;
    let mut command_inputs = Vec::new();

    while index < lines.len() {
        let line = &lines[index];
        let trimmed = line.trim_start();
        let leading = leading_spaces(line);

        if leading == 2 && !trimmed.is_empty() {
            in_command = trimmed.starts_with("command ");
            command_inputs.clear();
        }

        if expansions.events && is_event_group_start(line) {
            let next_index = skip_nested_block(lines, index, leading);
            for event in collect_event_decls(&lines[index..next_index]) {
                let indent = " ".repeat(leading);
                let child_indent = " ".repeat(leading + 2);
                output.push(format!("{indent}{} {}", event.kind, event.name));

                for group in &event_groups {
                    if event.name.starts_with(&group.prefix) {
                        for payload in &group.payload {
                            output.push(format!("{child_indent}{}", expand_payload_entry(payload)));
                        }
                    }
                }

                for field in event.payload {
                    output.push(format!("{child_indent}{field}"));
                }
            }
            index = next_index;
            continue;
        }

        if in_command && leading == 4 && trimmed == "input" {
            command_inputs.clear();
        } else if in_command && leading == 4 && trimmed.starts_with("input ") {
            command_inputs = parse_ident_list(trimmed.trim_start_matches("input "));
        }

        if expansions.defaults
            && let Some(expanded) = expand_lookup_shorthand(line)
        {
            output.extend(expanded);
        } else if expansions.defaults
            && let Some(expanded) = expand_creates_from_input(line, &command_inputs)
        {
            output.extend(expanded);
        } else if expansions.defaults
            && let Some(expanded) = expand_transition_clauses(line)
        {
            output.extend(expanded);
        } else {
            output.push(line.to_owned());

            if expansions.events
                && let Some(event_name) = event_name(trimmed)
            {
                for group in &event_groups {
                    if event_name.starts_with(&group.prefix) {
                        let child_indent = " ".repeat(leading + 2);
                        for payload in &group.payload {
                            output.push(format!("{child_indent}{}", expand_payload_entry(payload)));
                        }
                    }
                }
            }
        }

        index += 1;
    }

    output
}
