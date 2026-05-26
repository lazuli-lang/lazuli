//! Command block emission for the runtime-form Go emitter, including
//! the typed input struct, the `lazuli.Command` value literal, the
//! effect builder (Creates / Updates / Deletes) and the event emit
//! list. See `super::emit_feature_go` for the orchestrator.

use std::fmt::Write;

use lazuli_codegen_spec::{
    EmitSource, RuntimeCommand, RuntimeEffect, RuntimeEmit, RuntimeEmitKind, RuntimeFeature,
};

use super::{
    command_input_struct_name, command_var_name, field_kind_go, lower_camel, pascal_case,
    write_aligned_struct_rows, write_policy, write_section_banner,
};

pub(super) fn write_command(s: &mut String, feature: &RuntimeFeature, command: &RuntimeCommand) {
    let resource = feature
        .resources
        .first()
        .expect("runtime spec needs at least one resource");
    let resource_pascal = pascal_case(&resource.name);
    let resource_var = format!("{}Resource", lower_camel(&resource.name));
    let qualified_name = format!("{}.{}", feature.name, command.short_name);
    let input_struct = command_input_struct_name(&command.short_name, &resource_pascal);
    let var_name = command_var_name(&command.short_name, &resource_pascal, &resource.name);

    write_section_banner(
        s,
        &[
            format!("Command: {qualified_name}"),
            format!("  command {}", command.short_name),
        ],
    );

    // Input struct — column-aligned like `go fmt`.
    writeln!(s, "type {input_struct} struct {{").ok();
    let rows: Vec<(String, String, String)> = command
        .inputs
        .iter()
        .map(|input| {
            (
                input.field_name.clone(),
                field_kind_go(input.kind).to_owned(),
                format!("`json:\"{}\"`", input.field_name.to_ascii_lowercase()),
            )
        })
        .collect();
    write_aligned_struct_rows(s, &rows);
    writeln!(s, "}}").ok();
    writeln!(s).ok();

    // Command var.
    writeln!(
        s,
        "var {var_name} = lazuli.Command[{input_struct}, {resource_pascal}]{{"
    )
    .ok();
    writeln!(s, "\tName:      \"{qualified_name}\",").ok();
    writeln!(s, "\tResource:  &{resource_var},").ok();
    write_policy(s, "\t", &command.policy_name, &command.policy_atoms);
    // `ir-rate-limit-env-aware` Cell 2 — `RuntimeCommand.rate_limit`
    // remains a plain `String` (separate spec model). Emit the
    // env-qualified struct shape with no `by_env` entries; the runtime
    // helper `Resolve()` returns the default for every env.
    if command.rate_limit.is_empty() {
        writeln!(s, "\tRateLimit:  lazuli.RateLimit{{}},").ok();
    } else {
        writeln!(
            s,
            "\tRateLimit:  lazuli.RateLimit{{Default: \"{}\"}},",
            command.rate_limit
        )
        .ok();
    }
    writeln!(s, "\tAudit:      lazuli.AuditDefault,").ok();
    if !command.validators.is_empty() {
        write!(s, "\tValidators: []lazuli.ValidatorRef{{").ok();
        let parts: Vec<String> = command
            .validators
            .iter()
            .map(|name| format!("lazuli.V(\"{name}\")"))
            .collect();
        write!(s, "{}", parts.join(", ")).ok();
        writeln!(s, "}},").ok();
    }
    write_effect(s, command, &resource_var);
    write_emits(s, &command.emits);
    if !command.invalidates.is_empty() {
        write!(s, "\tInvalidates: []string{{").ok();
        let parts: Vec<String> = command
            .invalidates
            .iter()
            .map(|n| format!("\"{n}\""))
            .collect();
        write!(s, "{}", parts.join(", ")).ok();
        writeln!(s, "}},").ok();
    }
    writeln!(s, "}}").ok();
    writeln!(s).ok();
}

fn write_effect(s: &mut String, command: &RuntimeCommand, resource_var: &str) {
    match command.effect {
        RuntimeEffect::CreatesFromInput => {
            writeln!(
                s,
                "\tEffect: lazuli.Creates(&{resource_var}, lazuli.Bindings{{"
            )
            .ok();
            for input in &command.inputs {
                let column = input.field_name.to_ascii_lowercase();
                writeln!(s, "\t\t\"{column}\": lazuli.FromInput(\"{column}\"),").ok();
            }
            writeln!(s, "\t}}),").ok();
        }
        RuntimeEffect::UpdatesByID => {
            writeln!(s, "\tEffect: lazuli.Updates(&{resource_var},").ok();
            writeln!(
                s,
                "\t\tlazuli.Bindings{{\"id\": lazuli.FromInput(\"ID\")}},"
            )
            .ok();
            // SET = every input that isn't the ID.
            let set_inputs: Vec<&str> = command
                .inputs
                .iter()
                .filter(|i| i.field_name != "ID")
                .map(|i| i.field_name.as_str())
                .collect();
            write!(s, "\t\tlazuli.Bindings{{").ok();
            let parts: Vec<String> = set_inputs
                .iter()
                .map(|n| {
                    format!(
                        "\"{}\": lazuli.FromInput(\"{}\")",
                        n.to_ascii_lowercase(),
                        n
                    )
                })
                .collect();
            write!(s, "{}", parts.join(", ")).ok();
            writeln!(s, "}},").ok();
            writeln!(s, "\t),").ok();
        }
        RuntimeEffect::DeletesByID => {
            writeln!(
                s,
                "\tEffect: lazuli.Deletes(&{resource_var}, lazuli.Bindings{{"
            )
            .ok();
            writeln!(s, "\t\t\"id\": lazuli.FromInput(\"ID\"),").ok();
            writeln!(s, "\t}}),").ok();
        }
    }
}

fn write_emits(s: &mut String, emits: &[RuntimeEmit]) {
    if emits.is_empty() {
        return;
    }
    writeln!(s, "\tEmits: []lazuli.EventEmit{{").ok();
    for emit in emits {
        match &emit.kind {
            RuntimeEmitKind::FromCreates => {
                writeln!(
                    s,
                    "\t\t{{Name: \"{}\", From: lazuli.FromCreates}},",
                    emit.name
                )
                .ok();
            }
            RuntimeEmitKind::Bind(pairs) => {
                writeln!(s, "\t\t{{").ok();
                writeln!(s, "\t\t\tName: \"{}\",", emit.name).ok();
                writeln!(s, "\t\t\tBind: lazuli.Bindings{{").ok();
                for (col, source) in pairs {
                    let src = match source {
                        EmitSource::Input(n) => format!("lazuli.FromInput(\"{n}\")"),
                        EmitSource::Ctx(p) => format!("lazuli.FromCtx(\"{p}\")"),
                    };
                    writeln!(s, "\t\t\t\t\"{col}\": {src},").ok();
                }
                writeln!(s, "\t\t\t}},").ok();
                writeln!(s, "\t\t}},").ok();
            }
        }
    }
    writeln!(s, "\t}},").ok();
}
