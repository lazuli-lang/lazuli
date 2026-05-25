//! `mcp_server` feature child — Model Context Protocol server vocabulary.
//!
//! Feature-scoped declaration at indent 2 enumerating an MCP server's
//! transport, optional scope and bearer auth, plus the tool / resource /
//! prompt catalog the server exposes. Closed catalog of child keywords —
//! everything outside the listed set is a parse error so authors get an
//! immediate diagnostic instead of silent drift.
//!
//! ## Grammar (closed)
//!
//! ```text
//! mcp_server <name>
//!   transport stdio | http_sse | http_streamable    # required
//!   scope feature.<name>                            # optional
//!   auth bearer env.<NAME>                          # optional
//!   metadata                                        # optional, single
//!     name "..."
//!     description "..."
//!     version "..."
//!   tool <name>                                     # repeatable
//!     description "..."
//!     params                                        # optional
//!       <field>: <Type> [required|optional]
//!     returns <Type>
//!     handler @fn.<name>                            # required
//!     policy @policy.<name>
//!   resource <name>                                 # repeatable
//!     uri_template "..."                            # required
//!     mime "..."
//!     handler @fn.<name>                            # required
//!     policy @policy.<name>
//!   prompt <name>                                   # repeatable
//!     description "..."
//!     params
//!       <field>: <Type> [required|optional]
//!     template "./..."                              # required
//! ```
//!
//! ## See also
//!
//! - `docs/canonical-semantics.md` — MCP grammar.
//! - `lazuli_ir::nodes::mcp` — typed lowering target.

use super::super::common::{SourceLine, is_trivia, line_error, unquote_lzx_value};
use super::super::error::ParseError;
use super::{
    AGENT_INDENT_AGENT_CHILD, AGENT_INDENT_FEATURE_CHILD, AGENT_INDENT_GRANDCHILD,
    AGENT_INDENT_GREAT_GRANDCHILD,
};

use crate::ast::Span;

