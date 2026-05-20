//! OpenAPI 3.1.0 emission from typed Lazuli IR.
//!
//! Pure consumer of `lazuli_ir::Module`. Walks features for commands,
//! agent `expose http` mounts, and `api <name>` blocks; emits an OpenAPI
//! 3.1 document via a small purpose-built YAML printer (we don't depend
//! on `serde_yaml` to keep the build graph tight).
//!
//! Boundary discipline: this crate emits a spec artifact. It never
//! generates Go server stubs, language SDKs, or any transport binding —
//! those live in adapters that consume the emitted spec.

use lazuli_ir as ir;

pub struct EmitOptions {
    /// API version reported in `info.version`. Defaults to "0.0.0".
    pub api_version: Option<String>,
    /// When true, omit operations whose IR shape is text-pattern only.
    pub strict_typed_only: bool,
}

impl Default for EmitOptions {
    fn default() -> Self {
        Self {
            api_version: None,
            strict_typed_only: false,
        }
    }
}

/// Emit OpenAPI 3.1.0 YAML from a Lazuli `Module`.
pub fn emit(module: &ir::Module, opts: EmitOptions) -> String {
    let mut out = YamlEmitter::new();
    out.line("openapi: 3.1.0");
    out.line("info:");
    out.indent();
    out.kv(
        "title",
        &module
            .app
            .as_ref()
            .map(|a| a.name.as_str())
            .unwrap_or("lazuli"),
    );
    out.kv(
        "version",
        &opts
            .api_version
            .clone()
            .unwrap_or_else(|| "0.0.0".to_owned()),
    );
    out.line("x-lazuli-generator: lazuli_openapi");
    out.dedent();

    out.line("paths:");
    out.indent();
    for feature in &module.features {
        for cmd in &feature.commands {
            emit_command(&mut out, &feature.name, cmd);
        }
        for api in &feature.apis {
            emit_api(&mut out, &feature.name, api, opts.strict_typed_only);
        }
        for agent in &feature.agents {
            if let Some(expose) = &agent.expose_http {
                emit_agent_expose(&mut out, &feature.name, &agent.name, expose);
            }
        }
        for webhook in &feature.webhooks {
            emit_webhook(&mut out, &feature.name, webhook);
        }
    }
    out.dedent();

    out.line("components:");
    out.indent();
    out.line("schemas:");
    out.indent();
    let mut schemas_emitted = false;
    for feature in &module.features {
        for resource in &feature.resources {
            emit_resource_schema(&mut out, resource);
            schemas_emitted = true;
        }
        for record in &feature.records {
            emit_record_schema(&mut out, record);
            schemas_emitted = true;
        }
        for enum_decl in &feature.enums {
            emit_enum_schema(&mut out, enum_decl);
            schemas_emitted = true;
        }
    }
    if !schemas_emitted {
        out.line("{}");
    }
    out.dedent();
    out.line("responses:");
    out.indent();
    out.line("Problem:");
    out.indent();
    out.line("description: RFC 7807 problem-details envelope.");
    out.line("content:");
    out.indent();
    out.line("application/problem+json:");
    out.indent();
    out.line("schema:");
    out.indent();
    out.line("type: object");
    out.line("required: [type, title, status]");
    out.line("properties:");
    out.indent();
    out.kv("type", "{ type: string, format: uri }");
    out.kv("title", "{ type: string }");
    out.kv("status", "{ type: integer }");
    out.kv("detail", "{ type: string }");
    out.kv("instance", "{ type: string }");
    out.dedent();
    out.dedent();
    out.dedent();
    out.dedent();
    out.dedent();
    out.dedent();
    out.dedent();
    out.dedent();

    out.into_string()
}

