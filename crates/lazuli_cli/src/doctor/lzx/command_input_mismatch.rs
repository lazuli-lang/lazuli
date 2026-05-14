//! `lzx-command-input-mismatch` — `fields` (in `view create`) references a
//! field not in the command's input.
//!
//! Fires on `view create` views whose `fields` list contains a name that
//! isn't a slot in the `submit` command's `input` block. Suppresses when the
//! referenced command can't be found on the feature (upstream resolution
//! error — out of this rule's scope).
//!
//! Severity: `error` per `docs/proposals/lzx-integration-codegen.md` §11.

use super::ir_stub::{Command, CommandRef, Feature, Module, View};
use super::sort_findings;

/// One `lzx-command-input-mismatch` finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub feature: String,
    pub view: String,
    pub line: usize,
    pub message: String,
}

impl Finding {
    pub const CODE: &'static str = "lzx-command-input-mismatch";

    pub fn message(feature: &str, view: &str, field: &str, command: &str) -> String {
        format!(
            "view `{view}` in feature `{feature}`: field `{field}` is not in the \
             input block of command `{command}`. Add the slot to the command's \
             input or fix the typo in `fields`. See \
             docs/proposals/lzx-integration-codegen.md §11."
        )
    }
}

/// Run `lzx-command-input-mismatch` across all `.lzx` surfaces.
pub fn check(module: &Module) -> Vec<Finding> {
    let mut out = Vec::new();
    for feature in &module.features {
        for surface in &feature.surfaces {
            for audience in &surface.audiences {
                for view in &audience.views {
                    if let View::Create(create) = view {
                        check_create(
                            feature,
                            &create.name,
                            create.line,
                            &create.submit,
                            &create.fields,
                            &mut out,
                        );
                    }
                }
            }
        }
    }
    sort_findings(&mut out, |f| {
        (f.feature.clone(), f.view.clone(), Finding::CODE, f.line)
    });
    out
}

fn check_create(
    feature: &Feature,
    view_name: &str,
    line: usize,
    submit: &CommandRef,
    fields: &[String],
    out: &mut Vec<Finding>,
) {
    let command = match find_command(feature, submit) {
        Some(c) => c,
        None => return,
    };
    for field in fields {
        if !command.input_fields.iter().any(|f| f == field) {
            out.push(Finding {
                feature: feature.name.clone(),
                view: view_name.to_owned(),
                line,
                message: Finding::message(&feature.name, view_name, field, &command.name),
            });
        }
    }
}

fn find_command<'a>(feature: &'a Feature, cref: &CommandRef) -> Option<&'a Command> {
    // Cross-feature command references are out of scope for this rule —
    // resolving them needs the full module index; the analyzer pass handles
    // those upstream.
    if cref.feature.as_deref().is_some_and(|f| f != feature.name) {
        return None;
    }
    feature.commands.iter().find(|c| c.name == cref.name)
}

#[cfg(test)]
mod tests {
    use super::super::ir_stub::*;
    use super::*;

    fn mk_module(view: View, command: Command) -> Module {
        Module {
            features: vec![Feature {
                name: "slug".into(),
                resources: vec![],
                queries: vec![],
                commands: vec![command],
                surfaces: vec![Surface {
                    feature: "slug".into(),
                    target: "web".into(),
                    audiences: vec![Audience {
                        name: "admin".into(),
                        requires: vec![],
                        views: vec![view],
                        line: 3,
                    }],
                }],
            }],
        }
    }

    fn mk_create(name: &str, line: usize, submit: &str, fields: &[&str]) -> View {
        View::Create(ViewCreate {
            name: name.to_owned(),
            at: Some("/new".into()),
            submit: CommandRef {
                feature: None,
                name: submit.to_owned(),
                line,
            },
            fields: fields.iter().map(|s| (*s).to_owned()).collect(),
            cells: vec![],
            line,
        })
    }

    fn mk_command(name: &str, input_fields: &[&str]) -> Command {
        Command {
            name: name.to_owned(),
            input_fields: input_fields.iter().map(|s| (*s).to_owned()).collect(),
            policy_atoms: vec![],
        }
    }

    /// Negative — every field exists in the command input.
    #[test]
    fn negative_all_fields_in_input_no_finding() {
        let v = mk_create("slug_create", 20, "create", &["key", "title"]);
        let c = mk_command("create", &["key", "title", "description"]);
        let module = mk_module(v, c);
        assert!(check(&module).is_empty());
    }

    /// Positive — one rogue field fires exactly one finding.
    #[test]
    fn positive_single_bad_field_fires_once() {
        let v = mk_create("slug_create", 22, "create", &["key", "ghost"]);
        let c = mk_command("create", &["key", "title"]);
        let module = mk_module(v, c);
        let findings = check(&module);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].view, "slug_create");
        assert_eq!(findings[0].line, 22);
        assert!(findings[0].message.contains("ghost"));
        assert!(findings[0].message.contains("create"));
    }

    /// Edge — multiple rogue fields on the same view fire multiple findings,
    /// in stable order.
    #[test]
    fn edge_multiple_bad_fields_fire_multiple_findings() {
        let v = mk_create("slug_create", 25, "create", &["alpha", "bravo", "charlie"]);
        let c = mk_command("create", &[]);
        let module = mk_module(v, c);
        let findings = check(&module);
        assert_eq!(findings.len(), 3);
        let bodies: Vec<String> = findings.iter().map(|f| f.message.clone()).collect();
        assert!(bodies.iter().any(|m| m.contains("alpha")));
        assert!(bodies.iter().any(|m| m.contains("bravo")));
        assert!(bodies.iter().any(|m| m.contains("charlie")));
    }

    /// Edge — list/detail views are out of scope; only create views fire.
    #[test]
    fn list_view_does_not_trigger_this_rule() {
        let v = View::List(ViewList {
            name: "slug_list".into(),
            at: Some("/slugs".into()),
            source: QueryRef {
                feature: None,
                name: "mine".into(),
            },
            render: ListRender::Table {
                columns: vec!["whatever".into()],
            },
            columns: vec!["whatever".into()],
            search: vec![],
            filter: vec![],
            search_decl: None,
            filter_decls: vec![],
            selection: None,
            cells: vec![],
            actions: vec![],
            drawer: None,
            line: 30,
        });
        let c = mk_command("create", &["key"]);
        let module = mk_module(v, c);
        assert!(check(&module).is_empty());
    }
}
