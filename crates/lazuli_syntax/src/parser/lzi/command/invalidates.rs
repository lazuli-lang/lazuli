//! `invalidates query.<name>[(args)]` — block and single-line forms.
//!
//! `parse_invalidates_entry` is `pub(crate)` because the `lzi/mod.rs`
//! feature skeleton walker re-exports it for use outside the command
//! grammar (it surfaces as a stand-alone entry in `query` blocks too).

use super::super::super::common::{SourceLine, is_trivia, line_error};
use super::super::super::error::ParseError;
use super::super::{
    AGENT_INDENT_AGENT_CHILD, AGENT_INDENT_GRANDCHILD, parse_named_args, split_call_signature,
};

use crate::ast::{InvalidatesDecl, Span};

pub(in crate::parser::lzi) fn parse_invalidates_block(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(Vec<InvalidatesDecl>, usize), ParseError> {
    let mut out: Vec<InvalidatesDecl> = Vec::new();
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
                "`invalidates` children use six-space indentation",
            ));
        }
        out.push(parse_invalidates_entry(line, trimmed)?);
        i += 1;
    }
    Ok((out, i))
}

pub(crate) fn parse_invalidates_entry(
    line: &SourceLine<'_>,
    rest: &str,
) -> Result<InvalidatesDecl, ParseError> {
    let rest = rest.trim();
    // `query.list` or `query.by_id(id: route.id)`.
    if rest.contains('(') {
        let (query, args_part) = split_call_signature(line, rest)?;
        let args = parse_named_args(line, args_part)?;
        Ok(InvalidatesDecl {
            query: query.to_owned(),
            args,
            span: Span::new(line.start, line.end),
        })
    } else {
        if rest.is_empty() {
            return Err(line_error(
                line,
                "`invalidates` entry requires a query reference",
            ));
        }
        Ok(InvalidatesDecl {
            query: rest.to_owned(),
            args: Vec::new(),
            span: Span::new(line.start, line.end),
        })
    }
}

// =============================================================================
// codegen-correctness-cycle-2 cell IA1 — `invalidates` block parser tests.
//
// Authoring syntax (cross-resource cache invalidation under `command`):
//
//   command save_X
//     ...
//     effect updates X
//     invalidates
//       query.lookup_my_X
//       feature_y.query.list_by_x
//
// The block form is the canonical authoring shape — each entry is a
// qualified query reference (bare `<query_name>` segments today; the
// optional leading feature segment routes to a sibling feature).
// Same-feature single-line form (`invalidates query.<name>`) is kept
// for backward compatibility but not exercised here.
// =============================================================================
#[cfg(test)]
mod invalidates_parser_tests {
    use super::super::super::parse_feature_skeletons;

    #[test]
    fn invalidates_block_parses_same_and_cross_feature() {
        let source = "feature customer\n  command save_X\n    input\n      id: ID required\n    policy @policy.update\n    updates Customer\n      tier = input.tier\n    invalidates\n      query.lookup_my_X\n      feature_y.query.list_by_x\n";
        let features = parse_feature_skeletons(source).expect("parses");
        let command = &features[0].commands[0];

        assert_eq!(command.name, "save_X");
        assert_eq!(command.invalidates.len(), 2);
        // Same-feature entry — bare `query.<name>` form.
        assert_eq!(command.invalidates[0].query, "query.lookup_my_X");
        assert!(command.invalidates[0].args.is_empty());
        // Cross-feature entry — `<feature>.query.<name>` form.
        assert_eq!(command.invalidates[1].query, "feature_y.query.list_by_x");
        assert!(command.invalidates[1].args.is_empty());
    }

    #[test]
    fn invalidates_block_parses_with_named_args() {
        let source = "feature customer\n  command reassign\n    route id: ID\n    input\n      owner_id: ID required\n    policy @policy.update\n    updates Customer\n      owner_id = input.owner_id\n    invalidates\n      query.list\n      query.by_id(id: route.id)\n";
        let features = parse_feature_skeletons(source).expect("parses");
        let invalidates = &features[0].commands[0].invalidates;

        assert_eq!(invalidates.len(), 2);
        assert_eq!(invalidates[0].query, "query.list");
        assert_eq!(invalidates[1].query, "query.by_id");
        assert_eq!(invalidates[1].args.len(), 1);
        assert_eq!(invalidates[1].args[0].name, "id");
        assert_eq!(invalidates[1].args[0].value, "route.id");
    }

    #[test]
    fn invalidates_block_requires_grandchild_indent() {
        // Entries at indent 4 (sibling indent) fall back to the
        // command-child dispatcher and surface its "children are …"
        // diagnostic. The grammar gate is: only indent-6 (grandchild)
        // lines after `invalidates` are entries.
        let source = "feature customer\n  command save_X\n    policy @policy.update\n    updates Customer\n    invalidates\n    query.list\n";
        let err = parse_feature_skeletons(source).unwrap_err();
        let msg = format!("{:?}", err);
        assert!(
            msg.contains("invalidates") || msg.contains("`command` children"),
            "expected grammar diagnostic, got: {msg}"
        );
    }

    #[test]
    fn invalidates_block_rejects_unclosed_call() {
        // A call expression that opens `(` but never closes is rejected
        // up-front — covers the per-entry parse without depending on the
        // analyzer's downstream resolution pass.
        let source = "feature customer\n  command save_X\n    policy @policy.update\n    updates Customer\n    invalidates\n      query.by_id(id: route.id\n";
        let err = parse_feature_skeletons(source).unwrap_err();
        let msg = format!("{:?}", err);
        assert!(
            msg.contains(")") || msg.contains("call expression"),
            "expected unclosed-call diagnostic, got: {msg}"
        );
    }
}