fn emit_command(out: &mut YamlEmitter, feature: &str, cmd: &ir::Command) {
    let path = command_path(feature, cmd);
    let method = command_method(cmd);
    out.line(&format!("{}:", quote_key(&path)));
    out.indent();
    out.line(&format!("{}:", method));
    out.indent();
    out.kv("operationId", &format!("{}_{}", feature, cmd.name));
    out.kv("x-lazuli-feature", feature);
    out.kv("x-lazuli-kind", "command");
    if let Some(rl) = &cmd.rate_limit {
        out.kv_quoted("x-lazuli-rate-limit", rl);
    }
    if !cmd.emits.is_empty() {
        out.kv(
            "x-lazuli-emits",
            &format!(
                "[{}]",
                cmd.emits.iter().cloned().collect::<Vec<_>>().join(", ")
            ),
        );
    }
    if let Some(audit) = &cmd.audit {
        out.kv(
            "x-lazuli-audit",
            &format!(
                "[{}]",
                audit
                    .subjects
                    .iter()
                    .map(|s| quote_value(s))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        );
        if let Some(emit_to) = &audit.emit_to {
            out.kv_quoted("x-lazuli-audit-emit-to", emit_to);
        }
    }
    if let Some(approval) = &cmd.approval {
        emit_approval(out, approval);
    }
    if let Some(dep) = &cmd.deprecated {
        out.line("deprecated: true");
        if let Some(since) = &dep.since {
            out.kv_quoted("x-lazuli-deprecated-since", since);
        }
        if let Some(rep) = &dep.replacement {
            out.kv_quoted(
                "x-lazuli-deprecated-replacement",
                &render_deprecation_replacement(feature, rep),
            );
        }
        if let Some(sunset) = &dep.sunset {
            out.kv_quoted("x-lazuli-deprecated-sunset", sunset);
        }
    }
    let policy_str = render_policy(&cmd.policy);
    if !policy_str.is_empty() {
        out.kv_quoted("x-lazuli-policy", &policy_str);
    }
    // Path parameters
    if !cmd.route.is_empty() {
        out.line("parameters:");
        out.indent();
        for slot in &cmd.route {
            out.line(&format!("- name: {}", slot.name));
            out.indent();
            out.line("in: path");
            out.line("required: true");
            out.line("schema:");
            out.indent();
            emit_schema_inline(out, &slot.type_ref);
            out.dedent();
            out.dedent();
        }
        out.dedent();
    }
    // Request body
    match &cmd.input {
        ir::CommandInput::Empty => {}
        ir::CommandInput::Short(_) | ir::CommandInput::Typed(_) => {
            out.line("requestBody:");
            out.indent();
            out.line("required: true");
            out.line("content:");
            out.indent();
            out.line("application/json:");
            out.indent();
            out.line("schema:");
            out.indent();
            emit_command_input_schema(out, cmd);
            out.dedent();
            out.dedent();
            out.dedent();
            out.dedent();
        }
    }
    // Responses
    out.line("responses:");
    out.indent();
    match &cmd.effect {
        ir::CommandEffect::Creates(e) => {
            out.line("'201':");
            out.indent();
            out.line("description: created");
            out.line("content:");
            out.indent();
            out.line("application/json:");
            out.indent();
            out.line("schema:");
            out.indent();
            out.line(&format!("$ref: '#/components/schemas/{}'", e.resource.name));
            out.dedent();
            out.dedent();
            out.dedent();
            out.dedent();
        }
        ir::CommandEffect::Updates(e) => {
            out.line("'200':");
            out.indent();
            out.line("description: updated");
            out.line("content:");
            out.indent();
            out.line("application/json:");
            out.indent();
            out.line("schema:");
            out.indent();
            out.line(&format!("$ref: '#/components/schemas/{}'", e.resource.name));
            out.dedent();
            out.dedent();
            out.dedent();
            out.dedent();
        }
        ir::CommandEffect::Deletes(_) => {
            out.line("'204':");
            out.indent();
            out.line("description: deleted");
            out.dedent();
        }
        ir::CommandEffect::Returns(e) => {
            out.line("'200':");
            out.indent();
            out.line("description: ok");
            out.line("content:");
            out.indent();
            out.line("application/json:");
            out.indent();
            out.line("schema:");
            out.indent();
            emit_schema_inline(out, &e.return_type);
            out.dedent();
            out.dedent();
            out.dedent();
            out.dedent();
        }
        ir::CommandEffect::None => {
            out.line("'200':");
            out.indent();
            out.line("description: ok");
            out.dedent();
        }
    }
    out.line("'4XX':");
    out.indent();
    out.line("$ref: '#/components/responses/Problem'");
    out.dedent();
    out.line("'5XX':");
    out.indent();
    out.line("$ref: '#/components/responses/Problem'");
    out.dedent();
    out.dedent();
    out.dedent();
    out.dedent();
}

fn emit_api(out: &mut YamlEmitter, feature: &str, api: &ir::Api, strict: bool) {
    if strict {
        return;
    }
    out.line(&format!("{}:", quote_key(&api.path)));
    out.indent();
    out.line(&format!("{}:", http_method_name(api.method)));
    out.indent();
    out.kv("operationId", &format!("{}_{}", feature, api.name));
    out.kv("x-lazuli-feature", feature);
    out.kv("x-lazuli-kind", "api");
    if let Some(rl) = &api.rate_limit {
        out.kv_quoted("x-lazuli-rate-limit", rl);
    }
    if let Some(dep) = &api.deprecated {
        out.line("deprecated: true");
        if let Some(since) = &dep.since {
            out.kv_quoted("x-lazuli-deprecated-since", since);
        }
        if let Some(rep) = &dep.replacement {
            out.kv_quoted(
                "x-lazuli-deprecated-replacement",
                &render_deprecation_replacement(feature, rep),
            );
        }
        if let Some(sunset) = &dep.sunset {
            out.kv_quoted("x-lazuli-deprecated-sunset", sunset);
        }
    }
    let policy_str = render_policy(&api.policy);
    if !policy_str.is_empty() {
        out.kv_quoted("x-lazuli-policy", &policy_str);
    }
    out.line("responses:");
    out.indent();
    out.line("'200':");
    out.indent();
    out.line("description: ok");
    out.line("content:");
    out.indent();
    out.line("application/json:");
    out.indent();
    out.line("schema:");
    out.indent();
    emit_schema_inline(out, &api.output);
    out.dedent();
    out.dedent();
    out.dedent();
    out.dedent();
    out.line("'4XX':");
    out.indent();
    out.line("$ref: '#/components/responses/Problem'");
    out.dedent();
    out.dedent();
    out.dedent();
    out.dedent();
}

fn emit_agent_expose(out: &mut YamlEmitter, feature: &str, agent: &str, expose: &ir::HttpExposure) {
    out.line(&format!("{}:", quote_key(&expose.path)));
    out.indent();
    out.line(&format!("{}:", http_method_name(expose.method)));
    out.indent();
    out.kv("operationId", &format!("{}_{}", feature, agent));
    out.kv("x-lazuli-feature", feature);
    out.kv("x-lazuli-kind", "agent");
    out.line("responses:");
    out.indent();
    out.line("'200':");
    out.indent();
    out.line("description: agent response");
    out.dedent();
    out.dedent();
    out.dedent();
    out.dedent();
}

fn emit_webhook(out: &mut YamlEmitter, feature: &str, webhook: &ir::Webhook) {
    out.line(&format!("{}:", quote_key(&webhook.route)));
    out.indent();
    out.line("post:");
    out.indent();
    out.kv(
        "operationId",
        &format!("{}_{}_webhook", feature, webhook.name),
    );
    out.kv("x-lazuli-feature", feature);
    out.kv("x-lazuli-kind", "webhook");
    out.kv_quoted("x-lazuli-webhook", &webhook.name);
    out.kv_quoted("x-lazuli-handler", &webhook.handler.path);
    if let Some(verify) = &webhook.structured_verify {
        emit_verify(out, verify);
    } else {
        out.kv_quoted("x-lazuli-verify-path", &webhook.verify.path);
    }
    if let Some(tenant_from) = &webhook.tenant_from {
        out.kv_quoted("x-lazuli-tenant-from", &render_path(&tenant_from.path));
    }
    if let Some(idempotency) = &webhook.idempotency {
        out.kv_quoted("x-lazuli-idempotency-by", &render_path(&idempotency.by));
    }
    if let Some(policy) = &webhook.policy {
        let policy_str = render_policy(policy);
        if !policy_str.is_empty() {
            out.kv_quoted("x-lazuli-policy", &policy_str);
        }
    }
    if !webhook.emits.is_empty() {
        out.kv("x-lazuli-emits", &quoted_list(&webhook.emits));
    }
    if let Some(payload_from) = &webhook.payload_from {
        out.kv_quoted(
            "x-lazuli-payload-from",
            &format!("webhook_events.{}", payload_from.name),
        );
    }
    if let Some(retry) = &webhook.retry {
        emit_retry(out, retry);
    }
    if let Some(replay) = &webhook.replay {
        emit_replay(out, replay);
    }
    if let Some(dlq) = &webhook.dlq {
        emit_dlq(out, dlq);
    }
    out.line("requestBody:");
    out.indent();
    out.line("required: true");
    out.line("content:");
    out.indent();
    out.line("application/json:");
    out.indent();
    out.line("schema:");
    out.indent();
    out.line("type: object");
    out.dedent();
    out.dedent();
    out.dedent();
    out.dedent();
    out.line("responses:");
    out.indent();
    out.line("'200':");
    out.indent();
    out.line("description: accepted");
    if let Some(return_type) = &webhook.returns {
        out.line("content:");
        out.indent();
        out.line("application/json:");
        out.indent();
        out.line("schema:");
        out.indent();
        emit_schema_inline(out, return_type);
        out.dedent();
        out.dedent();
        out.dedent();
    }
    out.dedent();
    out.line("'4XX':");
    out.indent();
    out.line("$ref: '#/components/responses/Problem'");
    out.dedent();
    out.line("'5XX':");
    out.indent();
    out.line("$ref: '#/components/responses/Problem'");
    out.dedent();
    out.dedent();
    out.dedent();
    out.dedent();
}

fn emit_command_input_schema(out: &mut YamlEmitter, cmd: &ir::Command) {
    match &cmd.input {
        ir::CommandInput::Empty => {}
        ir::CommandInput::Short(fields) => {
            out.line("type: object");
            out.line("properties:");
            out.indent();
            for f in fields {
                out.line(&format!("{}:", f));
                out.indent();
                out.line("type: string");
                out.dedent();
            }
            out.dedent();
        }
        ir::CommandInput::Typed(slots) => {
            out.line("type: object");
            let required: Vec<&str> = slots
                .iter()
                .filter(|s| s.required)
                .map(|s| s.name.as_str())
                .collect();
            if !required.is_empty() {
                out.line(&format!("required: [{}]", required.join(", ")));
            }
            out.line("properties:");
            out.indent();
            for slot in slots {
                out.line(&format!("{}:", slot.name));
                out.indent();
                emit_schema_inline(out, &slot.type_ref);
                out.dedent();
            }
            out.dedent();
        }
    }
}

fn emit_schema_inline(out: &mut YamlEmitter, ty: &ir::TypeRef) {
    match ty {
        ir::TypeRef::Builtin(b) => {
            let (kind, fmt) = builtin_to_openapi(b);
            out.line(&format!("type: {}", kind));
            if let Some(fmt) = fmt {
                out.line(&format!("format: {}", fmt));
            }
        }
        ir::TypeRef::UserDefined(qn) => {
            out.line(&format!("$ref: '#/components/schemas/{}'", qn.name));
        }
        ir::TypeRef::EnumRef(qn) => {
            out.line(&format!("$ref: '#/components/schemas/{}'", qn.name));
        }
        ir::TypeRef::Many(inner) => {
            out.line("type: array");
            out.line("items:");
            out.indent();
            emit_schema_inline(out, inner);
            out.dedent();
        }
        ir::TypeRef::Capability(cap) => match cap {
            ir::CapabilityRef::File(_) => {
                out.line("type: string");
                out.line("format: binary");
                out.line("x-lazuli-capability: File");
            }
            ir::CapabilityRef::Hashed(_) => {
                out.line("type: string");
                out.line("x-lazuli-capability: Hashed");
            }
            ir::CapabilityRef::Encrypted(_) => {
                out.line("type: string");
                out.line("x-lazuli-capability: Encrypted");
            }
            ir::CapabilityRef::E2ee(_) => {
                out.line("type: string");
                out.line("x-lazuli-capability: E2ee");
            }
            ir::CapabilityRef::Token(_) => {
                out.line("type: string");
                out.line("x-lazuli-capability: Token");
            }
        },
        ir::TypeRef::Unresolved(s) => {
            out.line("type: string");
            out.line(&format!("x-lazuli-unresolved: {}", quote_value(s)));
        }
    }
}

fn emit_resource_schema(out: &mut YamlEmitter, r: &ir::Resource) {
    out.line(&format!("{}:", r.name));
    out.indent();
    out.line("type: object");
    out.line("properties:");
    out.indent();
    for f in &r.fields {
        out.line(&format!("{}:", f.name));
        out.indent();
        emit_schema_inline(out, &f.type_ref);
        if !f.required {
            out.line("nullable: true");
        }
        out.dedent();
    }
    out.dedent();
    out.dedent();
}

fn emit_record_schema(out: &mut YamlEmitter, r: &ir::Record) {
    out.line(&format!("{}:", r.name));
    out.indent();
    out.line("type: object");
    out.line("properties:");
    out.indent();
    for f in &r.fields {
        out.line(&format!("{}:", f.name));
        out.indent();
        emit_schema_inline(out, &f.type_ref);
        if !f.required {
            out.line("nullable: true");
        }
        out.dedent();
    }
    out.dedent();
    out.dedent();
}

fn emit_enum_schema(out: &mut YamlEmitter, e: &ir::EnumDecl) {
    out.line(&format!("{}:", e.name));
    out.indent();
    out.line("type: string");
    out.line(&format!(
        "enum: [{}]",
        e.variants
            .iter()
            .map(|v| v.name.clone())
            .collect::<Vec<_>>()
            .join(", ")
    ));
    out.dedent();
}

fn command_path(feature: &str, cmd: &ir::Command) -> String {
    let mut path = format!("/api/{}/{}", feature, cmd.name);
    for slot in &cmd.route {
        path.push_str(&format!("/{{{}}}", slot.name));
    }
    path
}

fn command_method(cmd: &ir::Command) -> &'static str {
    match cmd.kind {
        ir::CommandKind::Create => "post",
        ir::CommandKind::Update => "patch",
        ir::CommandKind::Delete => "delete",
        ir::CommandKind::Returns => "post",
    }
}

