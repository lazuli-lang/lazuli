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
mod tests {
    use super::*;
    use lazuli_ir::{
        AppManifest, BuiltinType, CapabilityRef, Defaults, FileCapability, FileSize,
        FileSizeLiteral, FileVisibility, HttpMethod, MimeType, Module, PathRef, Policies,
        PolicyRef, QualifiedName, RateLimitSpec, Record, Resource, TypeRef,
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
            uses_spans: Vec::new(),
            uses_versions: Vec::new(),
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
            errors: None,
            commands: Vec::new(),
            apis: Vec::new(),
            records: Vec::new(),
            queries: Vec::new(),
            resume_routers: Vec::new(),
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
            channels: Vec::new(),
            caches: Vec::new(),
            aggregates: vec![],
            mcp_servers: vec![],
            previous_names: Vec::new(),
            span_ref: None,
            synth_origins: std::collections::BTreeMap::new(),
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
            error_pages: Vec::new(),
            uses: Vec::new(),
            packs: Vec::new(),
            bindings: Vec::new(),
            architecture: None,
            services: Vec::new(),
            communication: None,
            environments: Vec::new(),
            urls: Vec::new(),
            cors: None,
            headers: None,
            cookie: None,
            proxy: None,
            limits: None,
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
            route_guard: None,
            actor_query: None,
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
            policy_when_denied: None,
            rate_limit: None,
            output,
            handler: PathRef::authored(format!("./handlers/{name}.go")),
            locale_negotiate: None,
            deprecated: None,
            span_ref: None,
        }
    }

    fn simple_record(name: &str) -> Record {
        Record {
            name: name.to_owned(),
            public_contract: None,
            fields: Vec::new(),
            discriminator_field: None,
            span_ref: None,
        }
    }

    fn simple_resource(name: &str) -> Resource {
        Resource {
            name: name.to_owned(),
            public_contract: None,
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
            invariants: vec![],

            lock: None,

            composite_key: None,
            conventions: Vec::new(),
            lifecycle_routes: None,
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
            auto_photo_policy: None,
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
        api.rate_limit = Some(RateLimitSpec::from_default(
            "10 per hour per user".to_owned(),
        ));
        api.handler = PathRef::authored("./api/export_customers.go");
        feature.apis.push(api);

        let out = emit(&feature).expect("must emit");
        assert!(out.contains("// Code generated by lazuli; DO NOT EDIT."));
        assert!(out.contains("package customergen"));
        assert!(out.contains("\"lazuli.dev/runtime/lazuli\""));
        assert!(out.contains("\"lazuli.dev/runtime/lazuli/storage\""));
        assert!(!out.contains("\"lazuli/test/customer/api\""));
        assert!(out.contains("type CustomerExportApiArgs struct{}"));
        assert!(out.contains(
            "var customerExportApi = lazuli.Api[CustomerExportApiArgs, storage.FileRef]{"
        ));
        assert!(!out.contains("var customerExportApi = struct {"));
        assert!(!out.contains("TODO(runtime):"));
        // Codegen now emits a `func init()` that registers the typed
        // API value, plus an inline hint pointing to
        // `ValidateApiHandlers`. The previous inert `TODO` comment
        // never registered anything (review bug #1, 2026-05-15).
        assert!(out.contains("func init() { lazuli.RegisterApi(&customerExportApi) }"));
        assert!(out.contains("// Wire customerExportApi.Handler in your application code"));
        assert!(out.contains("lazuli.ValidateApiHandlers()"));
        assert!(
            !out.contains("// TODO(extension-points)"),
            "legacy TODO comment must be gone"
        );
        assert!(out.contains("Method:    lazuli.MethodGet,"));
        assert!(out.contains("Policy:    lazuli.Policy{Name: \"@policy.global_read\"},"));
        assert!(out.contains("RateLimit: lazuli.RateLimit{Default: \"10 per hour per user\"},"));
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
        assert!(out.contains(
            "var customerExportApi = lazuli.Api[CustomerExportApiArgs, storage.FileRef]{"
        ));
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
        assert!(out.contains(
            "var customerExportApi = lazuli.Api[CustomerExportApiArgs, storage.FileRef]{"
        ));
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
        api.rate_limit = Some(RateLimitSpec::from_default(
            "60 per minute per user".to_owned(),
        ));
        feature.apis.push(api);

        let out = emit(&feature).expect("must emit");
        assert!(out.contains("type CustomerSummaryApiArgs struct {"));
        assert!(out.contains("ID lazuli.ID `json:\"id\"`"));
        assert!(out.contains("// TODO(ir): Api path parameters have no typed IR slots"));
        assert!(out.contains(
            r#"var customerSummaryApi = lazuli.Api[CustomerSummaryApiArgs, CustomerSummary]{
	Name:      "customer.customer_summary",
	Feature:   "customer",
	Method:    lazuli.MethodGet,
	Path:      "/api/customer/{id}/summary",
	Policy:    lazuli.Policy{Name: "@policy.read"},
	RateLimit: lazuli.RateLimit{Default: "60 per minute per user"},
}

//lazuli:pattern api_register v1
func init() { lazuli.RegisterApi(&customerSummaryApi) }
// Wire customerSummaryApi.Handler in your application code, then call
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
        assert!(out.contains(
            "var ownerProfileApi = lazuli.Api[OwnerProfileApiArgs, orggen.UserProfile]{"
        ));
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
    fn deprecated_api_emits_deprecation_literal() {
        let mut feature = base_feature("customer");
        feature.records.push(simple_record("Customer"));
        let mut api = simple_api(
            "legacy_export",
            HttpMethod::Get,
            "/api/customers/export-v1",
            TypeRef::Many(Box::new(TypeRef::UserDefined(QualifiedName {
                feature: None,
                name: "Customer".to_owned(),
            }))),
        );
        api.deprecated = Some(lazuli_ir::Deprecation {
            since: Some("2026-04-01".to_owned()),
            replacement: Some(lazuli_ir::DeprecationReplacement::LocalApi(
                "export_v2".to_owned(),
            )),
            sunset: Some("2026-09-30".to_owned()),
        });
        feature.apis.push(api);

        let out = emit(&feature).expect("must emit");
        assert!(out.contains(
            "Deprecation: &lazuli.Deprecation{Since: \"2026-04-01\", Replacement: \"customer.api.export_v2\", Sunset: \"2026-09-30\"},"
        ));
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
mod feature_emit_tests;
