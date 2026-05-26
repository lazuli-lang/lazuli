//! `lzx-action-not-in-audience` — `actions` references a command whose
//! effective policy isn't reachable from the audience's `requires`.
//!
//! Fires on `view list` and `view detail` views: each entry in `actions` must
//! resolve to a command whose effective policy atoms intersect the enclosing
//! audience's `requires` set (or the command must be public — empty
//! `policy_atoms`, per `audience_reachable_commands` semantics).
//!
//! Suppresses when the action references a command that can't be located on
//! the feature (upstream resolution error).
//!
//! Severity: `error` per `docs/proposals/lzx-integration-codegen.md` §11.

use super::ir_stub::{Audience, Command, CommandRef, Feature, Module, View};
use super::{audience_reachable_commands, sort_findings};

/// One `lzx-action-not-in-audience` finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub feature: String,
    pub view: String,
    pub line: usize,
    pub message: String,
}

impl Finding {
    /// Stable doctor rule code surfaced to the user.
    pub const CODE: &'static str = "lzx-action-not-in-audience";

    /// Render the canonical diagnostic message.
    pub fn message(feature: &str, view: &str, audience: &str, command: &str) -> String {
        format!(
            "view `{view}` in feature `{feature}`: action `{command}` is not \
             reachable from audience `{audience}` — the command's effective \
             policy atoms do not intersect the audience's `requires` set. \
             Either drop the action, relax the command's policy, or move the \
             view to a different audience. See \
             docs/proposals/lzx-integration-codegen.md §7.2 + §11."
        )
    }
}

/// Run `lzx-action-not-in-audience` across all `.lzx` surfaces.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_cli::doctor::lzx::action_not_in_audience::check;
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
    let reachable: Vec<&Command> = audience_reachable_commands(audience, feature);
    let reachable_names: Vec<&str> = reachable.iter().map(|c| c.name.as_str()).collect();

    for view in &audience.views {
        let (view_name, actions) = match view {
            View::List(v) => (v.name.as_str(), v.actions.as_slice()),
            View::Detail(v) => (v.name.as_str(), v.actions.as_slice()),
            // `view create.submit` is checked by route binding / input
            // mismatch rules; the audience scope is enforced by the audience
            // being the enclosing block.
            View::Create(_) => continue,
        };
        for action in actions {
            check_action(feature, audience, view_name, action, &reachable_names, out);
        }
    }
}

fn check_action(
    feature: &Feature,
    audience: &Audience,
    view_name: &str,
    action: &CommandRef,
    reachable_names: &[&str],
    out: &mut Vec<Finding>,
) {
    // Skip cross-feature actions — full resolution is upstream.
    if action.feature.as_deref().is_some_and(|f| f != feature.name) {
        return;
    }
    // Skip unknown commands — also upstream.
    let command_exists = feature.commands.iter().any(|c| c.name == action.name);
    if !command_exists {
        return;
    }
    if !reachable_names.contains(&action.name.as_str()) {
        out.push(Finding {
            feature: feature.name.clone(),
            view: view_name.to_owned(),
            line: action.line,
            message: Finding::message(&feature.name, view_name, &audience.name, &action.name),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::super::ir_stub::*;
    use super::*;

    fn cref(name: &str, line: usize) -> CommandRef {
        CommandRef {
            feature: None,
            name: name.to_owned(),
            line,
        }
    }

    fn list_with_actions(name: &str, line: usize, actions: Vec<CommandRef>) -> View {
        View::List(ViewList {
            name: name.to_owned(),
            at: Some("/list".into()),
            source: QueryRef {
                feature: None,
                name: "mine".into(),
            },
            render: ListRender::Table { columns: vec![] },
            columns: vec![],
            search: vec![],
            filter: vec![],
            search_decl: None,
            filter_decls: vec![],
            selection: None,
            sort: None,
            cells: vec![],
            actions,
            drawer: None,
            line,
        })
    }

    fn mk_module(
        audience_name: &str,
        requires: Vec<&str>,
        views: Vec<View>,
        commands: Vec<Command>,
    ) -> Module {
        Module {
            features: vec![Feature {
                name: "slug".into(),
                resources: vec![],
                queries: vec![],
                commands,
                surfaces: vec![Surface {
                    feature: "slug".into(),
                    target: "web".into(),
                    audiences: vec![Audience {
                        name: audience_name.to_owned(),
                        requires: requires.iter().map(|s| (*s).to_owned()).collect(),
                        views,
                        line: 3,
                    }],
                }],
            }],
        }
    }

    fn mk_command(name: &str, policy_atoms: Vec<&str>) -> Command {
        Command {
            name: name.to_owned(),
            input_fields: vec![],
            input: CommandInput::Empty,
            policy_atoms: policy_atoms.iter().map(|s| (*s).to_owned()).collect(),
        }
    }

    /// Negative — every action's policy intersects the audience requires.
    #[test]
    fn negative_actions_all_reachable_no_finding() {
        let module = mk_module(
            "admin",
            vec!["@scope.workspace_admin"],
            vec![list_with_actions(
                "slug_list",
                15,
                vec![cref("create", 16), cref("delete", 17)],
            )],
            vec![
                mk_command("create", vec!["@scope.workspace_admin"]),
                mk_command("delete", vec!["@scope.workspace_admin"]),
            ],
        );
        assert!(check(&module).is_empty());
    }

    /// Positive — one action's policy doesn't match audience.requires.
    #[test]
    fn positive_unreachable_action_fires_once() {
        let module = mk_module(
            "public",
            vec!["@scope.workspace_member"],
            vec![list_with_actions(
                "slug_list",
                15,
                vec![cref("archive", 16)],
            )],
            vec![mk_command("archive", vec!["@scope.workspace_admin"])],
        );
        let findings = check(&module);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].view, "slug_list");
        assert_eq!(findings[0].line, 16);
        assert!(findings[0].message.contains("archive"));
        assert!(findings[0].message.contains("public"));
    }

    /// Edge — public commands (empty `policy_atoms`) are reachable from every
    /// audience; reachable + unreachable actions on same view produce exactly
    /// one finding for the bad one.
    #[test]
    fn edge_mixed_actions_public_and_scoped() {
        let module = mk_module(
            "viewer",
            vec!["@scope.read_only"],
            vec![list_with_actions(
                "slug_list",
                20,
                vec![
                    cref("public_ping", 21), // public — reachable
                    cref("delete", 22),      // scoped admin — NOT reachable
                ],
            )],
            vec![
                mk_command("public_ping", vec![]), // public
                mk_command("delete", vec!["@scope.workspace_admin"]),
            ],
        );
        let findings = check(&module);
        assert_eq!(findings.len(), 1);
        assert!(findings[0].message.contains("delete"));
        assert_eq!(findings[0].line, 22);
    }

    /// Edge — unknown command is suppressed (upstream resolution error).
    #[test]
    fn edge_unknown_command_is_suppressed() {
        let module = mk_module(
            "admin",
            vec!["@scope.workspace_admin"],
            vec![list_with_actions("slug_list", 30, vec![cref("ghost", 31)])],
            vec![],
        );
        assert!(
            check(&module).is_empty(),
            "unknown command must be left to upstream resolver"
        );
    }
}