fn http_method_name(m: ir::HttpMethod) -> &'static str {
    match m {
        ir::HttpMethod::Get => "get",
        ir::HttpMethod::Post => "post",
        ir::HttpMethod::Put => "put",
        ir::HttpMethod::Patch => "patch",
        ir::HttpMethod::Delete => "delete",
    }
}

fn render_policy(p: &ir::PolicyRef) -> String {
    match p {
        ir::PolicyRef::None => String::new(),
        ir::PolicyRef::Local(name) => format!("@policy.{}", name),
        ir::PolicyRef::Atom(a) => a.clone(),
        ir::PolicyRef::External { feature, name } => format!("{}.policy.{}", feature, name),
        ir::PolicyRef::Unresolved(s) => s.clone(),
    }
}

fn render_deprecation_replacement(feature: &str, rep: &ir::DeprecationReplacement) -> String {
    match rep {
        ir::DeprecationReplacement::LocalCommand(name) => {
            format!("{}.command.{}", feature, name)
        }
        ir::DeprecationReplacement::LocalApi(name) => {
            format!("{}.api.{}", feature, name)
        }
        ir::DeprecationReplacement::Qualified(qn) => match &qn.feature {
            Some(feature) => format!("{}.command.{}", feature, qn.name),
            None => format!("command.{}", qn.name),
        },
        ir::DeprecationReplacement::QualifiedApi(qn) => match &qn.feature {
            Some(feature) => format!("{}.api.{}", feature, qn.name),
            None => format!("api.{}", qn.name),
        },
        ir::DeprecationReplacement::Url(u) => u.clone(),
    }
}

