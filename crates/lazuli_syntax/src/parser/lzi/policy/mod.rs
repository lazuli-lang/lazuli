//! Feature-level `policies` block parser.
//!
//! A `policies` block is the single declarative source of truth for
//! who can do what in a feature. The grammar is closed and indent-
//! sensitive (every block here lives under a `feature <name>` at
//! `AGENT_INDENT_FEATURE_CHILD`):
//!
//! ```text
//! policies
//!   create: @actor.staff, @actor.owner
//!     when_denied @translation.errors.forbidden
//!     when_denied_route
//!       unauthenticated -> view sign_in
//!       role_mismatch staff -> path "/dashboard"
//!       default -> view forbidden
//!   update: @policy.owner
//!   fields Customer
//!     email
//!       read: @actor.staff, @actor.owner
//!       write: @actor.owner
//! ```
//!
//! Two child shapes are accepted under `policies`:
//!
//! - **Policy categories** — `<name>: <@atom>, <@atom>` lines. Each
//!   may carry an optional `when_denied` translation reference and/or
//!   a `when_denied_route` redirect block (used by `command` audit
//!   diagnostics — `ERR-VOCAB-WHEN-DENIED-*`).
//! - **Field policy blocks** — `fields <Resource>` headers carrying
//!   per-field `read:` / `write:` clauses. These lower to typed
//!   `FieldPoliciesDecl` records so the analyzer can cross-check the
//!   resource shape.
//!
//! Closed-catalog rules (single-`when_denied`, single-`when_denied_route`,
//! at-most-one-default, unique `role_mismatch <role>`) are enforced
//! here so downstream consumers can trust the AST. Vocabulary closure
//! (which policy `@atom` namespaces exist) lives analyzer-side.
//!
//! Visibility: the entry `parse_policies_decl` is `pub(super)` so the
//! feature-skeleton walker in `mod.rs` can call it.

mod field_policies;
mod route_block;

use super::super::common::{SourceLine, is_trivia, line_error};
use super::super::error::ParseError;
use super::is_policy_identifier;
use super::parse_translation_key_token;

use crate::ast::{
    FieldPoliciesDecl, PoliciesDecl, PolicyCategoryDecl, PolicyConditionalAtomAst, Span,
    TranslationKeyRefAst, WhenDeniedRouteAst,
};

use field_policies::parse_field_policies_block;
use route_block::parse_when_denied_route_block;

/// GAP-09 — split a single policy-category entry into `(atom, predicate)`
/// when it carries a ` when ` tail, else `None`. The split point is the
/// first whitespace-delimited `when` token; the atom is everything before
/// it and the predicate everything after. A trailing/empty predicate or
/// empty atom yields `None` so a malformed `@x when` degrades to a plain
/// (unconditional) atom rather than a phantom predicate.
fn split_when_clause(entry: &str) -> Option<(&str, &str)> {
    // Match a standalone ` when ` token (space-delimited) to avoid
    // splitting inside an atom name or a quoted literal.
    let mut search = 0;
    while let Some(rel) = entry[search..].find(" when ") {
        let idx = search + rel;
        let atom = entry[..idx].trim();
        let when = entry[idx + " when ".len()..].trim();
        if !atom.is_empty() && !when.is_empty() {
            return Some((atom, when));
        }
        search = idx + " when ".len();
    }
    None
}

