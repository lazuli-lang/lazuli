//! Feature-skeleton walker: the hand-rolled line-stream parser that
//! walks every `feature <name>` block in a `.lzi` source and dispatches
//! to sibling parsers for each feature child (agent, auth, job, command,
//! resource, ...). Re-exported from `parser/lzi/mod.rs`.
//!
//! See the parent module docs for ABI notes; the walker stays here so
//! `lzi/mod.rs` can be the thin module root the rails-style split asks
//! for.

mod skeleton;

use super::super::common::{is_trivia, line_error, source_lines};
use super::super::error::ParseError;

use crate::ast::FeatureSkeleton;

use skeleton::parse_feature_skeleton;

pub(crate) const AGENT_INDENT_FEATURE_CHILD: usize = 2;
pub(crate) const AGENT_INDENT_AGENT_CHILD: usize = 4;
pub(crate) const AGENT_INDENT_GRANDCHILD: usize = 6;
pub(crate) const AGENT_INDENT_GREAT_GRANDCHILD: usize = 8;

/// Parse every `feature <name>` block in a `.lzi` source, returning a
/// skeleton that lists only the agents inside each feature. Other feature
/// children are not surfaced — Cut A intentionally narrows.
///
/// ## Examples
///
/// ```
/// use lazuli_syntax::parse_feature_skeletons;
///
/// let src = "feature customer\nfeature billing\n";
/// let features = parse_feature_skeletons(src).expect("parses");
/// assert_eq!(features.len(), 2);
/// assert_eq!(features[0].name, "customer");
/// assert_eq!(features[1].name, "billing");
/// ```
pub fn parse_feature_skeletons(source: &str) -> Result<Vec<FeatureSkeleton>, ParseError> {
    let lines = source_lines(source);
    let mut features = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            i += 1;
            continue;
        }

        if line.indent == 0 {
            if let Some(rest) = trimmed.strip_prefix("feature ") {
                let name = rest.trim().to_owned();
                if name.is_empty() {
                    return Err(line_error(
                        line,
                        "feature header requires a name: `feature <name>`",
                    ));
                }
                let (feature, next) = parse_feature_skeleton(&lines, i, name)?;
                features.push(feature);
                i = next;
                continue;
            }
            // Other top-level constructs (`app`, `workspace`, `contract`, ...)
            // are not parsed by this slice. Skip the line.
            i += 1;
            continue;
        }

        // Stray indented top-level content — outside any feature. Skip.
        i += 1;
    }

    Ok(features)
}