fn render_path(path: &ir::Path) -> String {
    path.segments.join(".")
}

fn quoted_list(values: &[String]) -> String {
    format!(
        "[{}]",
        values
            .iter()
            .map(|value| quote_value(value))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn emit_approval(out: &mut YamlEmitter, approval: &ir::ApprovalSpec) {
    out.line("x-lazuli-approval:");
    out.indent();
    out.kv("then", approval_then_literal(approval.then));
    out.kv_quoted("by", &approval.by);
    out.kv_quoted("reason", approval.required_when.as_deref().unwrap_or(""));
    out.dedent();
}

fn emit_verify(out: &mut YamlEmitter, verify: &ir::VerifySpec) {
    out.line("x-lazuli-verify:");
    out.indent();
    out.kv("scheme", verify_scheme_literal(verify.scheme));
    out.kv_quoted("algorithm", &verify.algorithm);
    out.kv_quoted("secret_env", &verify.secret_env);
    out.kv_quoted("header", &verify.header);
    out.dedent();
}

fn emit_retry(out: &mut YamlEmitter, retry: &ir::RetryPolicy) {
    out.line("x-lazuli-retry:");
    out.indent();
    out.kv("count", &retry.count.to_string());
    out.kv("backoff", backoff_literal(retry.backoff));
    out.dedent();
}

fn emit_replay(out: &mut YamlEmitter, replay: &ir::ReplaySpec) {
    out.line("x-lazuli-replay:");
    out.indent();
    out.kv("mode", replay_mode_literal(replay.mode));
    if let Some(within) = &replay.within {
        out.kv_quoted("within", within);
    }
    if let Some(dedupe_by) = &replay.dedupe_by {
        out.kv_quoted("dedupe_by", &render_path(dedupe_by));
    }
    out.dedent();
}

fn emit_dlq(out: &mut YamlEmitter, dlq: &ir::DlqSpec) {
    out.line("x-lazuli-dlq:");
    out.indent();
    match dlq {
        ir::DlqSpec::Emit { event } => {
            out.kv("kind", "emit");
            out.kv_quoted("event", event);
        }
        ir::DlqSpec::Handler { path } => {
            out.kv("kind", "handler");
            out.kv_quoted("path", &path.path);
        }
        ir::DlqSpec::Drop { reason } => {
            out.kv("kind", "drop");
            out.kv_quoted("reason", reason);
        }
    }
    out.dedent();
}

fn approval_then_literal(then: ir::ApprovalThen) -> &'static str {
    match then {
        ir::ApprovalThen::Deny => "deny",
        ir::ApprovalThen::Allow => "allow",
        ir::ApprovalThen::Escalate => "escalate",
    }
}

fn verify_scheme_literal(scheme: ir::VerifyScheme) -> &'static str {
    match scheme {
        ir::VerifyScheme::Hmac => "hmac",
    }
}

