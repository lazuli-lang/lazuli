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

use lazuli_ir::{Api, Feature, Gate, HttpMethod, PolicyRef, TypeRef};

use super::casing::{lower_camel, pascal_case};
use super::cross_feature::CrossFeatureIndex;
use super::imports::ImportSet;
use super::module::EmitContext;
use super::printer::GoPrinter;
use super::types::{self, TypeCtx};

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

    p.banner(source_label, &feature.name);
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
    let args_type = api_args_type_name(&api.name);
    let var_name = lower_camel(&api.name);
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
            format!("\"{}\",", escape_string(&api.name)),
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
            super::command::format_policy_with_expr_public(&api.policy, api.policy_expr.as_ref()),
        ),
    ];
    if let Some(rate_limit) = &api.rate_limit {
        kv_rows.push((
            "RateLimit:".to_owned(),
            format!("\"{}\",", escape_string(rate_limit)),
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

fn emit_args_struct(p: &mut GoPrinter, name: &str, args: &[ApiArg]) {
    if args.is_empty() {
        p.line(&format!("type {name} struct{{}}"));
        return;
    }

    p.line(&format!("type {name} struct {{"));
    p.indent();
    let rows: Vec<(String, String, String)> = args
        .iter()
        .map(|arg| {
            (
                pascal_case(&arg.name),
                arg.go_type.clone(),
                format!("`json:\"{}\"`", arg.name),
            )
        })
        .collect();
    let row_refs: Vec<(&str, &str, &str)> = rows
        .iter()
        .map(|(name, ty, tag)| (name.as_str(), ty.as_str(), tag.as_str()))
        .collect();
    p.aligned_struct_rows(&row_refs);
    p.dedent();
    p.line("}");
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ApiArg {
    name: String,
    go_type: String,
}

fn route_args(path: &str) -> Vec<ApiArg> {
    let mut args = Vec::new();
    let mut seen = Vec::<String>::new();

    for name in path_params(path) {
        if name.is_empty() || seen.iter().any(|existing| existing == &name) {
            continue;
        }
        let go_type = infer_route_arg_type(&name).to_owned();
        seen.push(name.clone());
        args.push(ApiArg { name, go_type });
    }

    args
}

fn path_params(path: &str) -> Vec<String> {
    let mut out = Vec::new();
    for segment in path.split('/') {
        if let Some(raw) = segment.strip_prefix(':') {
            let name = raw
                .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
                .next()
                .unwrap_or("")
                .trim();
            if !name.is_empty() {
                out.push(name.to_owned());
            }
            continue;
        }
        out.extend(brace_params(segment));
    }
    out
}

fn brace_params(path: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = path;
    while let Some(start) = rest.find('{') {
        let after_start = &rest[start + 1..];
        let Some(end) = after_start.find('}') else {
            break;
        };
        out.push(after_start[..end].trim().to_owned());
        rest = &after_start[end + 1..];
    }
    out
}

fn infer_route_arg_type(name: &str) -> &'static str {
    let lower = name.to_ascii_lowercase();
    if lower == "id" || lower.ends_with("_id") {
        "lazuli.ID"
    } else {
        "string"
    }
}

fn go_type_for_api_output(type_ref: &TypeRef, ctx: &TypeCtx<'_>) -> (String, Option<String>) {
    match type_ref {
        TypeRef::Capability(_) => types::go_type_for(type_ref, ctx),
        TypeRef::Many(inner) => {
            let (inner_go, import) = go_type_for_api_output(inner, ctx);
            (format!("[]{}", inner_go), import)
        }
        TypeRef::UserDefined(qname) if is_cap_file_literal(&qname.name) => storage_file_ref_type(),
        TypeRef::Unresolved(raw) if is_cap_file_literal(raw) => storage_file_ref_type(),
        _ => types::go_type_for(type_ref, ctx),
    }
}

fn storage_file_ref_type() -> (String, Option<String>) {
    (
        "storage.FileRef".to_owned(),
        Some("lazuli.dev/runtime/lazuli/storage".to_owned()),
    )
}

// API outputs may still carry the authored decorator text on older
// analyzer paths; normalize it before the generic identifier sanitizer.
fn is_cap_file_literal(raw: &str) -> bool {
    let raw = raw.trim();
    raw == "@cap.File" || raw.starts_with("@cap.File(")
}

fn register_imports_for_api_output(type_ref: &TypeRef, ctx: &TypeCtx<'_>, imports: &mut ImportSet) {
    let (_go, import) = go_type_for_api_output(type_ref, ctx);
    if let Some(path) = import {
        imports.add(&path);
    }
    if let TypeRef::Many(inner) = type_ref {
        register_imports_for_api_output(inner, ctx, imports);
    }
}

fn format_policy(policy: &PolicyRef) -> String {
    match policy {
        PolicyRef::Local(name) => format!(
            "lazuli.Policy{{Name: \"@policy.{}\"}},",
            escape_string(name)
        ),
        PolicyRef::Atom(atom) => {
            let stripped = atom.strip_prefix('@').unwrap_or(atom);
            if let Some(local) = stripped.strip_prefix("policy.") {
                return format!(
                    "lazuli.Policy{{Name: \"@policy.{}\"}},",
                    escape_string(local)
                );
            }
            let mut parts = stripped.splitn(2, '.');
            let ns = parts.next().unwrap_or("");
            let nm = parts.next().unwrap_or("");
            format!(
                "lazuli.Policy{{Name: \"@{}\", Atoms: []lazuli.PolicyAtom{{{{Namespace: \"{}\", Name: \"{}\"}}}}}},",
                escape_string(stripped),
                escape_string(ns),
                escape_string(nm)
            )
        }
        PolicyRef::External { feature, name } => format!(
            "lazuli.Policy{{Name: \"{}.policy.{}\"}},",
            escape_string(feature),
            escape_string(name)
        ),
        PolicyRef::Unresolved(raw) => {
            format!("lazuli.Policy{{Name: \"{}\"}},", escape_string(raw))
        }
        PolicyRef::None => "lazuli.Policy{},".to_owned(),
    }
}

fn method_const_name(method: HttpMethod) -> &'static str {
    match method {
        HttpMethod::Get => "lazuli.MethodGet",
        HttpMethod::Post => "lazuli.MethodPost",
        HttpMethod::Put => "lazuli.MethodPut",
        HttpMethod::Patch => "lazuli.MethodPatch",
        HttpMethod::Delete => "lazuli.MethodDelete",
    }
}

fn api_args_type_name(name: &str) -> String {
    format!("{}Args", pascal_case(name))
}

fn write_section_banner(p: &mut GoPrinter, lines: &[String]) {
    let rule = "-".repeat(76);
    p.line(&format!("// {rule}"));
    for line in lines {
        p.line(&format!("// {line}"));
    }
    p.line(&format!("// {rule}"));
    p.blank();
}

fn escape_string(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            _ => out.push(ch),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        AppManifest, BuiltinType, CapabilityRef, Defaults, FileCapability, FileSize,
        FileSizeLiteral, FileVisibility, MimeType, Module, PathRef, Policies, QualifiedName,
        Record, Resource,
    };

    fn base_feature(name: &str) -> Feature {
        Feature {
            name: name.to_owned(),
            purpose: None,
            non_goals: Vec::new(),
            context_path: None,
            defaults: Defaults {
                tenancy: None,
                timestamps: false,
                policy: None,
            },
            uses: Vec::new(),
            requirements: Vec::new(),
            enums: Vec::new(),
            resources: Vec::new(),
            events: Vec::new(),
            rules: Vec::new(),
            policies: Policies {
                categories: Vec::new(),
                fields: Vec::new(),
                span_ref: None,
            },
            commands: Vec::new(),
            apis: Vec::new(),
            records: Vec::new(),
            queries: Vec::new(),
            workflows: Vec::new(),
            jobs: Vec::new(),
            webhooks: Vec::new(),
            notifications: Vec::new(),
            event_groups: Vec::new(),
            tenant_migrations: Vec::new(),
            translation: None,
            pollers: vec![],
            auth: None,
            surfaces: Vec::new(),
            extensions: Vec::new(),
            escape_routes: Vec::new(),
            agents: Vec::new(),
            reports: Vec::new(),
            previous_names: Vec::new(),
            span_ref: None,
        }
    }

    fn minimal_app() -> AppManifest {
        AppManifest {
            name: "test".to_owned(),
            title: None,
            version: None,
            lazuli_version: None,
            targets: Vec::new(),
            default_locale: None,
            default_timezone: None,
            auth_failed_redirect: None,
            not_found: None,
            uses: Vec::new(),
            packs: Vec::new(),
            bindings: Vec::new(),
            architecture: None,
            services: Vec::new(),
            communication: None,
            environments: Vec::new(),
            urls: Vec::new(),
            cors: None,
            env: Vec::new(),
            integrations: Vec::new(),
            capabilities: Vec::new(),
            runtime: Vec::new(),
            deploy: None,
            logging: None,
            tracing: None,
            observability: None,
            locale: None,
            encryption_bindings: Vec::new(),
            span_ref: None,
        }
    }

    fn module_with_features(features: Vec<Feature>) -> Module {
        Module {
            workspace: None,
            contracts: Vec::new(),
            app: Some(minimal_app()),
            registry: None,
            profiles: Vec::new(),
            design: None,
            rbac: None,
            features,
        }
    }

    fn emit_from_module(module: &Module, feature_index: usize) -> Option<String> {
        let index = CrossFeatureIndex::build(module);
        let emit_ctx = EmitContext::no_source("feature/api.gen.go");
        emit_api_file(
            "examples/x.lzi",
            &module.features[feature_index],
            "lazuli/test",
            &index,
            &emit_ctx,
        )
    }

    fn emit(feature: &Feature) -> Option<String> {
        let module = module_with_features(vec![feature.clone()]);
        emit_from_module(&module, 0)
    }

    fn simple_api(name: &str, method: HttpMethod, path: &str, output: TypeRef) -> Api {
        Api {
            name: name.to_owned(),
            method,
            path: path.to_owned(),
            policy: PolicyRef::Local("read".to_owned()),
            policy_expr: None,
            rate_limit: None,
            output,
            handler: PathRef::authored(format!("./handlers/{name}.go")),
            locale_negotiate: None,
            span_ref: None,
        }
    }

    fn simple_record(name: &str) -> Record {
        Record {
            name: name.to_owned(),
            fields: Vec::new(),
            discriminator_field: None,
            span_ref: None,
        }
    }

    fn simple_resource(name: &str) -> Resource {
        Resource {
            name: name.to_owned(),
            tenancy: None,
            soft_delete: false,
            timestamps: None,
            fields: Vec::new(),
            constraints: Vec::new(),
            validate: None,
            validates: Vec::new(),
            retention: None,
            previous_names: Vec::new(),
            span_ref: None,
            lifecycle: None,
        }
    }

    fn make_file_capability(
        literal: FileSizeLiteral,
        accept: Vec<(&str, &str)>,
        visibility: Option<FileVisibility>,
        signed_ttl: Option<&str>,
    ) -> FileCapability {
        FileCapability {
            max_size: FileSize {
                bytes: literal.bytes(),
                literal,
            },
            accept: accept
                .into_iter()
                .map(|(family, subtype)| MimeType {
                    family: family.to_owned(),
                    subtype: subtype.to_owned(),
                })
                .collect(),
            visibility,
            signed_ttl: signed_ttl.map(str::to_owned),
        }
    }

    #[test]
    fn empty_feature_returns_none() {
        let feature = base_feature("customer");
        assert!(emit(&feature).is_none());
    }

    #[test]
    fn canonical_file_api_emits_real_type_storage_and_register_placeholder() {
        let mut feature = base_feature("customer");
        let mut api = simple_api(
            "customer_export",
            HttpMethod::Get,
            "/api/customers/export",
            TypeRef::Builtin(BuiltinType::CapFile),
        );
        api.policy = PolicyRef::Local("global_read".to_owned());
        api.rate_limit = Some("10 per hour per user".to_owned());
        api.handler = PathRef::authored("./api/export_customers.go");
        feature.apis.push(api);

        let out = emit(&feature).expect("must emit");
        assert!(out.contains("// Code generated by lazuli; DO NOT EDIT."));
        assert!(out.contains("package customer"));
        assert!(out.contains("\"lazuli.dev/runtime/lazuli\""));
        assert!(out.contains("\"lazuli.dev/runtime/lazuli/storage\""));
        assert!(!out.contains("\"lazuli/test/customer/api\""));
        assert!(out.contains("type CustomerExportArgs struct{}"));
        assert!(
            out.contains("var customerExport = lazuli.Api[CustomerExportArgs, storage.FileRef]{")
        );
        assert!(!out.contains("var customerExport = struct {"));
        assert!(!out.contains("TODO(runtime):"));
        // Codegen now emits a `func init()` that registers the typed
        // API value, plus an inline hint pointing to
        // `ValidateApiHandlers`. The previous inert `TODO` comment
        // never registered anything (review bug #1, 2026-05-15).
        assert!(out.contains("func init() { lazuli.RegisterApi(&customerExport) }"));
        assert!(out.contains("// Wire customerExport.Handler in your application code"));
        assert!(out.contains("lazuli.ValidateApiHandlers()"));
        assert!(
            !out.contains("// TODO(extension-points)"),
            "legacy TODO comment must be gone"
        );
        assert!(out.contains("Method:    lazuli.MethodGet,"));
        assert!(out.contains("Policy:    lazuli.Policy{Name: \"@policy.global_read\"},"));
        assert!(out.contains("RateLimit: \"10 per hour per user\","));
        // Generated Api value still leaves Handler unset — the user
        // wires it post-codegen. Validation happens at boot via
        // `ValidateApiHandlers()`.
        assert!(!out.contains("\tHandler:"));
    }

    #[test]
    fn typed_cap_file_api_output_uses_storage_file_ref() {
        let mut feature = base_feature("customer");
        feature.apis.push(simple_api(
            "customer_export",
            HttpMethod::Get,
            "/api/customers/export",
            TypeRef::Capability(CapabilityRef::File(make_file_capability(
                FileSizeLiteral::Mb(100),
                vec![("text", "csv")],
                Some(FileVisibility::Signed),
                Some("1h"),
            ))),
        ));

        let out = emit(&feature).expect("must emit");
        assert!(out.contains("\"lazuli.dev/runtime/lazuli/storage\""));
        assert!(
            out.contains("var customerExport = lazuli.Api[CustomerExportArgs, storage.FileRef]{")
        );
        assert!(!out.contains("_cap_File"));
    }

    #[test]
    fn cap_file_literal_api_output_uses_storage_file_ref() {
        let mut feature = base_feature("customer");
        feature.apis.push(simple_api(
            "customer_export",
            HttpMethod::Get,
            "/api/customers/export",
            TypeRef::UserDefined(QualifiedName {
                feature: None,
                name: "@cap.File(max_size:100mb,accept:text/csv,visibility:signed,signed_ttl:1h)"
                    .to_owned(),
            }),
        ));

        let out = emit(&feature).expect("must emit");
        assert!(out.contains("\"lazuli.dev/runtime/lazuli/storage\""));
        assert!(
            out.contains("var customerExport = lazuli.Api[CustomerExportArgs, storage.FileRef]{")
        );
        assert!(!out.contains("_cap_File"));
    }

    #[test]
    fn path_params_emit_inferred_args_and_method_constant() {
        let mut feature = base_feature("customer");
        feature.records.push(simple_record("CustomerSummary"));
        let mut api = simple_api(
            "customer_summary",
            HttpMethod::Get,
            "/api/customer/{id}/summary",
            TypeRef::UserDefined(QualifiedName {
                feature: None,
                name: "CustomerSummary".to_owned(),
            }),
        );
        api.rate_limit = Some("60 per minute per user".to_owned());
        feature.apis.push(api);

        let out = emit(&feature).expect("must emit");
        assert!(out.contains("type CustomerSummaryArgs struct {"));
        assert!(out.contains("ID lazuli.ID `json:\"id\"`"));
        assert!(out.contains("// TODO(ir): Api path parameters have no typed IR slots"));
        assert!(out.contains(
            r#"var customerSummary = lazuli.Api[CustomerSummaryArgs, CustomerSummary]{
	Name:      "customer_summary",
	Feature:   "customer",
	Method:    lazuli.MethodGet,
	Path:      "/api/customer/{id}/summary",
	Policy:    lazuli.Policy{Name: "@policy.read"},
	RateLimit: "60 per minute per user",
}

func init() { lazuli.RegisterApi(&customerSummary) }
// Wire customerSummary.Handler in your application code, then call
// `lazuli.ValidateApiHandlers()` at startup to fail fast on omissions."#
        ));
        assert!(!out.contains("TODO(runtime):"));
        // Handler still not inlined — extension-point pattern. But the
        // value is now registered via init(), unlike the prior
        // emission that silently dropped the endpoint.
        assert!(!out.contains("\tHandler:"));
    }

    #[test]
    fn method_consts_cover_runtime_catalog() {
        assert_eq!(method_const_name(HttpMethod::Get), "lazuli.MethodGet");
        assert_eq!(method_const_name(HttpMethod::Post), "lazuli.MethodPost");
        assert_eq!(method_const_name(HttpMethod::Put), "lazuli.MethodPut");
        assert_eq!(method_const_name(HttpMethod::Patch), "lazuli.MethodPatch");
        assert_eq!(method_const_name(HttpMethod::Delete), "lazuli.MethodDelete");
    }

    #[test]
    fn cross_feature_output_emits_qualified_type_and_import() {
        let mut customer = base_feature("customer");
        customer.apis.push(simple_api(
            "owner_profile",
            HttpMethod::Post,
            "/api/customer/{owner_id}/profile",
            TypeRef::UserDefined(QualifiedName {
                feature: None,
                name: "UserProfile".to_owned(),
            }),
        ));

        let mut org = base_feature("org");
        org.records.push(simple_record("UserProfile"));

        let module = module_with_features(vec![customer, org]);
        let out = emit_from_module(&module, 0).expect("must emit");
        assert!(out.contains("\"lazuli/test/org\""));
        assert!(out.contains("var ownerProfile = lazuli.Api[OwnerProfileArgs, org.UserProfile]{"));
        assert!(out.contains("Method:  lazuli.MethodPost,"));
        assert!(out.contains("OwnerID lazuli.ID `json:\"owner_id\"`"));
    }

    #[test]
    fn gated_api_emits_real_prelude_field_and_billing_imports() {
        // PG.C.2 — gated APIs lift the wave-4 comment annotation into
        // a real `Prelude: []billing.GateRef{...}` field that
        // `Api.Invoke` consults via `lazuli.RunPrelude`. Billing and
        // <module>/plan imports appear when any api carries gates.
        let mut feature = base_feature("billing");
        feature.records.push(simple_record("Invoice"));
        feature.apis.push(simple_api(
            "issue_invoice",
            HttpMethod::Post,
            "/api/invoices",
            TypeRef::UserDefined(QualifiedName {
                feature: None,
                name: "Invoice".to_owned(),
            }),
        ));

        let mut gates: std::collections::BTreeMap<String, Vec<lazuli_ir::Gate>> =
            std::collections::BTreeMap::new();
        gates.insert(
            "billing/api:issue_invoice".to_owned(),
            vec![
                lazuli_ir::Gate::Behind {
                    feature: "issue_invoice".to_owned(),
                },
                lazuli_ir::Gate::Quota {
                    limit: "invoices_per_month".to_owned(),
                },
            ],
        );
        let module = module_with_features(vec![feature]);
        let cross_index = CrossFeatureIndex::build(&module);
        let emit_ctx =
            EmitContext::for_feature(None, "billing-app", "billing", "billing/api.gen.go")
                .with_gates(Some(&gates));
        let out = emit_api_file(
            "examples/billing.lzi",
            &module.features[0],
            "billing-app",
            &cross_index,
            &emit_ctx,
        )
        .expect("must emit");

        assert!(
            out.contains("\"lazuli.dev/runtime/lazuli/billing\""),
            "billing import missing:\n{out}"
        );
        assert!(
            out.contains("\"billing-app/plan\""),
            "plan import missing:\n{out}"
        );
        assert!(
            out.contains("Prelude: []billing.GateRef{"),
            "Prelude field missing:\n{out}"
        );
        assert!(
            out.contains("{Kind: billing.GateBehind, Name: \"issue_invoice\"},"),
            "behind-gate row missing:\n{out}"
        );
        assert!(
            out.contains("{Kind: billing.GateQuota, Name: \"invoices_per_month\"},"),
            "quota-gate row missing:\n{out}"
        );
    }

    #[test]
    fn ungated_api_emits_no_prelude_or_billing_import() {
        // PG.C.2 backward-compat — APIs without gates emit
        // byte-equivalent wave-3 output (no Prelude field, no
        // billing import).
        let mut feature = base_feature("customer");
        feature.records.push(simple_record("Customer"));
        feature.apis.push(simple_api(
            "list_customers",
            HttpMethod::Get,
            "/api/customers",
            TypeRef::UserDefined(QualifiedName {
                feature: None,
                name: "Customer".to_owned(),
            }),
        ));
        let out = emit(&feature).expect("must emit");
        assert!(
            !out.contains("Prelude:"),
            "no Prelude when no gates:\n{out}"
        );
        assert!(
            !out.contains("\"lazuli.dev/runtime/lazuli/billing\""),
            "no billing import when no gates:\n{out}"
        );
    }

    #[test]
    fn deterministic_across_runs_and_sorts_by_name() {
        let mut feature = base_feature("customer");
        feature.resources.push(simple_resource("Customer"));
        feature.apis.push(simple_api(
            "zebra",
            HttpMethod::Delete,
            "/api/zebra",
            TypeRef::UserDefined(QualifiedName {
                feature: None,
                name: "Customer".to_owned(),
            }),
        ));
        feature.apis.push(simple_api(
            "alpha",
            HttpMethod::Get,
            "/api/alpha",
            TypeRef::UserDefined(QualifiedName {
                feature: None,
                name: "Customer".to_owned(),
            }),
        ));

        let a = emit(&feature).expect("must emit");
        let b = emit(&feature).expect("must emit");
        assert_eq!(a, b);

        let alpha_pos = a.find("Api: customer.alpha").expect("alpha banner");
        let zebra_pos = a.find("Api: customer.zebra").expect("zebra banner");
        assert!(alpha_pos < zebra_pos);
    }
}

