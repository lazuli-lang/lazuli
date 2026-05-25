//! Agents projection plus `--expand=tools` projection.
//!
//! Walks every `agent <name>` top-level block on the feature and
//! surfaces the typed agent contract: inputs, context, policy, rate
//! limit, output kind/discriminator, model parameters, prompt template,
//! tool list, eval cases, eval determinism (`pinned` vs
//! `nondeterministic`, derived from `temperature == 0 && seed.is_some()`),
//! safety policy, and the optional `expose http` block.
//!
//! `inspect_agent_tools_projection` materialises the per-agent dispatch
//! graph used by `--expand=tools`. Each tool reference is classified by
//! kind (`query.list`, `query.lookup`, `query.sql`, `query.view`,
//! `query`, `command`, `api`, `adapter`) and scope (`local`,
//! `cross_feature`, `adapter`). Cross-feature resolution of effects /
//! policies / PII lives in doctor; this projection records the
//! structural facts visible from the file alone, with a derived
//! effect (`read` / `write` / `unknown`) following the kind heuristic.

use super::super::expand::leading_spaces;
use super::super::text_walkers::{
    command_input_names, direct_child_value, named_top_block_name, strip_quotes, top_level_blocks,
};
use super::super::{
    InspectAgent, InspectAgentExpose, InspectAgentToolBinding, InspectAgentToolsEntry,
};

pub(in crate::commands::inspect) fn inspect_agents(lines: &[String]) -> Vec<InspectAgent> {
    let mut agents = Vec::new();

    for block in top_level_blocks(lines, "agent ") {
        let name = named_top_block_name(block[0].trim_start())
            .unwrap_or("unknown")
            .to_owned();
        let inputs = command_input_names(block);
        let context = direct_child_value(block, "context ")
            .as_deref()
            .map(strip_quotes);
        let policy = direct_child_value(block, "policy ");
        let rate_limit = direct_child_value(block, "rate_limit ")
            .as_deref()
            .map(strip_quotes);
        let output_raw = direct_child_value(block, "output ");
        let (output_kind, output_discriminator) = classify_agent_output(output_raw.as_deref());
        let model = direct_child_value(block, "model ");
        let prompt = direct_child_value(block, "prompt ")
            .as_deref()
            .map(strip_quotes);
        // Cut A — tool entries live as indent-6 lines under a `tools`
        // child block. The legacy `tools <comma-list>` shorthand never
        // existed in the canonical syntax; the previous text extractor
        // returned `None` for the canonical form. This walker handles
        // both for safety while older fixtures linger.
        let tools = collect_agent_block_entries(block, "tools");
        let evals = collect_agent_eval_case_names(block);
        let safety = direct_child_value(block, "safety ");

        let temperature = direct_child_value(block, "temperature ");
        let max_tokens = direct_child_value(block, "max_tokens ");
        let top_p = direct_child_value(block, "top_p ");
        let seed = direct_child_value(block, "seed ");

        let eval_determinism = if evals.is_empty() {
            None
        } else {
            let temp_zero = temperature.as_deref().and_then(|s| s.parse::<f64>().ok()) == Some(0.0);
            let seed_present = seed.is_some();
            Some(if temp_zero && seed_present {
                "pinned"
            } else {
                "nondeterministic"
            })
        };

        let expose_http = collect_agent_expose(block);

        agents.push(InspectAgent {
            name,
            inputs,
            context,
            policy,
            rate_limit,
            output: output_raw,
            output_kind,
            output_discriminator,
            model,
            temperature,
            max_tokens,
            top_p,
            seed,
            prompt,
            tools,
            evals,
            eval_determinism,
            safety,
            expose_http,
            origin: "agent",
        });
    }

    agents
}

pub(in crate::commands::inspect) fn inspect_agent_tools_projection(
    agents: &[InspectAgent],
) -> Vec<InspectAgentToolsEntry> {
    agents
        .iter()
        .filter(|agent| !agent.tools.is_empty())
        .map(|agent| InspectAgentToolsEntry {
            agent: agent.name.clone(),
            tools: agent
                .tools
                .iter()
                .map(|reference| tool_binding_for_reference(reference))
                .collect(),
        })
        .collect()
}

fn tool_binding_for_reference(reference: &str) -> InspectAgentToolBinding {
    let trimmed = reference.trim();
    if trimmed.starts_with("@tool.") {
        return InspectAgentToolBinding {
            reference: trimmed.to_owned(),
            kind: "adapter",
            scope: "adapter",
            derived_effect: "unknown",
        };
    }

    let segments: Vec<&str> = trimmed.split('.').collect();
    let (kind, scope) = match segments.as_slice() {
        ["query", "list", _] => ("query.list", "local"),
        ["query", "lookup", _] => ("query.lookup", "local"),
        ["query", "sql", _] => ("query.sql", "local"),
        ["query", "view", _] => ("query.view", "local"),
        ["query", _] => ("query", "local"),
        ["command", _] => ("command", "local"),
        ["api", _] => ("api", "local"),
        [_feature, "query", "list", _] => ("query.list", "cross_feature"),
        [_feature, "query", "lookup", _] => ("query.lookup", "cross_feature"),
        [_feature, "query", "sql", _] => ("query.sql", "cross_feature"),
        [_feature, "query", "view", _] => ("query.view", "cross_feature"),
        [_feature, "query", _] => ("query", "cross_feature"),
        [_feature, "command", _] => ("command", "cross_feature"),
        [_feature, "api", _] => ("api", "cross_feature"),
        _ => ("unknown", "unknown"),
    };

    let derived_effect = match kind {
        "command" => "write",
        "query.list" | "query.lookup" | "query.sql" | "query.view" | "query" => "read",
        _ => "unknown",
    };

    InspectAgentToolBinding {
        reference: trimmed.to_owned(),
        kind,
        scope,
        derived_effect,
    }
}

