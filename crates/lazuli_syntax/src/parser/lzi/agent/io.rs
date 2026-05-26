//! `agent.input` / `agent.output` / `agent.tools` sub-blocks.
//!
//! Extracted from the original monolithic `agent.rs`. Owns the
//! slot-typed input list, the closed-catalog output kind
//! (`stream <T>` | `discriminator <Enum>` | bare `<T>`), and the
//! `tools` reference list.

use super::super::super::common::{SourceLine, is_trivia, line_error};
use super::super::super::error::ParseError;
use super::super::{AGENT_INDENT_AGENT_CHILD, AGENT_INDENT_GRANDCHILD};
use crate::ast::{AgentInputSlot, AgentOutput, AgentTool, Span};

pub(super) fn parse_agent_input(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(Vec<AgentInputSlot>, usize), ParseError> {
    let mut slots = Vec::new();
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
                "agent input slots use six-space indentation",
            ));
        }

        slots.push(parse_agent_input_slot(line)?);
        i += 1;
    }

    Ok((slots, i))
}

fn parse_agent_input_slot(line: &SourceLine<'_>) -> Result<AgentInputSlot, ParseError> {
    let trimmed = line.text.trim_start();
    let (name_part, rest) = trimmed.split_once(':').ok_or_else(|| {
        line_error(
            line,
            "input slot must be `<name>: <type> [required|optional]`",
        )
    })?;

    let name = name_part.trim();
    if name.is_empty() {
        return Err(line_error(line, "input slot is missing a name"));
    }

    let mut required = false;
    let mut optional = false;
    let mut type_tokens: Vec<&str> = Vec::new();
    for token in rest.split_whitespace() {
        match token {
            "required" => required = true,
            "optional" => optional = true,
            other => type_tokens.push(other),
        }
    }

    if type_tokens.is_empty() {
        return Err(line_error(line, "input slot is missing a type"));
    }
    if required && optional {
        return Err(line_error(
            line,
            "input slot cannot be both `required` and `optional`",
        ));
    }

    Ok(AgentInputSlot {
        name: name.to_owned(),
        type_text: type_tokens.join(" "),
        required,
        optional,
        span: Span::new(line.start, line.end),
    })
}

/// Parse the value side of an `output ...` declaration. The leading
/// `output ` prefix has already been consumed by the caller.
pub(super) fn parse_agent_output_value(
    line: &SourceLine<'_>,
    rest: &str,
) -> Result<AgentOutput, ParseError> {
    let trimmed = rest.trim();
    if let Some(rest) = trimmed.strip_prefix("stream ") {
        let type_ref = rest.trim();
        if type_ref.is_empty() {
            return Err(line_error(line, "`output stream` requires a type"));
        }
        if type_ref.split_whitespace().next() == Some("discriminator") {
            return Err(line_error(
                line,
                "`output stream` cannot also carry `discriminator`; pick one form",
            ));
        }
        Ok(AgentOutput::Stream(type_ref.to_owned()))
    } else if let Some(rest) = trimmed.strip_prefix("discriminator ") {
        let target = rest.trim();
        if target.is_empty() {
            return Err(line_error(line, "`output discriminator` requires a type"));
        }
        if target.split_whitespace().count() > 1 {
            return Err(line_error(
                line,
                "`output discriminator` accepts a single enum reference",
            ));
        }
        Ok(AgentOutput::Discriminator(target.to_owned()))
    } else if trimmed.is_empty() {
        Err(line_error(line, "`output` requires a value"))
    } else {
        // Bare type ref form: `output <Type>` (legacy default; lowering
        // disambiguates record-with-discriminator vs Text).
        if trimmed.split_whitespace().count() > 1 {
            return Err(line_error(
                line,
                "`output <Type>` accepts a single type reference",
            ));
        }
        Ok(AgentOutput::Plain(trimmed.to_owned()))
    }
}

