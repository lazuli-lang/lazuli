//! ROUTE-ID-UNUSED-IN-EFFECT-001 (`@correctness.route_id_unused_in_effect`)
//! — a command declares a `route <name>: <Type>` slot but the route value
//! has no path through to the Effect.
//!
//! Pairs with `LAZ-route-id-codegen-go` (cell A1): without this guard, a
//! codegen regression that drops the URL parameter from the input struct
//! goes undetected, and the runtime executes `update`/`delete` with a
//! zero-valued `id` (e.g. wiping or no-op'ing on row `0`).
//!
//! ## What the diagnostic catches
//!
//! For every command whose effect is `Updates(_)` or `Deletes(_)`:
//!
//! 1. Walk `command.route` (the parsed `route <name>: <Type>` slots).
//! 2. For each slot without a `from ctx.<expr>` default — the URL **must**
//!    carry the value — verify that the slot's `name` is reachable from
//!    the Effect:
//!    - it appears in `CommandInput::Typed(slots)` by name (the Go input
//!      struct will then carry the corresponding `<PascalCase>` field), or
//!    - it appears in `CommandInput::Short(names)` (sugar that lowers to
//!      a typed slot of the same name).
//! 3. Otherwise fire the diagnostic — the URL parameter is data the URL
//!    provides that the handler **must** consume; if the input struct
//!    has no field for it, `FromInput("<PascalCase>")` silently reads a
//!    zero value and the WHERE clause matches the wrong row (or no row).
//!
//! `from ctx.<expr>` slots are skipped: the runtime sources those from
//! the request context, not the input struct, so the codegen omitting an
//! input field is intentional.
//!
//! ## Why this lives in doctor, not codegen
//!
//! Codegen is one of N possible targets (Go today; Rust/Yew/Flutter
//! hypothetically tomorrow). Any compliant codegen needs the route slot
//! to flow into its input shape. Declaring the invariant at IR level
//! keeps the boundary intact: the language enforces consistency between
//! `route` and the effect; each backend honours it the way its runtime
//! prefers.

use std::path::{Path, PathBuf};

use lazuli_ir::{Command, CommandEffect, CommandInput, Feature};

// output

/// One ROUTE-ID-UNUSED-IN-EFFECT-001 finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub path: PathBuf,
    pub feature: String,
    pub command: String,
    pub param_name: String,
    /// The PascalCase form the codegen would expect on the input
    /// struct (e.g. `ID` for `route id`, `CustomerID` for
    /// `route customer_id`). Surfaced in the diagnostic message to
    /// pinpoint the missing input field.
    pub pascal_field: String,
    pub effect_kind: &'static str,
}

impl Finding {
    pub const CODE: &'static str = "ROUTE-ID-UNUSED-IN-EFFECT-001";

    pub fn message(&self) -> String {
        format!(
            "command '{}' declares 'route {}: …' but its {} effect does not \
             reference 'input.{}'. The URL parameter will be silently dropped.",
            self.command, self.param_name, self.effect_kind, self.pascal_field
        )
    }
}

// detection

/// Run ROUTE-ID-UNUSED-IN-EFFECT-001 for all commands in one feature.
///
/// `path` is the source `.lzi` file - used to anchor findings; no I/O
/// is performed here. LSP-side entry point; mirrors the
/// `check(feature, path)` shape used by the rest of the correctness
/// catalog so `wire_feature_check!` picks it up uniformly.
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    check_commands(&feature.name, &feature.commands, path)
}

