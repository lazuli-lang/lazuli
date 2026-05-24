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
    use super::*;
    use lazuli_ir::{
        Assignment, BuiltinType, Command, CommandEffect, CommandKind, Defaults, DeleteEffect, Expr,
        FieldConstraints, Path as IrPath, Policies, PolicyRef, QualifiedName, RouteSlot,
        RouteSlotKind, TypeRef, TypedSlot, UpdateEffect,
    };

    fn qn(name: &str) -> QualifiedName {
        QualifiedName {
            feature: None,
            name: name.to_owned(),
        }
    }

    fn builtin(b: BuiltinType) -> TypeRef {
        TypeRef::Builtin(b)
    }

    fn mk_route(name: &str, ty: BuiltinType, from: Option<&str>) -> RouteSlot {
        RouteSlot {
            name: name.to_owned(),
            type_ref: builtin(ty),
            from: from.map(|s| s.to_owned()),
            kind: RouteSlotKind::Plain,
        }
    }

    fn mk_typed_slot(name: &str, ty: BuiltinType) -> TypedSlot {
        TypedSlot {
            name: name.to_owned(),
            type_ref: builtin(ty),
            required: true,
            constraints: FieldConstraints::default(),
        validate_skip: false,
        }
    }

    fn mk_cmd(
        name: &str,
        route: Vec<RouteSlot>,
        input: CommandInput,
        effect: CommandEffect,
    ) -> Command {
        let kind = match &effect {
            CommandEffect::Updates(_) => CommandKind::Update,
            CommandEffect::Deletes(_) => CommandKind::Delete,
            CommandEffect::Creates(_) => CommandKind::Create,
            _ => CommandKind::Returns,
        };
        Command {
            name: name.to_owned(),
            public_contract: None,
            kind,
            route,
            input,
            target: None,
            lets: vec![],
            effect,
            policy: PolicyRef::None,
            policy_expr: None,
            policy_when_denied: None,
            emits: vec![],
            rate_limit: None,
            audit: None,
            approval: None,
            invalidates: vec![],
            external_calls: vec![],
            timeout: None,
            retry: None,
            idempotency: None,
            write_window: None,
            deprecated: None,
            handler: None,
            tests: None,
            triggers: vec![],
            synthesized_from_cap_file: None,
            previous_names: vec![],
            span_ref: None,
            owner_scope_sql: None,
        }
    }

    fn mk_feature(command: Command) -> Feature {
        Feature {
            name: "customer".into(),
            purpose: None,
            non_goals: vec![],
            context_path: None,
            defaults: Defaults::default(),
            uses: vec![],
            uses_spans: Vec::new(),
            uses_versions: Vec::new(),
            requirements: vec![],
            enums: vec![],
            resources: vec![],
            events: vec![],
            rules: vec![],
            policies: Policies::default(),
            errors: None,
            commands: vec![command],
            apis: vec![],
            records: vec![],
            queries: vec![],
            resume_routers: vec![],
            workflows: vec![],
            jobs: vec![],
            webhooks: vec![],
            notifications: vec![],
            event_groups: vec![],
            tenant_migrations: vec![],
            translation: None,
            pollers: vec![],
            auth: None,
            surfaces: vec![],
            extensions: vec![],
            escape_routes: vec![],
            agents: vec![],
            reports: vec![],
            channels: vec![],
            caches: vec![],
            aggregates: vec![],
            mcp_servers: vec![],
            previous_names: vec![],
            synth_origins: std::collections::BTreeMap::new(),
            span_ref: None,
        }
    }

    fn updates_effect() -> CommandEffect {
        CommandEffect::Updates(UpdateEffect {
            resource: qn("Customer"),
            assignments: vec![Assignment {
                field: "tier".to_owned(),
                value: Expr::Path(IrPath::from_segments(["input", "tier"])),
            }],
        })
    }

    fn deletes_effect() -> CommandEffect {
        CommandEffect::Deletes(DeleteEffect {
            resource: qn("Customer"),
        })
    }

    #[test]
    fn silent_when_input_omits_route_slot_because_codegen_a1_emits_field() {
        // Post-cell-A1: codegen-go emits every `command.route` slot as
        // an input-struct field directly. Authors don't need to repeat
        // the route slot inside the typed input block. The diagnostic
        // stays silent — A1 owns the codegen guarantee; A3 only catches
        // SHADOWING (route slot + same-name input slot with different
        // type), tested below.
        let cmd = mk_cmd(
            "update_tier",
            vec![mk_route("id", BuiltinType::Id, None)],
            CommandInput::Typed(vec![mk_typed_slot("tier", BuiltinType::Text)]),
            updates_effect(),
        );
        let feature = mk_feature(cmd);

        let findings = check(&feature, Path::new("features/customer/customer.lzi"));

        assert!(findings.is_empty(), "expected no findings, got {findings:?}");
    }

    #[test]
    fn silent_when_route_id_with_empty_input_post_a1() {
        // `delete_X route id: ID` lowers to `CommandInput::Empty` (no
        // body fields). Codegen A1 still emits a synthetic input struct
        // carrying the route slot, so the runtime UPDATE/DELETE Effect's
        // `FromInput("ID")` binding resolves. Diagnostic stays silent.
        let cmd = mk_cmd(
            "delete_customer",
            vec![mk_route("id", BuiltinType::Id, None)],
            CommandInput::Empty,
            deletes_effect(),
        );
        let feature = mk_feature(cmd);

        let findings = check(&feature, Path::new("f.lzi"));

        assert!(findings.is_empty(), "expected no findings, got {findings:?}");
    }

    #[test]
    fn silent_when_composite_route_input_partially_redeclares_post_a1() {
        // `route customer_id: ID + tag_id: ID` with input redeclaring
        // only `customer_id` (same type). A1 emits both struct fields.
        // Same-name same-type partial redeclare is a no-op shadow.
        let cmd = mk_cmd(
            "untag_customer",
            vec![
                mk_route("customer_id", BuiltinType::Id, None),
                mk_route("tag_id", BuiltinType::Id, None),
            ],
            CommandInput::Typed(vec![mk_typed_slot("customer_id", BuiltinType::Id)]),
            deletes_effect(),
        );
        let feature = mk_feature(cmd);

        let findings = check(&feature, Path::new("f.lzi"));

        assert!(findings.is_empty(), "expected no findings, got {findings:?}");
    }

    #[test]
    fn positive_shadow_route_id_with_different_typed_input_slot_fires() {
        // True shadow bug: `route id: ID` but the typed input ALSO has
        // an `id` slot with a different type (e.g. Text). Codegen would
        // emit two fields with the same name — illegal Go — OR silently
        // pick one and drop the other. Either way it's a bug worth
        // surfacing.
        let cmd = mk_cmd(
            "shadow_update",
            vec![mk_route("id", BuiltinType::Id, None)],
            CommandInput::Typed(vec![mk_typed_slot("id", BuiltinType::Text)]),
            updates_effect(),
        );
        let feature = mk_feature(cmd);

        let findings = check(&feature, Path::new("f.lzi"));

        assert_eq!(findings.len(), 1, "expected shadow finding, got {findings:?}");
        assert_eq!(findings[0].param_name, "id");
        assert_eq!(Finding::CODE, "ROUTE-ID-UNUSED-IN-EFFECT-001");
    }

    #[test]
    fn negative_updates_with_route_id_consumed_by_typed_input_passes() {
        // Healthy shape: `route id: ID` + the typed input also lists
        // `id` (so the Go input struct has an `ID` field). No finding.
        let cmd = mk_cmd(
            "update_tier",
            vec![mk_route("id", BuiltinType::Id, None)],
            CommandInput::Typed(vec![
                mk_typed_slot("id", BuiltinType::Id),
                mk_typed_slot("tier", BuiltinType::Text),
            ]),
            updates_effect(),
        );
        let feature = mk_feature(cmd);

        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }

    #[test]
    fn negative_route_slot_with_ctx_default_passes() {
        // `route customer_id: ID from ctx.user.id` — the runtime
        // sources the value from context, not from the input struct,
        // so the input is allowed to omit it.
        let cmd = mk_cmd(
            "update_self",
            vec![mk_route(
                "customer_id",
                BuiltinType::Id,
                Some("ctx.user.id"),
            )],
            CommandInput::Typed(vec![mk_typed_slot("tier", BuiltinType::Text)]),
            updates_effect(),
        );
        let feature = mk_feature(cmd);

        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }

    #[test]
    fn negative_short_input_covering_route_slot_passes() {
        // `input id, tier` (short form) lists `id` — once expanded by
        // the analyzer it'll be a typed slot for `id`. No finding.
        let cmd = mk_cmd(
            "update_tier",
            vec![mk_route("id", BuiltinType::Id, None)],
            CommandInput::Short(vec!["id".to_owned(), "tier".to_owned()]),
            updates_effect(),
        );
        let feature = mk_feature(cmd);

        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }

    #[test]
    fn negative_creates_command_with_route_slot_does_not_fire() {
        // Creates effect doesn't use a WHERE binding — the route slot
        // shape on a `create` command is unusual but not the bug class
        // this diagnostic guards against.
        let cmd = mk_cmd(
            "create_customer",
            vec![mk_route("tenant_id", BuiltinType::Id, None)],
            CommandInput::Typed(vec![mk_typed_slot("name", BuiltinType::Text)]),
            CommandEffect::Creates(lazuli_ir::CreateEffect {
                resource: qn("Customer"),
                from_input: true,
                assignments: vec![],
            }),
        );
        let feature = mk_feature(cmd);

        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }

    #[test]
    fn negative_command_without_route_does_not_fire() {
        let cmd = mk_cmd(
            "update_tier",
            vec![],
            CommandInput::Typed(vec![mk_typed_slot("tier", BuiltinType::Text)]),
            updates_effect(),
        );
        let feature = mk_feature(cmd);

        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }

    #[test]
    fn pascal_case_handles_id_acronym() {
        assert_eq!(pascal_case("id"), "ID");
        assert_eq!(pascal_case("customer_id"), "CustomerID");
        assert_eq!(pascal_case("tag_id"), "TagID");
    }
}