pub(super) fn parse_agent_tools(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(Vec<AgentTool>, usize), ParseError> {
    let mut tools = Vec::new();
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
                "tool entries use six-space indentation, one reference per line",
            ));
        }

        if trimmed.split_whitespace().count() != 1 {
            return Err(line_error(
                line,
                "each tool entry is a single qualified reference",
            ));
        }

        tools.push(AgentTool {
            reference: trimmed.to_owned(),
            span: Span::new(line.start, line.end),
        });
        i += 1;
    }

    Ok((tools, i))
}

#[cfg(test)]
mod io_tests {
    use super::super::super::parse_feature_skeletons;
    use crate::AgentOutput;

    #[test]
    fn agent_with_tools_block_parses() {
        let source = r#"
feature customer
  agent triage_customer
    input
      message: Text required
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./prompts/triage.md"
    tools
      customer.query.by_id
      customer.query.list
      @tool.web_search
"#;

        let features = parse_feature_skeletons(source).unwrap();
        assert_eq!(features.len(), 1);
        assert_eq!(features[0].name, "customer");
        assert_eq!(features[0].agents.len(), 1);

        let agent = &features[0].agents[0];
        assert_eq!(agent.name, "triage_customer");
        assert_eq!(agent.input.len(), 1);
        assert_eq!(agent.input[0].name, "message");
        assert_eq!(agent.input[0].type_text, "Text");
        assert!(agent.input[0].required);
        assert_eq!(
            agent.policy.as_deref(),
            Some(&["@policy.read".to_owned()][..])
        );
        assert_eq!(agent.model.as_deref(), Some("@llm.default"));
        assert_eq!(agent.prompt.as_deref(), Some("./prompts/triage.md"));
        assert_eq!(agent.output, Some(AgentOutput::Stream("Text".to_owned())));
        assert_eq!(agent.tools.len(), 3);
        assert_eq!(agent.tools[0].reference, "customer.query.by_id");
        assert_eq!(agent.tools[1].reference, "customer.query.list");
        assert_eq!(agent.tools[2].reference, "@tool.web_search");
    }

    #[test]
    fn agent_with_discriminator_output_parses() {
        let source = r#"
feature customer_support
  agent classify_intent
    input
      message: Text required
    policy @policy.read
    output discriminator Intent
    model @llm.classifier
    temperature 0
    seed 42
    prompt "./prompts/classify_intent.md"
"#;

        let features = parse_feature_skeletons(source).unwrap();
        let agent = &features[0].agents[0];
        assert_eq!(
            agent.output,
            Some(AgentOutput::Discriminator("Intent".to_owned()))
        );
    }

    #[test]
    fn agent_with_discriminated_record_output_parses() {
        // The parser sees `output Action` as a bare type reference and emits
        // `Plain`. Lowering disambiguates record-with-discriminator vs Text.
        let source = r#"
feature customer
  agent extract_action
    input
      message: Text required
    policy @policy.read
    output Action
    model @llm.default
    prompt "./prompts/extract.md"
"#;

        let features = parse_feature_skeletons(source).unwrap();
        let agent = &features[0].agents[0];
        assert_eq!(agent.output, Some(AgentOutput::Plain("Action".to_owned())));
    }

    #[test]
    fn agent_rejects_unknown_output_kind() {
        let source = r#"
feature customer
  agent bad_output
    input
      message: Text required
    policy @policy.read
    output stream discriminator Intent
    model @llm.default
    prompt "./prompts/x.md"
"#;

        let err = parse_feature_skeletons(source).unwrap_err();
        assert!(
            err.to_string().contains("output stream"),
            "error should mention `output stream` mis-shape: {err}"
        );
    }

    #[test]
    fn agent_input_optional_slot_parses() {
        let source = r#"
feature customer
  agent triage
    input
      message: Text required
      hint: Text optional
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./prompts/triage.md"
"#;

        let features = parse_feature_skeletons(source).unwrap();
        let agent = &features[0].agents[0];
        assert_eq!(agent.input.len(), 2);
        assert!(agent.input[0].required);
        assert!(!agent.input[0].optional);
        assert!(!agent.input[1].required);
        assert!(agent.input[1].optional);
    }
}