/// CLI-side entry point: runs the same diagnostic against a flat list
/// of commands. `Tier3FeatureFacts` in `lazuli_cli` carries `commands:
/// Vec<Command>` without rebuilding the parent `Feature`, so it calls
/// this directly to avoid synthesizing one just for the check.
pub fn check_commands(feature_name: &str, commands: &[Command], path: &Path) -> Vec<Finding> {
    let mut out = Vec::new();

    for command in commands {
        let effect_kind = match &command.effect {
            CommandEffect::Updates(_) => "updates",
            CommandEffect::Deletes(_) => "deletes",
            _ => continue,
        };

        for slot in &command.route {
            // `route <name>: <Type> from ctx.<expr>` sources the value
            // from the request context — codegen emits `FromCtx(...)`,
            // no input field is needed. Skip.
            if slot.from.is_some() {
                continue;
            }

            // Post-cell-A1: codegen-go now emits every `command.route`
            // slot as a struct field on the input type (PascalCase name,
            // `validate:"required"`), so the Effect's `FromInput(...)`
            // binding resolves against a real field. The legacy check
            // — input block must redeclare the route slot — would now
            // fire on every well-formed `command X route id: ID input
            // foo: Bar` because authors don't repeat the route slot in
            // the typed input block. Keep the check only when the input
            // block declares a slot with the SAME name as a route slot
            // but a different type — that's a real shadow bug. Plain
            // "input doesn't redeclare the route slot" is now silent.
            if let Some(input_type_repr) = input_slot_named(&command.input, &slot.name) {
                let route_type_repr = format!("{:?}", slot.type_ref);
                // Empty repr (CommandInput::Short) carries no type — can't
                // compare, so silent.
                if !input_type_repr.is_empty() && input_type_repr != route_type_repr {
                    out.push(Finding {
                        path: path.to_path_buf(),
                        feature: feature_name.to_owned(),
                        command: command.name.clone(),
                        param_name: slot.name.clone(),
                        pascal_field: pascal_case(&slot.name),
                        effect_kind,
                    });
                }
            }
        }
    }

    out
}

fn input_slot_named(input: &CommandInput, slot_name: &str) -> Option<String> {
    match input {
        CommandInput::Typed(slots) => slots
            .iter()
            .find(|s| s.name == slot_name)
            .map(|s| format!("{:?}", s.type_ref)),
        // Short form names don't carry types, so we can't compare.
        CommandInput::Short(names) => {
            if names.iter().any(|n| n == slot_name) {
                Some(String::new())
            } else {
                None
            }
        }
        CommandInput::Empty => None,
    }
}

// internals

/// True when `slot_name` is named by the command's input shape — either
/// a `Typed(slots)` entry with the same name, or a `Short(names)` entry.
/// Both forms lower to an input-struct field the codegen can bind to
/// `FromInput(<PascalCase>)`.
fn input_consumes_slot(input: &CommandInput, slot_name: &str) -> bool {
    match input {
        CommandInput::Typed(slots) => slots.iter().any(|s| s.name == slot_name),
        CommandInput::Short(names) => names.iter().any(|n| n == slot_name),
        CommandInput::Empty => false,
    }
}

/// PascalCase identifier conversion that honours the Go-runtime acronym
/// catalog (`id` → `ID`, `url` → `URL`, …). Mirrors the closed table
/// in `crates/lazuli_codegen_go/src/emitter/casing.rs` so the diagnostic
/// message matches the field name the Go codegen expects on the input
/// struct. Duplicated rather than imported to keep the doctor crate's
/// dependency graph clean of any codegen backend.
fn pascal_case(s: &str) -> String {
    let words = split_words(s);
    let mut out = String::with_capacity(s.len());
    for word in &words {
        if word.is_empty() {
            continue;
        }
        if is_acronym(word) {
            out.push_str(&word.to_ascii_uppercase());
            continue;
        }
        let mut chars = word.chars();
        if let Some(first) = chars.next() {
            for u in first.to_uppercase() {
                out.push(u);
            }
        }
        out.push_str(chars.as_str());
    }
    out
}

fn is_acronym(word: &str) -> bool {
    matches!(
        word.to_ascii_lowercase().as_str(),
        "id" | "url" | "uri" | "api" | "html" | "json" | "sql" | "ttl" | "uuid"
    )
}

fn split_words(s: &str) -> Vec<String> {
    let mut words: Vec<String> = Vec::new();
    let mut current = String::new();
    let chars: Vec<char> = s.chars().collect();

    for (i, &ch) in chars.iter().enumerate() {
        if ch == '_' || ch == '-' {
            if !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
            continue;
        }
        if ch.is_ascii_uppercase() {
            let prev_lower =
                i > 0 && (chars[i - 1].is_ascii_lowercase() || chars[i - 1].is_ascii_digit());
            let next_lower = i + 1 < chars.len() && chars[i + 1].is_ascii_lowercase();
            if !current.is_empty() && (prev_lower || next_lower) {
                if !next_lower {
                    current.push(ch);
                    continue;
                }
                if !current.is_empty() {
                    words.push(std::mem::take(&mut current));
                }
            }
        }
        current.push(ch);
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

// tests

#[cfg(test)]
mod tests {
    include!("route_id_effect_consistency_tests.rs");
}