/// MCP bucket cycle — parse a feature-scoped `mcp_server <name>` block.
/// Header line is `mcp_server <ident>` at indent 2. Children at indent 4:
/// `transport <stdio|http_sse|http_streamable>` (required, closed-catalog),
/// `scope feature.<name>` (optional), `auth bearer env.<NAME>` (optional),
/// `metadata` sub-block (optional, single occurrence), `tool <name>`
/// sub-blocks (repeatable), `resource <name>` sub-blocks (repeatable),
/// `prompt <name>` sub-blocks (repeatable).
pub(super) fn parse_mcp_server(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(crate::ast::McpServer, usize), ParseError> {
    let header = &lines[start];
    let header_trimmed = header.text.trim_start();
    let name = header_trimmed
        .strip_prefix("mcp_server ")
        .map(|rest| rest.trim().to_owned())
        .ok_or_else(|| line_error(header, "mcp_server header must be `mcp_server <name>`"))?;
    if name.is_empty() {
        return Err(line_error(header, "mcp_server header requires a name"));
    }

    let mut transport: Option<String> = None;
    let mut scope_feature: Option<String> = None;
    let mut auth: Option<String> = None;
    let mut metadata = crate::ast::McpServerMetadata::default();
    let mut tools: Vec<crate::ast::McpTool> = Vec::new();
    let mut resources: Vec<crate::ast::McpResource> = Vec::new();
    let mut prompts: Vec<crate::ast::McpPrompt> = Vec::new();
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent <= AGENT_INDENT_FEATURE_CHILD {
            break;
        }
        if line.indent != AGENT_INDENT_AGENT_CHILD {
            return Err(line_error(
                line,
                "`mcp_server` body children use four-space indentation",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("transport ") {
            let value = rest.trim().to_owned();
            transport = Some(value);
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("scope ") {
            scope_feature = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("auth ") {
            auth = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if trimmed == "metadata" {
            let (parsed, next) = parse_mcp_server_metadata(lines, i)?;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            metadata = parsed;
            i = next;
        } else if trimmed.starts_with("tool ") {
            let (parsed, next) = parse_mcp_tool(lines, i)?;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            tools.push(parsed);
            i = next;
        } else if trimmed.starts_with("resource ") {
            let (parsed, next) = parse_mcp_resource(lines, i)?;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            resources.push(parsed);
            i = next;
        } else if trimmed.starts_with("prompt ") {
            let (parsed, next) = parse_mcp_prompt(lines, i)?;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            prompts.push(parsed);
            i = next;
        } else {
            return Err(line_error(
                line,
                "`mcp_server` children are `transport`, `scope`, `auth`, `metadata`, `tool <name>`, `resource <name>`, or `prompt <name>`",
            ));
        }
    }

    let transport = transport.ok_or_else(|| {
        line_error(
            header,
            "`mcp_server` requires a `transport <stdio|http_sse|http_streamable>` declaration",
        )
    })?;

    Ok((
        crate::ast::McpServer {
            name,
            transport,
            scope_feature,
            auth,
            metadata,
            tools,
            resources,
            prompts,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

/// Parse `metadata` sub-block at indent 4 inside an `mcp_server`.
/// Children at indent 6: `name "..."`, `description "..."`, `version "..."`.
fn parse_mcp_server_metadata(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(crate::ast::McpServerMetadata, usize), ParseError> {
    let mut out = crate::ast::McpServerMetadata::default();
    let mut i = start + 1;
    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();
        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent <= AGENT_INDENT_AGENT_CHILD {
            break;
        }
        if line.indent != AGENT_INDENT_GRANDCHILD {
            return Err(line_error(
                line,
                "`metadata` children use six-space indentation",
            ));
        }
        if let Some(rest) = trimmed.strip_prefix("name ") {
            out.name = Some(unquote_lzx_value(rest.trim()).to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("description ") {
            out.description = Some(unquote_lzx_value(rest.trim()).to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("version ") {
            out.version = Some(unquote_lzx_value(rest.trim()).to_owned());
        } else {
            return Err(line_error(
                line,
                "`metadata` children are `name \"...\"`, `description \"...\"`, or `version \"...\"`",
            ));
        }
        i += 1;
    }
    Ok((out, i))
}

/// Parse `tool <name>` sub-block at indent 4 inside an `mcp_server`.
fn parse_mcp_tool(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(crate::ast::McpTool, usize), ParseError> {
    let header = &lines[start];
    let header_trimmed = header.text.trim_start();
    let name = header_trimmed
        .strip_prefix("tool ")
        .map(|rest| rest.trim().to_owned())
        .ok_or_else(|| line_error(header, "tool header must be `tool <name>`"))?;
    if name.is_empty() {
        return Err(line_error(header, "tool header requires a name"));
    }
    let mut description: Option<String> = None;
    let mut params: Vec<crate::ast::McpParam> = Vec::new();
    let mut returns: Option<String> = None;
    let mut handler: Option<String> = None;
    let mut policy: Option<String> = None;
    let mut last_end = header.end;
    let mut i = start + 1;
    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();
        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent <= AGENT_INDENT_AGENT_CHILD {
            break;
        }
        if line.indent != AGENT_INDENT_GRANDCHILD {
            return Err(line_error(
                line,
                "`tool` body children use six-space indentation",
            ));
        }
        if let Some(rest) = trimmed.strip_prefix("description ") {
            description = Some(unquote_lzx_value(rest.trim()).to_owned());
            last_end = line.end;
            i += 1;
        } else if trimmed == "params" {
            let (parsed, next) = parse_mcp_params(lines, i)?;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            params = parsed;
            i = next;
        } else if let Some(rest) = trimmed.strip_prefix("returns ") {
            returns = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("handler ") {
            handler = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("policy ") {
            policy = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else {
            return Err(line_error(
                line,
                "`tool` children are `description`, `params`, `returns`, `handler`, or `policy`",
            ));
        }
    }
    let handler = handler
        .ok_or_else(|| line_error(header, "`tool` requires a `handler @fn.<name>` declaration"))?;
    Ok((
        crate::ast::McpTool {
            name,
            description,
            params,
            returns,
            handler,
            policy,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

/// Parse `resource <name>` sub-block at indent 4 inside an `mcp_server`.
fn parse_mcp_resource(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(crate::ast::McpResource, usize), ParseError> {
    let header = &lines[start];
    let header_trimmed = header.text.trim_start();
    let name = header_trimmed
        .strip_prefix("resource ")
        .map(|rest| rest.trim().to_owned())
        .ok_or_else(|| line_error(header, "resource header must be `resource <name>`"))?;
    if name.is_empty() {
        return Err(line_error(header, "resource header requires a name"));
    }
    let mut uri_template: Option<String> = None;
    let mut mime: Option<String> = None;
    let mut handler: Option<String> = None;
    let mut policy: Option<String> = None;
    let mut last_end = header.end;
    let mut i = start + 1;
    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();
        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent <= AGENT_INDENT_AGENT_CHILD {
            break;
        }
        if line.indent != AGENT_INDENT_GRANDCHILD {
            return Err(line_error(
                line,
                "`resource` body children use six-space indentation",
            ));
        }
        if let Some(rest) = trimmed.strip_prefix("uri_template ") {
            uri_template = Some(unquote_lzx_value(rest.trim()).to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("mime ") {
            mime = Some(unquote_lzx_value(rest.trim()).to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("handler ") {
            handler = Some(rest.trim().to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("policy ") {
            policy = Some(rest.trim().to_owned());
        } else {
            return Err(line_error(
                line,
                "`resource` children are `uri_template`, `mime`, `handler`, or `policy`",
            ));
        }
        last_end = line.end;
        i += 1;
    }
    let uri_template = uri_template.ok_or_else(|| {
        line_error(
            header,
            "`resource` requires a `uri_template \"...\"` declaration",
        )
    })?;
    let handler = handler.ok_or_else(|| {
        line_error(
            header,
            "`resource` requires a `handler @fn.<name>` declaration",
        )
    })?;
    Ok((
        crate::ast::McpResource {
            name,
            uri_template,
            mime,
            handler,
            policy,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

/// Parse `prompt <name>` sub-block at indent 4 inside an `mcp_server`.
fn parse_mcp_prompt(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(crate::ast::McpPrompt, usize), ParseError> {
    let header = &lines[start];
    let header_trimmed = header.text.trim_start();
    let name = header_trimmed
        .strip_prefix("prompt ")
        .map(|rest| rest.trim().to_owned())
        .ok_or_else(|| line_error(header, "prompt header must be `prompt <name>`"))?;
    if name.is_empty() {
        return Err(line_error(header, "prompt header requires a name"));
    }
    let mut description: Option<String> = None;
    let mut params: Vec<crate::ast::McpParam> = Vec::new();
    let mut template: Option<String> = None;
    let mut last_end = header.end;
    let mut i = start + 1;
    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();
        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent <= AGENT_INDENT_AGENT_CHILD {
            break;
        }
        if line.indent != AGENT_INDENT_GRANDCHILD {
            return Err(line_error(
                line,
                "`prompt` body children use six-space indentation",
            ));
        }
        if let Some(rest) = trimmed.strip_prefix("description ") {
            description = Some(unquote_lzx_value(rest.trim()).to_owned());
            last_end = line.end;
            i += 1;
        } else if trimmed == "params" {
            let (parsed, next) = parse_mcp_params(lines, i)?;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            params = parsed;
            i = next;
        } else if let Some(rest) = trimmed.strip_prefix("template ") {
            template = Some(unquote_lzx_value(rest.trim()).to_owned());
            last_end = line.end;
            i += 1;
        } else {
            return Err(line_error(
                line,
                "`prompt` children are `description`, `params`, or `template`",
            ));
        }
    }
    let template = template.ok_or_else(|| {
        line_error(
            header,
            "`prompt` requires a `template \"./...\"` declaration",
        )
    })?;
    Ok((
        crate::ast::McpPrompt {
            name,
            description,
            params,
            template,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

/// Parse `params` sub-block at indent 6 inside a `tool` or `prompt`.
/// Children at indent 8 are `<name>: <type> [required|optional]`.
fn parse_mcp_params(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(Vec<crate::ast::McpParam>, usize), ParseError> {
    let mut out: Vec<crate::ast::McpParam> = Vec::new();
    let mut i = start + 1;
    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();
        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent <= AGENT_INDENT_GRANDCHILD {
            break;
        }
        if line.indent != AGENT_INDENT_GREAT_GRANDCHILD {
            return Err(line_error(
                line,
                "`params` rows use eight-space indentation",
            ));
        }
        let (name_part, after_colon) = trimmed.split_once(':').ok_or_else(|| {
            line_error(
                line,
                "`params` row must be `<name>: <type> [required|optional]`",
            )
        })?;
        let after = after_colon.trim();
        let (ty, required) = if let Some(stripped) = after.strip_suffix(" required") {
            (stripped.trim().to_owned(), true)
        } else if let Some(stripped) = after.strip_suffix(" optional") {
            (stripped.trim().to_owned(), false)
        } else {
            (after.to_owned(), false)
        };
        out.push(crate::ast::McpParam {
            name: name_part.trim().to_owned(),
            ty,
            required,
        });
        i += 1;
    }
    Ok((out, i))
}
