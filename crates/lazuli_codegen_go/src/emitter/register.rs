//! Per-feature `register.gen.go` — single `func init()` that calls
//! `lazuli.Register(...)` for every Resource, Command, and Query declared
//! on the feature.
//!
//! Closes the first half of `WAR-RUNTIME-COMMAND-01` (the canonical pilot port
//! audit, 2026-05-16). Before this emitter existed, codegen produced
//! `var <cmd> = lazuli.Command[...]{...}` declarations but never invoked
//! `lazuli.Register(&cmd)` — every app had to ship a hand-written
//! `dist/go/<feature>/register.go` with a single-line workaround.
//! Without `Register`, the runtime dispatcher's `Commands()` /
//! `Queries()` snapshots were empty and the HTTP Mux returned 404 for
//! every `/api/v1/c/<command>` call.
//!
//! Records are intentionally excluded — they're typed structs without
//! identity, not registered. Only `Resource` / `Command` / `Query`
//! implement `lazuli.Registerable`.
//!
//! The second half (`@fn`-handler `Effect = lazuli.Returns(...)` wiring
//! when `CommandEffect == None`) is tracked separately and will land in
//! a follow-up.

use lazuli_ir::{Feature, Query};

use super::casing::{lower_camel, pascal_case};
use super::command::{command_var_name, effect_resource_pascal};
use super::patterns::{PATTERN_FEATURE_REGISTER, emit_pattern_header};
use super::printer::GoPrinter;
use super::query::{list_var_name, lookup_var_name, resource_for_query};

/// Emit `<feature>/register.gen.go` when the feature declares at least
/// one Resource, Command, or Query. Returns `None` when the feature has
/// nothing to register so the orchestrator can skip the file entirely
/// (mirrors the resource / enum skip rule — output listing stays
/// signal-rich).
///
/// ## Examples
///
/// ```ignore
/// let go_src = emit_register_file("billing.lzi", &feature);
/// ```
pub fn emit_register_file(source_label: &str, feature: &Feature) -> Option<String> {
    let mut var_refs: Vec<String> = Vec::new();

    for resource in &feature.resources {
        var_refs.push(format!("&{}Resource", lower_camel(&resource.name)));
    }

    for command in &feature.commands {
        let resource_pascal = effect_resource_pascal(&command.effect);
        var_refs.push(format!(
            "&{}",
            command_var_name(&command.name, &resource_pascal)
        ));
    }

    for query in &feature.queries {
        let var = match query {
            Query::List(q) => {
                let axis = resource_axis(feature, &q.name);
                list_var_name(&q.name, &axis)
            }
            Query::Lookup(q) => {
                let axis = resource_axis(feature, &q.name);
                lookup_var_name(&q.name, &axis)
            }
            Query::Sql(q) => lower_camel(&q.name),
            // query.compose: W5 — RegisterFn var name. Default to the
            // lower-camel name (matching the sql shape) so this collection
            // pass stays panic-free for features that mix a compose query
            // with normal queries; the real registration lands in W5.
            Query::Compose(q) => lower_camel(&q.name),
        };
        var_refs.push(format!("&{var}"));
    }

    if var_refs.is_empty() {
        return None;
    }

    let mut p = GoPrinter::new();
    p.banner(
        source_label,
        &super::casing::gen_package_name(&feature.name),
    );
    p.line("import \"lazuli.dev/runtime/lazuli\"");
    p.blank();
    p.line("// Registers every Resource, Command, and Query in this feature with");
    p.line("// the runtime's process-global registry. Required before `lazuli.Mux()`");
    p.line("// can route HTTP / before `lazuli.Commands()` / `lazuli.Queries()`");
    p.line("// return populated snapshots.");
    emit_pattern_header(&mut p, PATTERN_FEATURE_REGISTER);
    p.line("func init() {");
    p.indent();
    p.line("lazuli.Register(");
    p.indent();
    for var_ref in &var_refs {
        p.line(&format!("{var_ref},"));
    }
    p.dedent();
    p.line(")");
    p.dedent();
    p.line("}");

    Some(p.finish())
}

/// Resolve the `resource_name_axis` the query emitter uses to derive var
/// names. Falls back to `"Result"` so the synthesized var matches what
/// `query.rs` already emits when the query references no resource.
fn resource_axis(feature: &Feature, query_name: &str) -> String {
    resource_for_query(feature, query_name)
        .map(|r| pascal_case(&r.name))
        .unwrap_or_else(|| "Result".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resource_axis_falls_back_to_result_when_query_unknown() {
        let source = "feature empty\n  defaults\n    tenancy org\n";
        let parsed = lazuli_syntax::parse_feature_skeletons(source).expect("feature parses");
        let feature = lazuli_analyzer::lower_feature_skeleton(&parsed[0]).expect("feature lowers");
        assert_eq!(resource_axis(&feature, "does_not_exist"), "Result");
    }

    #[test]
    fn emit_register_file_returns_none_for_empty_feature() {
        let source = "feature empty\n  defaults\n    tenancy org\n";
        let parsed = lazuli_syntax::parse_feature_skeletons(source).expect("feature parses");
        let feature = lazuli_analyzer::lower_feature_skeleton(&parsed[0]).expect("feature lowers");
        assert!(emit_register_file("test.lzi", &feature).is_none());
    }
}
