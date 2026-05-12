//! Cell G5 - `Api` kind emission. Walks every `Api` declared on a
//! feature and emits typed args plus an API contract value into
//! `<feature>/api.gen.go`.
//!
//! Runtime gap: Lazuli Go does not yet expose `lazuli.Api[I, O]` or a
//! typed `HttpMethod` catalog. To keep generated Go source parseable
//! while preserving the intended shape, this emitter writes an
//! anonymous struct value and places `TODO(runtime)` comments inside
//! the value literal. When the runtime lands proposal 4.2, the value
//! can switch directly to `lazuli.Api[Args, Output]`.
//!
//! Determinism: APIs are sorted by name, route args preserve path
//! order, and imports flow through `ImportSet`.

use lazuli_ir::{Api, Feature, HttpMethod, PolicyRef, TypeRef};

use super::casing::{lower_camel, pascal_case};
use super::cross_feature::CrossFeatureIndex;
use super::imports::ImportSet;
use super::printer::GoPrinter;
use super::types::{self, TypeCtx};

/// Emit `<feature>/api.gen.go` for a feature, or `None` when the
/// feature declares no APIs.
pub fn emit_api_file(
    source_label: &str,
    feature: &Feature,
    module_name: &str,
    cross_index: &CrossFeatureIndex<'_>,
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
        register_imports_for_type(&api.output, &type_ctx, &mut imports);
        let handler = handler_ref(feature, api, module_name);
        if let Some(import_path) = handler.import_path {
            imports.add(&import_path);
        }
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
        emit_api(&mut p, feature, api, module_name, &type_ctx);
    }

    Some(p.finish())
}

fn emit_api(p: &mut GoPrinter, feature: &Feature, api: &Api, module_name: &str, ctx: &TypeCtx<'_>) {
    let qualified_name = format!("{}.{}", feature.name, api.name);
    let args_type = api_args_type_name(&api.name);
    let var_name = lower_camel(&api.name);
    let (output_type, _import) = types::go_type_for(&api.output, ctx);
    let handler = handler_ref(feature, api, module_name);

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

    p.line(&format!("var {var_name} = struct {{"));
    p.indent();
    let handler_type = format!("func(ctx *lazuli.Ctx, input {args_type}) ({output_type}, error)");
    let rate_limit_type = if api.rate_limit.is_some() {
        "lazuli.RateLimit"
    } else {
        "lazuli.RateLimit"
    };
    let rows = vec![
        ("Name", "string"),
        ("Feature", "string"),
        ("Method", "string"),
        ("Path", "string"),
        ("Policy", "lazuli.Policy"),
        ("RateLimit", rate_limit_type),
        ("Handler", handler_type.as_str()),
    ];
    p.aligned_rows(&rows);
    p.dedent();
    p.line("}{");
    p.indent();
    p.line(&format!(
        "// TODO(runtime): replace this anonymous struct with lazuli.Api[{args_type}, {output_type}] when Lazuli Go adds Api[I, O] (proposal 4.2)."
    ));
    p.line(&format!(
        "// TODO(runtime): replace Method string with lazuli.HttpMethod and {} when Lazuli Go adds the method catalog.",
        method_const_name(api.method)
    ));
    if !args.is_empty() {
        p.line("// TODO(ir): Api path parameters have no typed IR slots; args are inferred from the path.");
    }

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
            format!("\"{}\",", method_literal(api.method)),
        ),
        (
            "Path:".to_owned(),
            format!("\"{}\",", escape_string(&api.path)),
        ),
        ("Policy:".to_owned(), format_policy(&api.policy)),
    ];
    if let Some(rate_limit) = &api.rate_limit {
        kv_rows.push((
            "RateLimit:".to_owned(),
            format!("lazuli.RateLimit(\"{}\"),", escape_string(rate_limit)),
        ));
    }
    kv_rows.push(("Handler:".to_owned(), format!("{},", handler.expr())));

    let key_width = kv_rows.iter().map(|(k, _)| k.len()).max().unwrap_or(0);
    for (key, value) in &kv_rows {
        let pad = key_width.saturating_sub(key.len());
        p.line(&format!("{}{} {}", key, " ".repeat(pad), value));
    }
    p.dedent();
    p.line("}");
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

#[derive(Clone, Debug, PartialEq, Eq)]
struct HandlerRef {
    import_path: Option<String>,
    qualifier: Option<String>,
    func_name: String,
}

impl HandlerRef {
    fn expr(&self) -> String {
        match &self.qualifier {
            Some(qualifier) => format!("{qualifier}.{}", self.func_name),
            None => self.func_name.clone(),
        }
    }
}

