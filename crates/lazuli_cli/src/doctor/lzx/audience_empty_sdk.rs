//! `lzx-audience-empty-sdk` — an audience's `requires` set produces an empty
//! command/query intersection.
//!
//! Fires when an `audience` block in a `.lzx` surface has at least one
//! `requires @scope.X` clause but those scopes intersect zero scoped commands
//! and zero scoped queries on the feature. Such an audience would emit an
//! empty SDK projection per `docs/proposals/lzx-integration-codegen.md` §7 —
//! probably an authoring mistake (typo in scope, or audience added before
//! the corresponding commands).
//!
//! Public commands (`policy_atoms.is_empty()`) intentionally do NOT save an
//! otherwise-empty audience: an audience whose `requires` ONLY matches
//! public commands is functionally equivalent to "audience public_only" and
//! deserves the warning so the author can either drop the audience or scope
//! a real command to it.
//!
//! When the audience has no `requires` clauses, the rule suppresses: that's
//! the "open audience" case and is governed by a different proposal §7
//! (every command/query goes through).
//!
//! Severity: `warning` per `docs/proposals/lzx-integration-codegen.md` §11.

use super::ir_stub::{Audience, Command, Feature, Module};
use super::sort_findings;

/// One `lzx-audience-empty-sdk` finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub feature: String,
    /// Audience name (not view — this rule fires once per dead audience,
    /// not per view).  Kept under the `view` field to preserve the shared
    /// `Finding` shape across the lzx-* namespace.
    pub view: String,
    pub line: usize,
    pub message: String,
}

impl Finding {
    /// Stable doctor rule code surfaced to the user.
    pub const CODE: &'static str = "lzx-audience-empty-sdk";

    /// Render the canonical diagnostic message.
    pub fn message(feature: &str, audience: &str, requires: &[String]) -> String {
        let req_list = if requires.is_empty() {
            "<none>".to_string()
        } else {
            requires.join(", ")
        };
        format!(
            "audience `{audience}` in feature `{feature}`: `requires {req_list}` \
             intersects zero scoped commands or queries on this feature, so its \
             generated SDK projection is empty. Either drop the audience, \
             relax its `requires`, or add commands/queries with a matching \
             policy. See docs/proposals/lzx-integration-codegen.md §7 + §11."
        )
    }
}

/// Run `lzx-audience-empty-sdk` across all `.lzx` surfaces.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_cli::doctor::lzx::audience_empty_sdk::check;
/// use lazuli_cli::doctor::lzx::ir_stub::Module;
///
/// // let findings = check(&module);
/// ```
pub fn check(module: &Module) -> Vec<Finding> {
    let mut out = Vec::new();
    for feature in &module.features {
        for surface in &feature.surfaces {
            for audience in &surface.audiences {
                check_audience(feature, audience, &mut out);
            }
        }
    }
    sort_findings(&mut out, |f| {
        (f.feature.clone(), f.view.clone(), Finding::CODE, f.line)
    });
    out
}

fn check_audience(feature: &Feature, audience: &Audience, out: &mut Vec<Finding>) {
    // Audiences with no `requires` clauses are open — out of this rule's scope.
    if audience.requires.is_empty() {
        return;
    }

    let scoped_match = feature
        .commands
        .iter()
        .any(|cmd| command_intersects(cmd, &audience.requires));

    if !scoped_match {
        out.push(Finding {
            feature: feature.name.clone(),
            view: audience.name.clone(),
            line: audience.line,
            message: Finding::message(&feature.name, &audience.name, &audience.requires),
        });
    }
}

/// A command's policy intersects the audience's `requires` set iff at least
/// one of the command's atoms equals one of the required scopes. Public
/// commands (empty `policy_atoms`) do not save an audience — see module-level
/// docs for rationale.
fn command_intersects(cmd: &Command, requires: &[String]) -> bool {
    if cmd.policy_atoms.is_empty() {
        return false;
    }
    cmd.policy_atoms
        .iter()
        .any(|atom| requires.iter().any(|req| req == atom))
}

#[cfg(test)]
mod tests {
    use super::super::ir_stub::*;
    use super::*;

    fn mk_module(audiences: Vec<Audience>, commands: Vec<Command>) -> Module {
        Module {
            features: vec![Feature {
                name: "slug".into(),
                resources: vec![],
                queries: vec![],
                commands,
                surfaces: vec![Surface {
                    feature: "slug".into(),
                    target: "web".into(),
                    audiences,
                }],
            }],
        }
    }

    fn aud(name: &str, requires: Vec<&str>, line: usize) -> Audience {
        Audience {
            name: name.to_owned(),
            requires: requires.iter().map(|s| (*s).to_owned()).collect(),
            views: vec![],
            line,
        }
    }

    fn cmd(name: &str, atoms: Vec<&str>) -> Command {
        Command {
            name: name.to_owned(),
            input_fields: vec![],
            input: CommandInput::Empty,
            policy_atoms: atoms.iter().map(|s| (*s).to_owned()).collect(),
        }
    }

    /// Negative — audience's `requires` intersects at least one scoped command.
    #[test]
    fn negative_audience_has_scoped_command_no_finding() {
        let module = mk_module(
            vec![aud("admin", vec!["@scope.admin"], 5)],
            vec![cmd("delete", vec!["@scope.admin"])],
        );
        assert!(check(&module).is_empty());
    }

    /// Positive — audience requires a scope no command has.
    #[test]
    fn positive_audience_with_no_matching_command_fires() {
        let module = mk_module(
            vec![aud("ghost", vec!["@scope.nonexistent"], 7)],
            vec![cmd("delete", vec!["@scope.admin"])],
        );
        let findings = check(&module);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].view, "ghost");
        assert_eq!(findings[0].line, 7);
        assert!(findings[0].message.contains("@scope.nonexistent"));
    }

    /// Edge — public commands do not save the audience: an audience with
    /// `requires X` against a feature of only-public commands still fires.
    #[test]
    fn edge_public_commands_do_not_save_audience() {
        let module = mk_module(
            vec![aud("only_admin", vec!["@scope.admin"], 9)],
            vec![cmd("public_ping", vec![]), cmd("public_pong", vec![])],
        );
        let findings = check(&module);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].view, "only_admin");
    }

    /// Edge — audience with empty `requires` (open audience) is suppressed.
    #[test]
    fn edge_open_audience_no_requires_is_suppressed() {
        let module = mk_module(
            vec![aud("open", vec![], 11)],
            vec![cmd("any_scoped", vec!["@scope.admin"])],
        );
        assert!(check(&module).is_empty());
    }

    /// Edge — multiple audiences in one surface: only the dead ones fire.
    #[test]
    fn edge_multiple_audiences_only_dead_fires() {
        let module = mk_module(
            vec![
                aud("alive", vec!["@scope.admin"], 5),
                aud("dead", vec!["@scope.nonexistent"], 9),
                aud("also_dead", vec!["@scope.ghost"], 13),
            ],
            vec![cmd("delete", vec!["@scope.admin"])],
        );
        let findings = check(&module);
        assert_eq!(findings.len(), 2);
        let names: Vec<&str> = findings.iter().map(|f| f.view.as_str()).collect();
        assert!(names.contains(&"dead"));
        assert!(names.contains(&"also_dead"));
        assert!(!names.contains(&"alive"));
    }
}
