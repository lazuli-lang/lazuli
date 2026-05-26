//! MCP bucket cycle (M8) — `mcp_server` emission. Walks every
//! `MCPServerSpec` declared on a feature and emits
//! `<feature>/mcp_server.gen.go` containing one `var <name>Server =
//! mcp.ServerRegistration{...}` literal per server, plus an `init()`
//! that calls `mcp.RegisterServer(...)` for each.
//!
//! Wire-thin: the emitter imports `lazuli.dev/runtime/lazuli/mcp` and
//! the JSON Schema literal is a hand-rolled `map[string]any` so we
//! never pull in jsonschema-go at user-code level.
//!
//! Per docs/proposals/bucket-mcp-cycle.md §L2 + §M8.

use lazuli_ir::{
    Feature, MCPAuth, MCPParam, MCPPrompt, MCPResource, MCPServerSpec, MCPTool, MCPTransport,
};

use super::casing::lower_camel;
use super::imports::ImportSet;
use super::patterns::{PATTERN_MCP_SERVER_REGISTER, emit_pattern_header};
use super::printer::GoPrinter;

/// Emit `<feature>/mcp_server.gen.go` for a feature, or `None` when
/// the feature declares no `mcp_server` blocks.
///
/// ## Examples
///
/// ```ignore
/// let go_src = emit_mcp_server_file("mcp.lzi", &feature);
/// ```
pub fn emit_mcp_server_file(source_label: &str, feature: &Feature) -> Option<String> {
    if feature.mcp_servers.is_empty() {
        return None;
    }

    let mut p = GoPrinter::new();
    let mut imports = ImportSet::new();
    imports.add("lazuli.dev/runtime/lazuli/mcp");
    if feature.mcp_servers.iter().any(|s| s.auth.is_some()) {
        imports.add("os");
    }

    p.banner(
        source_label,
        &super::casing::gen_package_name(&feature.name),
    );
    imports.emit(&mut p);
    p.blank();

    let mut servers: Vec<&MCPServerSpec> = feature.mcp_servers.iter().collect();
    servers.sort_by(|a, b| a.name.cmp(&b.name));

    let mut first = true;
    for server in &servers {
        if !first {
            p.blank();
        }
        first = false;
        emit_server_var(&mut p, feature, server);
    }

    p.blank();
    emit_init(&mut p, &servers);

    Some(p.finish())
}

fn server_var_name(server: &MCPServerSpec) -> String {
    format!("{}Server", lower_camel(&server.name))
}

fn emit_server_var(p: &mut GoPrinter, feature: &Feature, server: &MCPServerSpec) {
    p.line(&format!(
        "// MCP server `{}`. Declared in feature `{}`.",
        server.name, feature.name
    ));
    p.line(&format!(
        "var {} = mcp.ServerRegistration{{",
        server_var_name(server)
    ));
    p.indent();
    p.line(&format!("Name:      \"{}\",", escape_string(&server.name)));
    p.line(&format!(
        "Transport: {},",
        transport_const(server.transport)
    ));
    if let Some(auth) = &server.auth {
        emit_auth(p, auth);
    }
    emit_metadata(p, server);
    if !server.tools.is_empty() {
        emit_tools(p, feature, server);
    }
    if !server.resources.is_empty() {
        emit_resources(p, feature, server);
    }
    if !server.prompts.is_empty() {
        emit_prompts(p, feature, server);
    }
    p.dedent();
    p.line("}");
}

fn transport_const(t: MCPTransport) -> &'static str {
    match t {
        MCPTransport::Stdio => "mcp.TransportStdio",
        MCPTransport::HttpSse => "mcp.TransportHTTPSSE",
        MCPTransport::HttpStreamable => "mcp.TransportHTTPStreamable",
    }
}

fn emit_auth(p: &mut GoPrinter, auth: &MCPAuth) {
    match auth {
        MCPAuth::BearerEnvVar { env } => {
            p.line("Auth: &mcp.AuthSpec{");
            p.indent();
            p.line("Scheme: \"bearer\",");
            p.line(&format!(
                "// Token resolved from env at boot; the env-var name is `{}`",
                escape_string(env)
            ));
            p.line(&format!("Token: os.Getenv(\"{}\"),", escape_string(env)));
            p.dedent();
            p.line("},");
        }
    }
}

fn emit_metadata(p: &mut GoPrinter, server: &MCPServerSpec) {
    let m = &server.metadata;
    if m.name.is_none() && m.description.is_none() && m.version.is_none() {
        return;
    }
    p.line("Metadata: mcp.ServerMetadata{");
    p.indent();
    if let Some(n) = &m.name {
        p.line(&format!("Name:        \"{}\",", escape_string(n)));
    }
    if let Some(v) = &m.version {
        p.line(&format!("Version:     \"{}\",", escape_string(v)));
    }
    if let Some(d) = &m.description {
        p.line(&format!("Description: \"{}\",", escape_string(d)));
    }
    p.dedent();
    p.line("},");
}

fn emit_tools(p: &mut GoPrinter, feature: &Feature, server: &MCPServerSpec) {
    p.line("Tools: []mcp.ToolRegistration{");
    p.indent();
    for tool in &server.tools {
        emit_tool(p, feature, server, tool);
    }
    p.dedent();
    p.line("},");
}

fn emit_tool(p: &mut GoPrinter, feature: &Feature, server: &MCPServerSpec, tool: &MCPTool) {
    let handler_key = handler_lookup_key(feature, server, &tool.name);
    p.line("{");
    p.indent();
    p.line(&format!("Name:        \"{}\",", escape_string(&tool.name)));
    if let Some(d) = &tool.description {
        p.line(&format!("Description: \"{}\",", escape_string(d)));
    }
    p.line("InputSchema: map[string]any{");
    p.indent();
    p.line("\"type\": \"object\",");
    if !tool.params.is_empty() {
        emit_params_schema(p, &tool.params);
    }
    p.dedent();
    p.line("},");
    p.line(&format!(
        "Handler:     mcp.ToolHandlerFromRegistry(\"{}\"),",
        escape_string(&handler_key)
    ));
    p.dedent();
    p.line("},");
}