fn handler_ref(feature: &Feature, api: &Api, module_name: &str) -> HandlerRef {
    let normalized = api.handler.path.replace('\\', "/");
    let trimmed = normalized
        .strip_prefix("./")
        .unwrap_or(normalized.as_str())
        .trim_matches('/');
    let mut parts: Vec<&str> = trimmed
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .collect();

    if parts
        .last()
        .map(|part| part.ends_with(".go"))
        .unwrap_or(false)
    {
        parts.pop();
    }

    if parts.is_empty() {
        return HandlerRef {
            import_path: None,
            qualifier: None,
            func_name: pascal_case(&api.name),
        };
    }

    let dir = parts.join("/");
    let last = parts.last().copied().unwrap_or("handlers");
    let import_path = format!(
        "{}/{}/{}",
        module_name.trim_end_matches('/'),
        feature.name,
        dir
    );

    HandlerRef {
        import_path: Some(import_path),
        qualifier: Some(sanitise_go_ident(last)),
        func_name: pascal_case(&api.name),
    }
}

fn register_imports_for_type(type_ref: &TypeRef, ctx: &TypeCtx<'_>, imports: &mut ImportSet) {
    let (_go, import) = types::go_type_for(type_ref, ctx);
    if let Some(path) = import {
        imports.add(&path);
    }
    if let TypeRef::Many(inner) = type_ref {
        register_imports_for_type(inner, ctx, imports);
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

fn method_literal(method: HttpMethod) -> &'static str {
    match method {
        HttpMethod::Get => "GET",
        HttpMethod::Post => "POST",
        HttpMethod::Put => "PUT",
        HttpMethod::Patch => "PATCH",
        HttpMethod::Delete => "DELETE",
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

fn sanitise_go_ident(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        return "handlers".to_owned();
    }
    if out
        .chars()
        .next()
        .map(|c| c.is_ascii_digit())
        .unwrap_or(false)
    {
        out.insert(0, '_');
    }
    out
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
        AppManifest, BuiltinType, Defaults, Module, PathRef, Policies, QualifiedName, Record,
        Resource,
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
            auth: None,
            surfaces: Vec::new(),
            extensions: Vec::new(),
            escape_routes: Vec::new(),
            agents: Vec::new(),
            previous_names: Vec::new(),
            span_ref: None,
        }
    }

    fn minimal_app() -> AppManifest {
        AppManifest {
            name: "test".to_owned(),
            title: None,
            version: None,
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
            locale: None,
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
            features,
        }
    }

    fn emit_from_module(module: &Module, feature_index: usize) -> Option<String> {
        let index = CrossFeatureIndex::build(module);
        emit_api_file(
            "examples/x.lzi",
            &module.features[feature_index],
            "lazuli/test",
            &index,
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
        }
    }

    #[test]
    fn empty_feature_returns_none() {
        let feature = base_feature("customer");
        assert!(emit(&feature).is_none());
    }

    #[test]
    fn canonical_file_api_emits_runtime_todos_storage_and_handler_import() {
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
        assert!(out.contains("\"lazuli/test/customer/api\""));
        assert!(out.contains("type CustomerExportArgs struct{}"));
        assert!(out.contains("var customerExport = struct {"));
        assert!(out.contains("// TODO(runtime): replace this anonymous struct with lazuli.Api[CustomerExportArgs, storage.FileRef]"));
        assert!(out.contains("Method:    \"GET\","));
        assert!(out.contains("Policy:    lazuli.Policy{Name: \"@policy.global_read\"},"));
        assert!(out.contains("RateLimit: lazuli.RateLimit(\"10 per hour per user\"),"));
        assert!(out.contains("Handler:   api.CustomerExport,"));
    }

    #[test]
    fn path_params_emit_inferred_args_and_method_gap_comment() {
        let mut feature = base_feature("customer");
        feature.records.push(simple_record("CustomerSummary"));
        feature.apis.push(simple_api(
            "customer_summary",
            HttpMethod::Patch,
            "/api/customer/{id}/summary/:locale",
            TypeRef::UserDefined(QualifiedName {
                feature: None,
                name: "CustomerSummary".to_owned(),
            }),
        ));

        let out = emit(&feature).expect("must emit");
        assert!(out.contains("type CustomerSummaryArgs struct {"));
        assert!(out.contains("ID     lazuli.ID `json:\"id\"`"));
        assert!(out.contains("Locale string    `json:\"locale\"`"));
        assert!(out.contains(
            "// TODO(runtime): replace Method string with lazuli.HttpMethod and lazuli.MethodPatch"
        ));
        assert!(out.contains("// TODO(ir): Api path parameters have no typed IR slots"));
        assert!(out.contains("Method:  \"PATCH\","));
        assert!(out.contains("Handler: handlers.CustomerSummary,"));
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
        assert!(
            out.contains("func(ctx *lazuli.Ctx, input OwnerProfileArgs) (org.UserProfile, error)")
        );
        assert!(out.contains("OwnerID lazuli.ID `json:\"owner_id\"`"));
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
