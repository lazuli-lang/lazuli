//! `auth oauth <provider>` sub-block — adapter (closed catalog).
//!
//! Extracted from the original monolithic `auth.rs`.

use super::super::super::common::{SourceLine, is_trivia, line_error};
use super::super::super::error::ParseError;
use super::super::{AGENT_INDENT_AGENT_CHILD, AGENT_INDENT_GRANDCHILD};
use crate::ast::{AuthOAuthProvider, Span};

pub(super) fn parse_auth_oauth(
    lines: &[SourceLine<'_>],
    start: usize,
    provider: String,
) -> Result<(AuthOAuthProvider, usize), ParseError> {
    let header = &lines[start];
    let mut adapter: Option<String> = None;
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
                "`auth oauth` children use six-space indentation",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("adapter ") {
            adapter = Some(rest.trim().to_owned());
        } else {
            return Err(line_error(line, "`auth oauth` children are `adapter`"));
        }

        last_end = line.end;
        i += 1;
    }

    let adapter = adapter.ok_or_else(|| {
        line_error(
            header,
            "`auth oauth` requires an `adapter @adapter.<name>` declaration",
        )
    })?;

    Ok((
        AuthOAuthProvider {
            provider,
            adapter,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

#[cfg(test)]
mod oauth_tests {
    use super::super::super::parse_feature_skeletons;

    #[test]
    fn auth_oauth_child_parses_multiple_providers() {
        let source = r#"
feature customer_auth
  auth
    identity Customer.email

    oauth google
      adapter @adapter.google_oauth

    oauth github
      adapter @adapter.github_oauth
"#;
        let features = parse_feature_skeletons(source).unwrap();
        let oauth = &features[0].auth.as_ref().expect("auth").oauth;
        assert_eq!(oauth.len(), 2);
        assert_eq!(oauth[0].provider, "google");
        assert_eq!(oauth[0].adapter, "@adapter.google_oauth");
        assert_eq!(oauth[1].provider, "github");
        assert_eq!(oauth[1].adapter, "@adapter.github_oauth");
    }
}
