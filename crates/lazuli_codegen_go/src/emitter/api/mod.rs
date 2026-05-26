//! Cell G5 - `Api` kind emission. Walks every `Api` declared on a
//! feature and emits typed args plus an API contract value into
//! `<feature>/api.gen.go`.
//!
//! API values use the real Lazuli Go `lazuli.Api[I, O]` contract. Handler
//! population remains an extension-point concern; generated literals leave
//! `Handler` unset and pin the intended registration site in a comment.
//!
//! Determinism: APIs are sorted by name, route args preserve path
//! order, and imports flow through `ImportSet`.

mod format;
mod output_type;
mod route_args;

use lazuli_ir::{Api, Feature, Gate};

use super::casing::lower_camel;
use super::cross_feature::CrossFeatureIndex;
use super::imports::ImportSet;
use super::module::EmitContext;
use super::printer::GoPrinter;
use super::types::TypeCtx;

use format::{api_args_type_name, escape_string, method_const_name, write_section_banner};
use output_type::{go_type_for_api_output, register_imports_for_api_output};
use route_args::{emit_args_struct, route_args};

/// Emit `<feature>/api.gen.go` for a feature, or `None` when the
/// feature declares no APIs.
pub fn emit_api_file(
    source_label: &str,
    feature: &Feature,
    module_name: &str,
    cross_index: &CrossFeatureIndex<'_>,
    emit_ctx: &EmitContext<'_>,
) -> Option<String> {
    if feature.apis.is_empty() {
        return None;
    }

    let mut p = GoPrinter::new();
    let mut imports = ImportSet::new();
    imports.add("lazuli.dev/runtime/lazuli");

    let type_ctx = TypeCtx {
        current_feature: feature.name.as_str(),
        module_name,
        cross_index,
    };

    let mut apis: Vec<&Api> = feature.apis.iter().collect();
    apis.sort_by(|a, b| a.name.cmp(&b.name));

    for api in &apis {
        register_imports_for_api_output(&api.output, &type_ctx, &mut imports);
    }
    // PG.C.2 — gated APIs carry a `Prelude: []billing.GateRef{...}`
    // field on the lazuli.Api value; `api.Invoke` runs the prelude
    // via `lazuli.RunPrelude` before the handler. Import `billing`
    // only when any api in the file declares gates.
    let any_gated = apis
        .iter()
        .any(|api| !emit_ctx.gates_for("api", &api.name).is_empty());
    if any_gated {
        imports.add("lazuli.dev/runtime/lazuli/billing");
        imports.add(&format!("{module_name}/plan"));
    }

    p.banner(
        source_label,
        &super::casing::gen_package_name(&feature.name),
    );
    imports.emit(&mut p);
    p.blank();

    let mut first_block = true;
    for api in &apis {
        if !first_block {
            p.blank();
        }
        first_block = false;
        emit_api(&mut p, feature, api, &type_ctx, emit_ctx);
    }

    Some(p.finish())
}

