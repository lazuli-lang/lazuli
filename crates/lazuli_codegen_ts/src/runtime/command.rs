//! `defineCommand<I, O>(name, { invalidates, deprecated? })` emitter.
//!
//! Each `RuntimeCommand` lowers to two TS declarations: an input
//! interface (`CreateCustomerInput` etc.) with camelCase keys, and an
//! exported const built via `defineCommand`. The wire JSON contract
//! stays snake_case — `LazuliClient.case-mapper.ts` translates at the
//! boundary, and the path-param `ID` is lowercased to `id` here to
//! match the runtime's `lazuli.FromInput("ID")` lift.
//!
//! Deprecation metadata flows through verbatim when authored, in the
//! exact shape the runtime tooling expects (`since` / `replacement` /
//! `sunset` string fields). The emitter chooses a single-line vs.
//! multi-line `defineCommand<>` signature based on a 60-char heuristic
//! tuned to keep `<feature>.gen.ts` readable in editor wrap-on.

use std::fmt::Write;

use lazuli_codegen_spec::{RuntimeCommand, RuntimeFeature};

use super::header::{format_string_array, write_section_banner};
use super::invalidates::merged_invalidates;
use super::naming::{field_kind_ts, lower_camel, pascal_case};

pub(super) fn write_command(s: &mut String, feature: &RuntimeFeature, command: &RuntimeCommand) {
    // Runtime spec invariant: every feature has at least one resource.
    let Some(resource) = feature.resources.first() else {
        return;
    };
    let resource_pascal = pascal_case(&resource.name);
    let qualified_name = format!("{}.{}", feature.name, command.short_name);
    let input_iface = command_input_struct_name(&command.short_name, &resource_pascal);
    let var_name = command_var_name(&command.short_name, &resource_pascal);

    write_section_banner(s, &[format!("Command: {qualified_name}")]);

    // Input interface keys are camelCase. The wire JSON contract stays
    // snake_case (Go runtime expectation); `LazuliClient` converts at
    // the boundary via `case-mapper.ts`. Includes the path-param `ID`
    // case: previously kept as `ID` to align with `lazuli.FromInput("ID")`
    // — now `id`, and the boundary mapper lifts `id` → `ID` on the
    // wire when the command's `Args` type marks it as a path param.
    writeln!(s, "export interface {input_iface} {{").ok();
    for input in &command.inputs {
        let key = if input.field_name == "ID" {
            "id".to_owned()
        } else {
            lower_camel(&input.field_name)
        };
        writeln!(s, "  {}: {};", key, field_kind_ts(input.kind)).ok();
    }
    writeln!(s, "}}").ok();
    writeln!(s).ok();

    // defineCommand call. Cache-correctness contract: author-declared
    // invalidation targets come first, then the auto-derived same-feature
    // query set. Both sources normalize to the post-B1 wire key
    // `<feature>.<query_name>`, and duplicates are dropped after
    // normalization so an explicit entry keeps priority without losing
    // derived coverage.
    let invalidates = merged_invalidates(feature, command);
    let invalidates_lit = format_string_array(&invalidates);
    if invalidates_lit.len() + qualified_name.len() < 60 {
        writeln!(
            s,
            "export const {var_name} = defineCommand<{input_iface}, {resource_pascal}>("
        )
        .ok();
        writeln!(s, "  \"{qualified_name}\",").ok();
        writeln!(s, "  {{").ok();
        writeln!(s, "    invalidates: {invalidates_lit},").ok();
        write_deprecated_spec(s, command, "    ");
        writeln!(s, "  }},").ok();
        writeln!(s, ");").ok();
    } else {
        writeln!(s, "export const {var_name} = defineCommand<").ok();
        writeln!(s, "  {input_iface},").ok();
        writeln!(s, "  {resource_pascal}").ok();
        writeln!(s, ">(\"{qualified_name}\", {{").ok();
        writeln!(s, "  invalidates: {invalidates_lit},").ok();
        write_deprecated_spec(s, command, "  ");
        writeln!(s, "}});").ok();
    }
    writeln!(s).ok();
}

fn write_deprecated_spec(s: &mut String, command: &RuntimeCommand, indent: &str) {
    let Some(dep) = &command.deprecated else {
        return;
    };
    writeln!(s, "{indent}deprecated: {{").ok();
    if let Some(since) = &dep.since {
        writeln!(s, "{indent}  since: \"{}\",", escape_ts_string(since)).ok();
    }
    if let Some(replacement) = &dep.replacement {
        writeln!(
            s,
            "{indent}  replacement: \"{}\",",
            escape_ts_string(replacement)
        )
        .ok();
    }
    if let Some(sunset) = &dep.sunset {
        writeln!(s, "{indent}  sunset: \"{}\",", escape_ts_string(sunset)).ok();
    }
    writeln!(s, "{indent}}},").ok();
}

fn escape_ts_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn command_input_struct_name(short_name: &str, resource_pascal: &str) -> String {
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

fn command_var_name(short_name: &str, resource_pascal: &str) -> String {
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
