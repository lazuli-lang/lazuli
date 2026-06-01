//! `defaults` block parser (Phase L Tier 4a).
//!
//! The `defaults` header sits at AGENT_INDENT_FEATURE_CHILD (2 spaces).
//! Children live at AGENT_INDENT_AGENT_CHILD (4 spaces):
//!
//!   defaults
//!     tenancy org
//!     timestamps
//!     policy_for jobs, webhooks: @actor.system
//!
//! Unknown children are a parse error so an LLM cannot author silent
//! typos like `timestapms` or `policy-for`.

use super::super::common::{SourceLine, is_trivia, line_error, unquote_lzx_value};
use super::super::error::ParseError;
use super::{AGENT_INDENT_AGENT_CHILD, AGENT_INDENT_FEATURE_CHILD};
use crate::ast::{DefaultsAudit, DefaultsPolicyFor, DefaultsTenancy, FeatureDefaults, Span};

pub(super) fn parse_defaults(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(FeatureDefaults, usize), ParseError> {
    let header = &lines[start];
    let mut tenancy: Option<DefaultsTenancy> = None;
    let mut timestamps = false;
    let mut policy_for: Vec<DefaultsPolicyFor> = Vec::new();
    let mut rate_limit: Option<String> = None;
    let mut audit: Option<DefaultsAudit> = None;
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
                "`defaults` body children use four-space indentation",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("tenancy ") {
            if tenancy.is_some() {
                return Err(line_error(
                    line,
                    "`defaults tenancy` may be declared at most once",
                ));
            }
            let axis = rest.trim();
            if axis.is_empty() {
                return Err(line_error(
                    line,
                    "`defaults tenancy` requires an axis (`org`, `team`, `none`, or a custom name)",
                ));
            }
            tenancy = Some(parse_defaults_tenancy(axis));
            last_end = line.end;
            i += 1;
        } else if trimmed == "timestamps" {
            if timestamps {
                return Err(line_error(
                    line,
                    "`defaults timestamps` may be declared at most once",
                ));
            }
            timestamps = true;
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("policy_for ") {
            policy_for.push(parse_defaults_policy_for(line, rest)?);
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("rate_limit ") {
            // 0004 — `defaults rate_limit "<spec>"` hoist. The spec stays a
            // string (string→struct axis deferred); per-command `rate_limit`
            // overrides at lowering.
            if rate_limit.is_some() {
                return Err(line_error(
                    line,
                    "`defaults rate_limit` may be declared at most once",
                ));
            }
            let spec = unquote_lzx_value(rest.trim()).trim().to_owned();
            if spec.is_empty() {
                return Err(line_error(
                    line,
                    "`defaults rate_limit` requires a spec (e.g. `defaults rate_limit \"60 per minute per actor\"`)",
                ));
            }
            rate_limit = Some(spec);
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("audit ") {
            // 0004 — `defaults audit default` hoist. Only `default` is
            // hoistable today; the per-command `audit <subjects>` / `audit
            // none` override is applied at lowering.
            if audit.is_some() {
                return Err(line_error(
                    line,
                    "`defaults audit` may be declared at most once",
                ));
            }
            let mode = rest.trim();
            match mode {
                "default" => audit = Some(DefaultsAudit::Default),
                _ => {
                    return Err(line_error(
                        line,
                        "`defaults audit` accepts only `default` (per-command `audit <subjects>` / `audit none` overrides)",
                    ));
                }
            }
            last_end = line.end;
            i += 1;
        } else {
            return Err(line_error(
                line,
                "`defaults` children are `tenancy`, `timestamps`, `policy_for <kinds>: <atom>`, `rate_limit \"<spec>\"`, or `audit default`",
            ));
        }
    }

    Ok((
        FeatureDefaults {
            tenancy,
            timestamps,
            policy_for,
            rate_limit,
            audit,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

pub(in super::super) fn parse_defaults_tenancy(axis: &str) -> DefaultsTenancy {
    match axis.trim() {
        "org" => DefaultsTenancy::Org,
        "team" => DefaultsTenancy::Team,
        "none" => DefaultsTenancy::None,
        other => DefaultsTenancy::Custom(other.to_owned()),
    }
}

fn parse_defaults_policy_for(
    line: &SourceLine<'_>,
    rest: &str,
) -> Result<DefaultsPolicyFor, ParseError> {
    let (kinds_part, atom_part) = rest.split_once(':').ok_or_else(|| {
        line_error(
            line,
            "`policy_for` requires `<kinds>: <atom>` (e.g. `policy_for jobs, webhooks: @actor.system`)",
        )
    })?;
    let kinds: Vec<String> = kinds_part
        .split(',')
        .map(|k| k.trim().to_owned())
        .filter(|k| !k.is_empty())
        .collect();
    if kinds.is_empty() {
        return Err(line_error(
            line,
            "`policy_for` requires at least one construct kind before the `:`",
        ));
    }
    let atom = atom_part.trim().to_owned();
    if atom.is_empty() {
        return Err(line_error(
            line,
            "`policy_for` requires a policy atom after the `:` (e.g. `@actor.system`)",
        ));
    }
    Ok(DefaultsPolicyFor {
        kinds,
        atom,
        span: Span::new(line.start, line.end),
    })
}

// =============================================================================
// Phase L Tier 4a — `defaults` block parser slice tests.
// =============================================================================
#[cfg(test)]
mod defaults_parser_tests {
    use super::super::parse_feature_skeletons;
    use crate::DefaultsTenancy;

    #[test]
    fn defaults_full_block_parses() {
        let source = r#"
feature customer
  defaults
    tenancy org
    timestamps
    policy_for jobs, webhooks: @actor.system
"#;
        let features = parse_feature_skeletons(source).unwrap();
        let defaults = features[0].defaults.as_ref().expect("defaults block");
        assert!(matches!(defaults.tenancy, Some(DefaultsTenancy::Org)));
        assert!(defaults.timestamps);
        assert_eq!(defaults.policy_for.len(), 1);
        assert_eq!(defaults.policy_for[0].kinds, vec!["jobs", "webhooks"]);
        assert_eq!(defaults.policy_for[0].atom, "@actor.system");
    }

    #[test]
    fn defaults_tenancy_only_parses() {
        let source = r#"
feature customer_auth
  defaults
    tenancy team
"#;
        let features = parse_feature_skeletons(source).unwrap();
        let defaults = features[0].defaults.as_ref().expect("defaults block");
        assert!(matches!(defaults.tenancy, Some(DefaultsTenancy::Team)));
        assert!(!defaults.timestamps);
        assert!(defaults.policy_for.is_empty());
    }

    #[test]
    fn defaults_custom_tenancy_parses() {
        let source = r#"
feature workspace_pinned
  defaults
    tenancy workspace
"#;
        let features = parse_feature_skeletons(source).unwrap();
        let defaults = features[0].defaults.as_ref().expect("defaults block");
        match defaults.tenancy.as_ref().expect("axis") {
            DefaultsTenancy::Custom(axis) => assert_eq!(axis, "workspace"),
            other => panic!("expected custom axis, got {other:?}"),
        }
    }

    #[test]
    fn defaults_duplicate_block_errors() {
        let source = r#"
feature customer
  defaults
    tenancy org

  defaults
    tenancy team
"#;
        let err = parse_feature_skeletons(source).unwrap_err();
        let message = format!("{err}");
        assert!(
            message.contains("at most one"),
            "error should mention duplicate defaults: {message}"
        );
    }

    #[test]
    fn defaults_unknown_child_errors() {
        let source = r#"
feature customer
  defaults
    timestaps
"#;
        let err = parse_feature_skeletons(source).unwrap_err();
        let message = format!("{err}");
        assert!(
            message.contains("tenancy"),
            "error should list valid children: {message}"
        );
    }

    #[test]
    fn defaults_policy_for_without_colon_errors() {
        let source = r#"
feature customer
  defaults
    policy_for jobs @actor.system
"#;
        let err = parse_feature_skeletons(source).unwrap_err();
        let message = format!("{err}");
        assert!(
            message.contains("<kinds>: <atom>"),
            "error should require explicit `:` (got {message:?})"
        );
    }
}