fn emit_api(
    p: &mut GoPrinter,
    feature: &Feature,
    api: &Api,
    ctx: &TypeCtx<'_>,
    emit_ctx: &EmitContext<'_>,
) {
    let qualified_name = format!("{}.{}", feature.name, api.name);
    // Suffix `Api` on the var + args type so `api <name>` and
    // `query.list <name>` declared on the same feature don't collide
    // at the package scope. Surfaced by pilot item.lzi where
    // `query.list search` and `api search` both emitted `var search`
    // / `type SearchArgs` and the resulting Go failed to compile.
    let args_type = api_args_type_name(&api.name);
    let var_name = format!("{}Api", lower_camel(&api.name));
    let (output_type, _import) = go_type_for_api_output(&api.output, ctx);

    write_section_banner(
        p,
        &[
            format!("Api: {qualified_name}"),
            format!("  api {}", api.name),
        ],
    );

    let args = route_args(&api.path);
    emit_args_struct(p, &args_type, &args);
    p.blank();

    if !args.is_empty() {
        p.line("// TODO(ir): Api path parameters have no typed IR slots; args are inferred from the path.");
    }
    p.line(&format!(
        "var {var_name} = lazuli.Api[{args_type}, {output_type}]{{"
    ));
    p.indent();

    let mut kv_rows: Vec<(String, String)> = vec![
        (
            "Name:".to_owned(),
            format!("\"{}\",", escape_string(&qualified_name)),
        ),
        (
            "Feature:".to_owned(),
            format!("\"{}\",", escape_string(&feature.name)),
        ),
        (
            "Method:".to_owned(),
            format!("{},", method_const_name(api.method)),
        ),
        (
            "Path:".to_owned(),
            format!("\"{}\",", escape_string(&api.path)),
        ),
        (
            "Policy:".to_owned(),
            super::command::format_policy_with_expr_public(
                &api.policy,
                api.policy_expr.as_ref(),
                Some(&feature.policies),
            ),
        ),
    ];
    if let Some(rate_limit) = &api.rate_limit {
        // `ir-rate-limit-env-aware` Cell 2 — emit env-qualified struct.
        // Printer is at indent_level=1 inside the Api literal.
        kv_rows.push((
            "RateLimit:".to_owned(),
            format!(
                "{},",
                super::command::format_rate_limit_struct(rate_limit, "\t")
            ),
        ));
    }
    if let Some(deprecation) = &api.deprecated {
        kv_rows.push((
            "Deprecation:".to_owned(),
            format!(
                "&lazuli.Deprecation{{Since: \"{}\", Replacement: \"{}\", Sunset: \"{}\"}},",
                escape_string(deprecation.since.as_deref().unwrap_or("")),
                escape_string(&super::command::format_deprecation_replacement(
                    &feature.name,
                    deprecation.replacement.as_ref()
                )),
                escape_string(deprecation.sunset.as_deref().unwrap_or(""))
            ),
        ));
    }

    let key_width = kv_rows.iter().map(|(k, _)| k.len()).max().unwrap_or(0);
    for (key, value) in &kv_rows {
        let pad = key_width.saturating_sub(key.len());
        p.line(&format!("{}{} {}", key, " ".repeat(pad), value));
    }
    emit_gate_annotations(p, emit_ctx.gates_for("api", &api.name));
    p.dedent();
    p.line("}");
    p.blank();
    // Self-register the typed API value into the global registry so
    // `lazuli.ValidateApiHandlers()` can see it. The user is
    // responsible for assigning `<var>.Handler = ...` in their
    // application code (typically `main.go`) BEFORE calling
    // `ValidateApiHandlers` — otherwise validation fails fast with a
    // listing of unwired endpoints instead of the server silently
    // returning `500: api handler not set` on the first hit.
    //
    // See review bug #1 (2026-05-15): the previous emission left an
    // inert `// TODO(extension-points): ...` comment with no
    // registration; the endpoint vanished into the void unless the
    // user happened to call `RegisterApi` themselves.
    p.line("//lazuli:pattern api_register v1");
    p.line(&format!(
        "func init() {{ lazuli.RegisterApi(&{var_name}) }}"
    ));
    p.line(&format!(
        "// Wire {var_name}.Handler in your application code, then call"
    ));
    p.line("// `lazuli.ValidateApiHandlers()` at startup to fail fast on omissions.");
}

/// PG.C.2 — emit the `Prelude: []billing.GateRef{...}` field on a
/// `lazuli.Api[I, O]` value. `Api.Invoke` consults the slice via
/// `lazuli.RunPrelude` before invoking the handler. Empty slice →
/// no field emitted.
fn emit_gate_annotations(p: &mut GoPrinter, gates: &[Gate]) {
    if gates.is_empty() {
        return;
    }
    p.line("Prelude: []billing.GateRef{");
    p.indent();
    for gate in gates {
        match gate {
            Gate::Behind { feature } => {
                p.line(&format!(
                    "{{Kind: billing.GateBehind, Name: {:?}}},",
                    feature
                ));
            }
            Gate::Quota { limit } => {
                p.line(&format!("{{Kind: billing.GateQuota, Name: {:?}}},", limit));
            }
        }
    }
    p.dedent();
    p.line("},");
}

#[cfg(test)]
mod feature_emit_tests;
#[cfg(test)]
mod tests;