fn emit_resources(p: &mut GoPrinter, feature: &Feature, server: &MCPServerSpec) {
    p.line("Resources: []mcp.ResourceRegistration{");
    p.indent();
    for r in &server.resources {
        emit_resource(p, feature, server, r);
    }
    p.dedent();
    p.line("},");
}

fn emit_resource(p: &mut GoPrinter, feature: &Feature, server: &MCPServerSpec, r: &MCPResource) {
    let handler_key = handler_lookup_key(feature, server, &r.name);
    p.line("{");
    p.indent();
    p.line(&format!("Name:        \"{}\",", escape_string(&r.name)));
    p.line(&format!(
        "URITemplate: \"{}\",",
        escape_string(&r.uri_template)
    ));
    if let Some(m) = &r.mime {
        p.line(&format!("MIME:        \"{}\",", escape_string(m)));
    }
    p.line(&format!(
        "Handler:     mcp.ResourceHandlerFromRegistry(\"{}\"),",
        escape_string(&handler_key)
    ));
    p.dedent();
    p.line("},");
}

fn emit_prompts(p: &mut GoPrinter, feature: &Feature, server: &MCPServerSpec) {
    p.line("Prompts: []mcp.PromptRegistration{");
    p.indent();
    for pr in &server.prompts {
        emit_prompt(p, feature, server, pr);
    }
    p.dedent();
    p.line("},");
}

fn emit_prompt(p: &mut GoPrinter, feature: &Feature, server: &MCPServerSpec, pr: &MCPPrompt) {
    let handler_key = handler_lookup_key(feature, server, &pr.name);
    p.line("{");
    p.indent();
    p.line(&format!("Name:         \"{}\",", escape_string(&pr.name)));
    if let Some(d) = &pr.description {
        p.line(&format!("Description:  \"{}\",", escape_string(d)));
    }
    p.line(&format!(
        "TemplatePath: \"{}\",",
        escape_string(&pr.template_path)
    ));
    if !pr.params.is_empty() {
        p.line("InputSchema: map[string]any{");
        p.indent();
        p.line("\"type\": \"object\",");
        emit_params_schema(p, &pr.params);
        p.dedent();
        p.line("},");
    }
    p.line(&format!(
        "Handler:      mcp.PromptHandlerFromRegistry(\"{}\"),",
        escape_string(&handler_key)
    ));
    p.dedent();
    p.line("},");
}

fn emit_params_schema(p: &mut GoPrinter, params: &[MCPParam]) {
    let required: Vec<&str> = params
        .iter()
        .filter(|p| p.required)
        .map(|p| p.name.as_str())
        .collect();

    p.line("\"properties\": map[string]any{");
    p.indent();
    for param in params {
        let json_ty = map_param_type(&param.ty_literal);
        p.line(&format!(
            "\"{}\": map[string]any{{\"type\": \"{}\"}},",
            escape_string(&param.name),
            json_ty,
        ));
    }
    p.dedent();
    p.line("},");

    if !required.is_empty() {
        p.line("\"required\": []any{");
        p.indent();
        for r in required {
            p.line(&format!("\"{}\",", escape_string(r)));
        }
        p.dedent();
        p.line("},");
    }
}

/// Map an author-side type literal (`string`, `int`, `bool`, `list of <T>`,
/// `enum [...]`, `JSON`, ...) into a JSON Schema `type` value. Unknown
/// literals fall through to `"string"` — doctor `MCP-PARAM-TYPE-001`
/// flags malformed types at design time.
fn map_param_type(ty: &str) -> &'static str {
    let trimmed = ty.trim();
    if trimmed.starts_with("list ") || trimmed.starts_with("JSON") {
        "array"
    } else if trimmed.starts_with("int") {
        "integer"
    } else if trimmed.starts_with("bool") || trimmed.starts_with("Boolean") {
        "boolean"
    } else if trimmed.starts_with("Decimal") || trimmed.starts_with("float") {
        "number"
    } else if trimmed.starts_with("enum") {
        // enum [a, b, c] — codegen emits as string for now; doctor may
        // promote to a typed enum schema in a follow-up.
        "string"
    } else {
        "string"
    }
}

/// Build the registry lookup key for a handler. Format
/// `<feature>.<server>.<member>` so the runtime can disambiguate when
/// the same member name appears in multiple servers across features.
fn handler_lookup_key(feature: &Feature, server: &MCPServerSpec, member: &str) -> String {
    format!("{}.{}.{}", feature.name, server.name, member)
}

fn emit_init(p: &mut GoPrinter, servers: &[&MCPServerSpec]) {
    emit_pattern_header(p, PATTERN_MCP_SERVER_REGISTER);
    p.line("func init() {");
    p.indent();
    for server in servers {
        p.line(&format!("mcp.RegisterServer({})", server_var_name(server)));
    }
    p.dedent();
    p.line("}");
}

fn escape_string(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_string_double_escapes_backslash_and_quote() {
        assert_eq!(escape_string(r#"a"b\c"#), r#"a\"b\\c"#);
    }

    #[test]
    fn map_param_type_maps_int_and_list() {
        assert_eq!(map_param_type("int"), "integer");
        assert_eq!(map_param_type("list of string"), "array");
        assert_eq!(map_param_type("bool"), "boolean");
        assert_eq!(map_param_type("string"), "string");
    }
}
