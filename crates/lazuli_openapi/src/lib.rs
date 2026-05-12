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
    }
    if cmd.approval.is_some() {
        out.line("x-lazuli-approval: true");
    }
    if let Some(dep) = &cmd.deprecated {
        out.line("deprecated: true");
        if let Some(since) = &dep.since {
            out.kv_quoted("x-lazuli-deprecated-since", since);
        }
        if let Some(rep) = &dep.replacement {
            let rendered = match rep {
                ir::DeprecationReplacement::LocalCommand(name) => {
                    format!("{}.command.{}", feature, name)
                }
                ir::DeprecationReplacement::Qualified(qn) => format!(
                    "{}.command.{}",
                    qn.feature.clone().unwrap_or_default(),
                    qn.name
                ),
                ir::DeprecationReplacement::Url(u) => u.clone(),
            };
            out.kv_quoted("x-lazuli-replacement", &rendered);
        }
        if let Some(sunset) = &dep.sunset {
            out.kv_quoted("x-lazuli-sunset", sunset);
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
            let (kind, fmt) = builtin_to_openapi(*b);
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

fn builtin_to_openapi(b: ir::BuiltinType) -> (&'static str, Option<&'static str>) {
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
        SemanticMoney => ("number", None),
        SemanticCurrency => ("string", None),
        // Hostpoint Phase Prep §9.1 — `@semantic.GeoPoint` carries
        // `{ lat, lng }`. OpenAPI does not have a `geography` format,
        // so we surface it as a generic `object` here; codegen-go is
        // the consumer that materialises the PostGIS-aware shape.
        SemanticGeoPoint => ("object", None),
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