fn replay_mode_literal(mode: ir::ReplayMode) -> &'static str {
    match mode {
        ir::ReplayMode::Allow => "allow",
        ir::ReplayMode::Deny => "deny",
    }
}

fn backoff_literal(backoff: ir::BackoffStrategy) -> &'static str {
    match backoff {
        ir::BackoffStrategy::Fixed => "fixed",
        ir::BackoffStrategy::Exponential => "exponential",
    }
}

fn builtin_to_openapi(b: &ir::BuiltinType) -> (&'static str, Option<&'static str>) {
    use ir::BuiltinType::*;
    match b {
        Text => ("string", None),
        Integer => ("integer", None),
        Decimal => ("number", None),
        Boolean => ("boolean", None),
        Id => ("string", Some("uuid")),
        DateTime => ("string", Some("date-time")),
        Date => ("string", Some("date")),
        Json => ("object", None),
        SemanticEmail => ("string", Some("email")),
        SemanticPhone => ("string", None),
        SemanticUrl => ("string", Some("uri")),
        SemanticUuid => ("string", Some("uuid")),
        SemanticMoney { .. } => ("number", None),
        SemanticCurrency => ("string", None),
        // GeoPoint follow-up — `@semantic.GeoPoint` carries
        // `{ lat, lng }`. OpenAPI does not have a `geography` format,
        // so we surface it as a generic `object` here; codegen-go is
        // the consumer that materialises the PostGIS-aware shape.
        SemanticGeoPoint => ("object", None),
        // B3 — plugin-contributed `@semantic.<Name>` projects to the
        // carrier's OpenAPI shape. The plugin owns validator + format
        // affinity; the wire surface here is the carrier alone.
        SemanticPluginType { carrier, .. } => builtin_to_openapi(carrier),
        CapSecret => ("string", None),
        CapFile => ("string", Some("binary")),
    }
}