#[cfg(test)]
mod feature_emit_tests {
    use super::*;
    use lazuli_ir::{Api, BuiltinType, Defaults, Module, PathRef, Policies, TypeRef};

    fn feature_with_api() -> Feature {
        Feature {
            name: "catalog".to_owned(),
            purpose: None,
            non_goals: Vec::new(),
            context_path: None,
            defaults: Defaults {
                tenancy: None,
                timestamps: false,
                policy: None,
            },
            uses: Vec::new(),
            requirements: Vec::new(),
            enums: Vec::new(),
            resources: Vec::new(),
            events: Vec::new(),
            rules: Vec::new(),
            policies: Policies {
                categories: Vec::new(),
                fields: Vec::new(),
                span_ref: None,
            },
            commands: Vec::new(),
            apis: vec![Api {
                name: "list_products".to_owned(),
                method: HttpMethod::Get,
                path: "/api/products/{category_id}".to_owned(),
                policy: PolicyRef::Local("read".to_owned()),
                policy_expr: None,
                rate_limit: Some("30 per minute per user".to_owned()),
                output: TypeRef::Many(Box::new(TypeRef::Builtin(BuiltinType::Text))),
                handler: PathRef::authored("./api/list_products.go"),
                locale_negotiate: None,
                span_ref: None,
            }],
            records: Vec::new(),
            queries: Vec::new(),
            workflows: Vec::new(),
            jobs: Vec::new(),
            webhooks: Vec::new(),
            notifications: Vec::new(),
            event_groups: Vec::new(),
            tenant_migrations: Vec::new(),
            translation: None,
            pollers: vec![],
            auth: None,
            surfaces: Vec::new(),
            extensions: Vec::new(),
            escape_routes: Vec::new(),
            agents: Vec::new(),
            reports: Vec::new(),
            previous_names: Vec::new(),
            span_ref: None,
        }
    }

    #[test]
    fn feature_emit_api_file_outputs_package_and_endpoint_contract() {
        let feature = feature_with_api();
        let module = Module {
            workspace: None,
            contracts: Vec::new(),
            app: None,
            registry: None,
            profiles: Vec::new(),
            design: None,
            rbac: None,
            features: vec![feature],
        };
        let cross_index = CrossFeatureIndex::build(&module);
        let emit_ctx = EmitContext::no_source("catalog/api.gen.go");

        let out = emit_api_file(
            "features/catalog/catalog.lzi",
            &module.features[0],
            "lazuli/test",
            &cross_index,
            &emit_ctx,
        )
        .expect("feature with one API must emit api.gen.go");

        assert!(!out.is_empty());
        assert!(out.contains("// Code generated by lazuli; DO NOT EDIT."));
        assert!(out.contains("package catalog"));
        assert!(out.contains("type ListProductsArgs struct {"));
        assert!(out.contains("CategoryID lazuli.ID `json:\"category_id\"`"));
        assert!(out.contains("var listProducts = lazuli.Api[ListProductsArgs, []string]{"));
        assert!(out.contains("Method:    lazuli.MethodGet,"));
        assert!(out.contains("RateLimit: \"30 per minute per user\","));
    }
}