/// Derive `(output_kind, output_discriminator)` from the raw text after
/// `output `. The discriminator name surfaces for the two discriminated
/// shapes plus the bare-record form (lowering disambiguates record vs
/// text via the workspace IR; we record the symbol verbatim).
fn classify_agent_output(raw: Option<&str>) -> (Option<&'static str>, Option<String>) {
    let Some(raw) = raw else { return (None, None) };
    let trimmed = raw.trim();
    if let Some(rest) = trimmed.strip_prefix("stream ") {
        return (Some("stream"), Some(rest.trim().to_owned()));
    }
    if let Some(rest) = trimmed.strip_prefix("discriminator ") {
        return (Some("discriminated_enum"), Some(rest.trim().to_owned()));
    }
    if trimmed.is_empty() {
        return (None, None);
    }
    // Bare type ref. Text builtins keep `text`; PascalCase identifiers
    // (likely an author-defined record/enum) carry the symbol forward so
    // doctor's `agent_discriminator_target_invalid_diagnostics` and the
    // expand pass can interpret. The `text` label stays — lowering
    // promotes to `discriminated_record` when records resolve.
    let first = trimmed.chars().next();
    let looks_like_symbol = first.is_some_and(|c| c.is_ascii_uppercase());
    let kind = if matches!(
        trimmed,
        "Text" | "Integer" | "Boolean" | "Decimal" | "Date" | "DateTime" | "Json" | "ID"
    ) {
        "text"
    } else if looks_like_symbol {
        // Could be a record-with-discriminator (DiscriminatedRecord) or
        // a plain record reference; expand-pass disambiguates. We label
        // as `text` here to keep the file-local pass single-pass; the
        // symbol is surfaced via `output_discriminator`.
        "text"
    } else {
        "text"
    };
    let discriminator = if looks_like_symbol {
        Some(trimmed.to_owned())
    } else {
        None
    };
    (Some(kind), discriminator)
}

/// Walk the agent body for `<block> NEWLINE\n   <entry>\n   ...` and
/// return the indent-6 children as their raw trimmed source. Used for
/// both the `tools` and a future cut's other list-shaped children.
fn collect_agent_block_entries(block: &[String], parent: &str) -> Vec<String> {
    let Some(parent_indent) = block.first().map(|line| leading_spaces(line)) else {
        return Vec::new();
    };
    let child_indent = parent_indent + 2;
    let grandchild_indent = child_indent + 2;

    let mut entries = Vec::new();
    let mut in_block = false;
    for line in block.iter().skip(1) {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let leading = leading_spaces(line);
        if leading <= parent_indent {
            break;
        }
        if leading == child_indent {
            in_block = trimmed == parent;
            continue;
        }
        if in_block && leading == grandchild_indent {
            entries.push(trimmed.to_owned());
        }
    }
    entries
}

/// Walk the agent body for an `expose http` block and surface the
/// declared method/path/route/audience/rate_limit. Cut A.7's
/// inspect-side observable; doctor handles cross-feature resolution.
fn collect_agent_expose(block: &[String]) -> Option<InspectAgentExpose> {
    let parent_indent = block.first().map(|line| leading_spaces(line))?;
    let child_indent = parent_indent + 2;
    let grandchild_indent = child_indent + 2;

    let mut in_expose = false;
    let mut method: Option<String> = None;
    let mut path: Option<String> = None;
    let mut route_slots: Vec<String> = Vec::new();
    let mut audience: Option<String> = None;
    let mut rate_limit_override: Option<String> = None;

    for line in block.iter().skip(1) {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let leading = leading_spaces(line);
        if leading <= parent_indent {
            break;
        }
        if leading == child_indent {
            in_expose = trimmed == "expose http";
            continue;
        }
        if in_expose && leading == grandchild_indent {
            if let Some(rest) = trimmed.strip_prefix("method ") {
                method = Some(rest.trim().to_ascii_uppercase());
            } else if let Some(rest) = trimmed.strip_prefix("path ") {
                path = Some(strip_quotes(rest.trim()).to_owned());
            } else if let Some(rest) = trimmed.strip_prefix("route ") {
                if let Some((name_part, _)) = rest.split_once(':') {
                    route_slots.push(name_part.trim().to_owned());
                }
            } else if let Some(rest) = trimmed.strip_prefix("audience ") {
                audience = Some(rest.trim().to_owned());
            } else if let Some(rest) = trimmed.strip_prefix("rate_limit ") {
                rate_limit_override = Some(strip_quotes(rest.trim()).to_owned());
            }
        }
    }

    let method = method?;
    let path = path?;
    Some(InspectAgentExpose {
        method,
        path,
        route_slots,
        audience,
        rate_limit_override,
    })
}

/// Walk the agent body for `evals` and return the list of eval `case`
/// names declared inside.
fn collect_agent_eval_case_names(block: &[String]) -> Vec<String> {
    let Some(parent_indent) = block.first().map(|line| leading_spaces(line)) else {
        return Vec::new();
    };
    let child_indent = parent_indent + 2;
    let grandchild_indent = child_indent + 2;

    let mut cases = Vec::new();
    let mut in_block = false;
    for line in block.iter().skip(1) {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let leading = leading_spaces(line);
        if leading <= parent_indent {
            break;
        }
        if leading == child_indent {
            in_block = trimmed == "evals";
            continue;
        }
        if in_block && leading == grandchild_indent {
            if let Some(rest) = trimmed.strip_prefix("case ") {
                let name = rest.split_whitespace().next().unwrap_or("").to_owned();
                if !name.is_empty() {
                    cases.push(name);
                }
            }
        }
    }
    cases
}