fn quote_key(k: &str) -> String {
    // OpenAPI keys with special chars need quoting; paths usually do.
    if k.contains(' ') || k.contains(':') {
        format!("\"{}\"", k)
    } else {
        k.to_owned()
    }
}

fn quote_value(v: &str) -> String {
    format!("\"{}\"", v.replace('"', "\\\""))
}

// ============================================================================
// Tiny purpose-built YAML emitter. Indent-prefixed lines, two-space steps.
// Avoids serde_yaml dependency.
// ============================================================================
struct YamlEmitter {
    buf: String,
    depth: usize,
}

impl YamlEmitter {
    fn new() -> Self {
        Self {
            buf: String::with_capacity(4096),
            depth: 0,
        }
    }
    fn indent(&mut self) {
        self.depth += 1;
    }
    fn dedent(&mut self) {
        if self.depth > 0 {
            self.depth -= 1;
        }
    }
    fn line(&mut self, s: &str) {
        for _ in 0..self.depth {
            self.buf.push_str("  ");
        }
        self.buf.push_str(s);
        self.buf.push('\n');
    }
    fn kv(&mut self, k: &str, v: &str) {
        self.line(&format!("{}: {}", k, v));
    }
    fn kv_quoted(&mut self, k: &str, v: &str) {
        self.line(&format!("{}: {}", k, quote_value(v)));
    }
    fn into_string(self) -> String {
        self.buf
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn module_with_feature(feature: ir::Feature) -> ir::Module {
        ir::Module {
            workspace: None,
            contracts: Vec::new(),
            app: None,
            registry: None,
            profiles: Vec::new(),
            design: None,
            rbac: None,
            features: vec![feature],
        }
    }

    fn base_feature() -> ir::Feature {
        ir::Feature {
            name: "billing".to_owned(),
            purpose: None,
            non_goals: Vec::new(),
            context_path: None,
            defaults: ir::Defaults::default(),
            uses: Vec::new(),
            uses_spans: Vec::new(),
            uses_versions: Vec::new(),
            requirements: Vec::new(),
            enums: Vec::new(),
            resources: Vec::new(),
            events: Vec::new(),
            rules: Vec::new(),
            policies: ir::Policies::default(),
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
        }
    }

    fn base_command() -> ir::Command {
        ir::Command {
            name: "reassign".to_owned(),
            public_contract: None,
            kind: ir::CommandKind::Update,
            route: Vec::new(),
            input: ir::CommandInput::Empty,
            target: None,
            lets: Vec::new(),
            effect: ir::CommandEffect::None,
            policy: ir::PolicyRef::None,
            policy_expr: None,
            policy_when_denied: None,
            emits: Vec::new(),
            rate_limit: None,
            audit: None,
            approval: None,
            invalidates: Vec::new(),
            external_calls: Vec::new(),
            timeout: None,
            retry: None,
            idempotency: None,
            write_window: None,
            deprecated: None,
            handler: None,
            tests: None,
            triggers: vec![],
            synthesized_from_cap_file: None,
            previous_names: Vec::new(),
            span_ref: None,
        }
    }

    fn base_webhook() -> ir::Webhook {
        ir::Webhook {
            name: "stripe_invoice_paid".to_owned(),
            route: "/webhooks/stripe/invoice-paid".to_owned(),
            verify: ir::PathRef::authored("./webhooks/verify_stripe.go"),
            structured_verify: None,
            tenant_from: None,
            scope_global: None,
            idempotency: None,
            policy: None,
            policy_expr: None,
            policy_when_denied: None,
            handler: ir::PathRef::authored("./webhooks/stripe_invoice_paid.go"),
            returns: None,
            emits: Vec::new(),
            emit_predicates: Vec::new(),
            payload_from: None,
            replay: None,
            dlq: None,
            retry: None,
            previous_names: Vec::new(),
            span_ref: None,
        }
    }

    #[test]
    fn command_tier4_fields_emit_as_openapi_extensions() {
        let mut command = base_command();
        command.rate_limit = Some("10 per minute per user".to_owned());
        command.audit = Some(ir::AuditSpec {
            subjects: vec!["actor".to_owned(), "target.id".to_owned()],
            emit_to: Some("audit_log".to_owned()),
            data_subject: None,
        });
        command.approval = Some(ir::ApprovalSpec {
            required_when: Some("target.tier = enterprise".to_owned()),
            by: "@role.admin".to_owned(),
            timeout: Some("24h".to_owned()),
            then: ir::ApprovalThen::Deny,
        });
        command.deprecated = Some(ir::Deprecation {
            since: Some("2026.04".to_owned()),
            replacement: Some(ir::DeprecationReplacement::LocalCommand(
                "reassign_v2".to_owned(),
            )),
            sunset: Some("2026-12-31".to_owned()),
        });

        let mut feature = base_feature();
        feature.commands.push(command);
        let yaml = emit(&module_with_feature(feature), EmitOptions::default());

        assert!(yaml.contains("x-lazuli-rate-limit: \"10 per minute per user\""));
        assert!(yaml.contains("x-lazuli-audit: [\"actor\", \"target.id\"]"));
        assert!(yaml.contains("x-lazuli-audit-emit-to: \"audit_log\""));
        assert!(yaml.contains(
            "x-lazuli-approval:\n        then: deny\n        by: \"@role.admin\"\n        reason: \"target.tier = enterprise\""
        ));
        assert!(yaml.contains("deprecated: true"));
        assert!(yaml.contains("x-lazuli-deprecated-since: \"2026.04\""));
        assert!(yaml.contains("x-lazuli-deprecated-replacement: \"billing.command.reassign_v2\""));
        assert!(yaml.contains("x-lazuli-deprecated-sunset: \"2026-12-31\""));
        assert!(!yaml.contains("x-lazuli-replacement:"));
        assert!(!yaml.contains("x-lazuli-sunset:"));
    }

    #[test]
    fn webhook_replay_retry_and_dlq_emit_as_openapi_extensions() {
        let mut webhook = base_webhook();
        webhook.structured_verify = Some(ir::VerifySpec {
            scheme: ir::VerifyScheme::Hmac,
            algorithm: "sha256".to_owned(),
            secret_env: "STRIPE_SECRET".to_owned(),
            header: "Stripe-Signature".to_owned(),
        });
        webhook.tenant_from = Some(ir::TenantFromSpec {
            path: ir::Path::from_segments(["payload", "org_id"]),
        });
        webhook.idempotency = Some(ir::IdempotencyKey {
            by: ir::Path::from_segments(["payload", "event_id"]),
        });
        webhook.payload_from = Some(ir::WebhookEventRef {
            name: "stripe_invoice_paid".to_owned(),
        });
        webhook.retry = Some(ir::RetryPolicy {
            count: 5,
            backoff: ir::BackoffStrategy::Exponential,
        });
        webhook.replay = Some(ir::ReplaySpec {
            mode: ir::ReplayMode::Allow,
            within: Some("24h".to_owned()),
            dedupe_by: Some(ir::Path::from_segments(["payload", "event_id"])),
        });
        webhook.dlq = Some(ir::DlqSpec::Emit {
            event: "stripe_invoice_paid_dead_lettered".to_owned(),
        });

        let mut feature = base_feature();
        feature.webhooks.push(webhook);
        let yaml = emit(&module_with_feature(feature), EmitOptions::default());

        assert!(yaml.contains("/webhooks/stripe/invoice-paid:\n    post:"));
        assert!(yaml.contains("x-lazuli-kind: webhook"));
        assert!(yaml.contains(
            "x-lazuli-verify:\n        scheme: hmac\n        algorithm: \"sha256\"\n        secret_env: \"STRIPE_SECRET\"\n        header: \"Stripe-Signature\""
        ));
        assert!(yaml.contains("x-lazuli-tenant-from: \"payload.org_id\""));
        assert!(yaml.contains("x-lazuli-idempotency-by: \"payload.event_id\""));
        assert!(yaml.contains("x-lazuli-payload-from: \"webhook_events.stripe_invoice_paid\""));
        assert!(yaml.contains("x-lazuli-retry:\n        count: 5\n        backoff: exponential"));
        assert!(yaml.contains(
            "x-lazuli-replay:\n        mode: allow\n        within: \"24h\"\n        dedupe_by: \"payload.event_id\""
        ));
        assert!(yaml.contains(
            "x-lazuli-dlq:\n        kind: emit\n        event: \"stripe_invoice_paid_dead_lettered\""
        ));
    }
}