pub(super) fn parse_policies_decl(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(PoliciesDecl, usize), ParseError> {
    let header = &lines[start];
    let header_indent = header.indent;
    let child_indent = header_indent + 2;
    let grandchild_indent = header_indent + 4;
    let greatgrand_indent = header_indent + 6;

    let mut categories: Vec<PolicyCategoryDecl> = Vec::new();
    let mut field_blocks: Vec<FieldPoliciesDecl> = Vec::new();
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();
        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent <= header_indent {
            break;
        }
        if line.indent != child_indent {
            return Err(line_error(
                line,
                "`policies` body children use one indentation level deeper than the header",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("fields ") {
            let resource = rest.trim().to_owned();
            if resource.is_empty() {
                return Err(line_error(
                    line,
                    "`fields` requires a resource name (`fields <Resource>`)",
                ));
            }
            let (block, next) = parse_field_policies_block(
                lines,
                i,
                resource,
                grandchild_indent,
                greatgrand_indent,
            )?;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            field_blocks.push(block);
            i = next;
            continue;
        }

        if let Some((name, atoms_text)) = trimmed.split_once(':') {
            let name = name.trim();
            if !is_policy_identifier(name) {
                i += 1;
                continue;
            }
            // GAP-09 — each comma-separated entry is either a bare atom
            // (`@role.admin`) or a predicate-gated atom
            // (`@policy.admin when input.scope = "production"`). Split the
            // optional ` when ` tail off each entry; predicate-gated atoms
            // land in `conditional_atoms`, unconditional atoms in `atoms`,
            // so every existing reader of `atoms` stays behaviour-identical.
            let mut atoms: Vec<String> = Vec::new();
            let mut conditional_atoms: Vec<PolicyConditionalAtomAst> = Vec::new();
            for entry in atoms_text.split(',').map(str::trim) {
                if !entry.starts_with('@') {
                    continue;
                }
                if let Some((atom, when)) = split_when_clause(entry) {
                    conditional_atoms.push(PolicyConditionalAtomAst {
                        atom: atom.to_owned(),
                        when: when.to_owned(),
                        span: Span::new(line.start, line.end),
                    });
                } else {
                    atoms.push(entry.to_owned());
                }
            }
            let category_header_line = line;
            let category_header_end = line.end;
            let mut category_last_end = category_header_end;
            // IR Error-Vocab (Cell PARSE-1) — consume the optional
            // `when_denied @translation.<key>` child(ren) at
            // grandchild_indent (6 spaces under a feature). Zero-or-one
            // per category; duplicate is a parse error.
            let mut when_denied: Option<TranslationKeyRefAst> = None;
            let mut when_denied_route: Option<WhenDeniedRouteAst> = None;
            let mut j = i + 1;
            while j < lines.len() {
                let inner = &lines[j];
                let inner_trim = inner.text.trim_start();
                if is_trivia(inner_trim) {
                    j += 1;
                    continue;
                }
                if inner.indent <= child_indent {
                    break;
                }
                if inner.indent != grandchild_indent {
                    return Err(line_error(
                        inner,
                        "policy category children use one indentation level deeper than the category line",
                    ));
                }
                if let Some(rest) = inner_trim.strip_prefix("when_denied ") {
                    if when_denied.is_some() {
                        return Err(line_error(
                            inner,
                            "policy category may declare at most one `when_denied` child (ERR-VOCAB-MULTIPLE-WHEN-DENIED)",
                        ));
                    }
                    when_denied = Some(parse_translation_key_token(inner, rest)?);
                    category_last_end = inner.end;
                    j += 1;
                    continue;
                }
                if inner_trim == "when_denied_route" {
                    if when_denied_route.is_some() {
                        return Err(line_error(
                            inner,
                            "policy category may declare at most one `when_denied_route` child",
                        ));
                    }
                    let (parsed, next) = parse_when_denied_route_block(
                        lines,
                        j,
                        grandchild_indent,
                        greatgrand_indent,
                    )?;
                    category_last_end = lines[next.saturating_sub(1).max(j)].end;
                    when_denied_route = Some(parsed);
                    j = next;
                    continue;
                }
                return Err(line_error(
                    inner,
                    "policy category children are `when_denied @translation.<key>` or `when_denied_route` only",
                ));
            }
            categories.push(PolicyCategoryDecl {
                name: name.to_owned(),
                atoms,
                conditional_atoms,
                when_denied,
                when_denied_route,
                span: Span::new(category_header_line.start, category_last_end),
            });
            last_end = category_last_end;
            i = j;
            continue;
        }

        return Err(line_error(
            line,
            "`policies` children are `<name>: <atom>, ...` or `fields <Resource>` headers",
        ));
    }

    Ok((
        PoliciesDecl {
            categories,
            fields: field_blocks,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}


// =============================================================================
// IR Error-Vocab (Cell PARSE-1) — parser slice tests for `when_denied` on
// commands and `policies` categories, plus `when_denied_route`.
// =============================================================================
#[cfg(test)]
mod policy_when_denied_parser_tests {
    use super::super::parse_feature_skeletons;

    #[test]
    fn command_policy_when_denied_lifts_translation_key_ref() {
        let source = r#"
feature account
  command choose_role
    policy @policy.authenticated
      when_denied @translation.choose_role_signin_required
    input
      role_id: ID required
    returns User
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let command = features
            .iter()
            .find(|f| f.name == "account")
            .and_then(|f| f.commands.iter().find(|c| c.name == "choose_role"))
            .expect("choose_role command");
        let key = command
            .policy_when_denied
            .as_ref()
            .expect("policy_when_denied lifted");
        assert_eq!(key.key, "choose_role_signin_required");
    }

    #[test]
    fn command_policy_when_denied_rejects_duplicate() {
        let source = r#"
feature account
  command choose_role
    policy @policy.authenticated
      when_denied @translation.first
      when_denied @translation.second
    input
      role_id: ID required
    returns User
"#;
        let err = parse_feature_skeletons(source).expect_err("duplicate when_denied must reject");
        let msg = format!("{err}");
        assert!(
            msg.contains("at most one `when_denied`"),
            "expected duplicate-detection error, got {msg}"
        );
    }

    #[test]
    fn command_policy_rejects_non_translation_when_denied() {
        let source = r#"
feature account
  command choose_role
    policy @policy.authenticated
      when_denied "plain string"
    input
      role_id: ID required
    returns User
"#;
        let err = parse_feature_skeletons(source).expect_err("non-@translation.<key> must reject");
        let msg = format!("{err}");
        assert!(
            msg.contains("@translation.") || msg.contains("expected `@translation.<key>`"),
            "expected `@translation.<key>` form error, got {msg}"
        );
    }

    #[test]
    fn policy_category_when_denied_lifts_translation_key_ref() {
        let source = r#"
feature account
  policies
    authenticated: @scope.authenticated
      when_denied @translation.must_be_signed_in
    admin_only: @role.admin
      when_denied @translation.admin_only_action
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let policies = features
            .iter()
            .find(|f| f.name == "account")
            .and_then(|f| f.policies.as_ref())
            .expect("policies block lifted");
        assert_eq!(policies.categories.len(), 2);
        let authenticated = policies
            .categories
            .iter()
            .find(|c| c.name == "authenticated")
            .expect("authenticated category");
        assert_eq!(
            authenticated.when_denied.as_ref().map(|k| k.key.as_str()),
            Some("must_be_signed_in")
        );
        let admin = policies
            .categories
            .iter()
            .find(|c| c.name == "admin_only")
            .expect("admin_only category");
        assert_eq!(
            admin.when_denied.as_ref().map(|k| k.key.as_str()),
            Some("admin_only_action")
        );
    }

    #[test]
    fn policy_category_when_denied_route_lifts_route_targets() {
        let source = r#"
feature account
  policies
    host_only: @scope.authenticated, @role.host
      when_denied @translation.host_only_required
      when_denied_route
        unauthenticated -> view sign_in
        role_mismatch traveler -> view explore
        role_mismatch operator -> view dashboard
        default -> path "/welcome"
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let policies = features[0].policies.as_ref().expect("policies block");
        let category = policies
            .categories
            .iter()
            .find(|category| category.name == "host_only")
            .expect("host_only category");
        let route = category
            .when_denied_route
            .as_ref()
            .expect("when_denied_route lifted");

        assert_eq!(
            route.unauthenticated,
            Some(crate::ast::RouteRedirectTargetAst::View(
                "sign_in".to_string()
            ))
        );
        assert_eq!(route.role_mismatch.len(), 2);
        assert_eq!(route.role_mismatch[0].role, "traveler");
        assert_eq!(
            route.role_mismatch[0].target,
            crate::ast::RouteRedirectTargetAst::View("explore".to_string())
        );
        assert_eq!(
            route.default,
            Some(crate::ast::RouteRedirectTargetAst::Path(
                "/welcome".to_string()
            ))
        );
    }

    #[test]
    fn policy_category_input_predicate_lifts_conditional_atoms() {
        // GAP-09 — canonical syntax: predicate-gated atoms split off into
        // `conditional_atoms`, leaving `atoms` empty for this category.
        let source = r#"
feature account
  policies
    create: @policy.admin when input.scope = "production", @policy.manager when input.scope = "media"
"#;
        let features = super::super::parse_feature_skeletons(source).expect("parses");
        let policies = features[0].policies.as_ref().expect("policies block");
        let create = policies
            .categories
            .iter()
            .find(|c| c.name == "create")
            .expect("create category");
        assert!(create.atoms.is_empty(), "predicate atoms must not land in `atoms`");
        assert_eq!(create.conditional_atoms.len(), 2);
        assert_eq!(create.conditional_atoms[0].atom, "@policy.admin");
        assert_eq!(create.conditional_atoms[0].when, "input.scope = \"production\"");
        assert_eq!(create.conditional_atoms[1].atom, "@policy.manager");
        assert_eq!(create.conditional_atoms[1].when, "input.scope = \"media\"");
    }

    #[test]
    fn policy_category_mixes_conditional_and_unconditional_atoms() {
        let source = r#"
feature account
  policies
    create: @role.owner, @policy.admin when input.scope = "production"
"#;
        let features = super::super::parse_feature_skeletons(source).expect("parses");
        let create = features[0]
            .policies
            .as_ref()
            .unwrap()
            .categories
            .iter()
            .find(|c| c.name == "create")
            .unwrap();
        assert_eq!(create.atoms, vec!["@role.owner".to_owned()]);
        assert_eq!(create.conditional_atoms.len(), 1);
        assert_eq!(create.conditional_atoms[0].atom, "@policy.admin");
    }

    #[test]
    fn policy_category_when_denied_route_rejects_duplicate_default() {
        let source = r#"
feature account
  policies
    host_only: @scope.authenticated
      when_denied_route
        default -> view welcome
        default -> path "/"
"#;
        let err = parse_feature_skeletons(source).expect_err("duplicate default must reject");
        let msg = format!("{err}");
        assert!(
            msg.contains("declares `default` at most once"),
            "expected duplicate default error, got {msg}"
        );
    }
}
