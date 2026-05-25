//! Resource-scoped `retention <duration> then <action>` line parser.
//!
//! `retention` is a single-line decorator under a `resource <Name>`
//! block. The duration is opaque text (`7y`, `30d`, `12h`) — the
//! analyzer cross-checks the format, not the parser. The action is a
//! closed three-way enum: `anonymize`, `delete`, `archive`.
//!
//! ```text
//! resource Customer
//!   retention 7y then anonymize
//! ```
//!
//! Visibility: `parse_resource_retention` is `pub(super)` — the only
//! caller is `handle_resource_retention` in `resource/mod.rs`.

use super::super::super::common::{SourceLine, line_error, line_error_owned};
use super::super::super::error::ParseError;

use crate::ast::{ResourceRetention, ResourceRetentionAction, Span};

pub(super) fn parse_resource_retention(
    line: &SourceLine<'_>,
    rest: &str,
) -> Result<ResourceRetention, ParseError> {
    let rest = rest.trim();
    let (duration, action) = rest.split_once(" then ").ok_or_else(|| {
        line_error(
            line,
            "`retention` requires `<duration> then <action>` (e.g. `retention 7y then anonymize`)",
        )
    })?;
    let action = match action.trim() {
        "anonymize" => ResourceRetentionAction::Anonymize,
        "delete" => ResourceRetentionAction::Delete,
        "archive" => ResourceRetentionAction::Archive,
        other => {
            return Err(line_error_owned(
                line,
                format!(
                    "`retention then` requires `anonymize`, `delete`, or `archive` (got `{other}`)"
                ),
            ));
        }
    };
    Ok(ResourceRetention {
        duration: duration.trim().to_owned(),
        action,
        span: Span::new(line.start, line.end),
    })
}
