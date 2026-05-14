//! `lzx-bulk-action-input-shape` + `lzx-bulk-actions-require-multi`.
//!
//! Bulk actions run against `view.selection` IDs, so they are only valid for
//! `selection multi` and only when the command input has one required `ID[]`
//! slot named `ids` or `<feature>_ids`.

use super::ir_stub::{
    Command, CommandInput, CommandRef, Feature, Module, SelectionMode, TypeRef, View,
};
use super::sort_findings;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub feature: String,
    pub view: String,
    pub line: usize,
    pub code: &'static str,
    pub message: String,
}

impl Finding {
    pub const INPUT_SHAPE_CODE: &'static str = "lzx-bulk-action-input-shape";
    pub const REQUIRE_MULTI_CODE: &'static str = "lzx-bulk-actions-require-multi";

    fn require_multi_message(view: &str, mode: SelectionMode) -> String {
        format!(
            "view '{view}' declares bulk_actions but selection mode is {} — bulk_actions requires 'selection multi'",
            mode_label(mode)
        )
    }

    fn input_shape_message(command: &str, actual_shape: &str) -> String {
        format!(
            "bulk action '{command}' input shape is not '{{ ids: ID[] }}' or '{{ <feature>_ids: ID[] }}' — got {actual_shape}; add an 'ids: list of ID required' input field or use a non-bulk action"
        )
    }
}

pub fn check(module: &Module) -> Vec<Finding> {
    let mut out = Vec::new();
    for feature in &module.features {
        for surface in &feature.surfaces {
            for audience in &surface.audiences {
                for view in &audience.views {
                    let View::List(list) = view else {
                        continue;
                    };
                    let Some(selection) = &list.selection else {
                        continue;
                    };
                    if selection.bulk_actions.is_empty() {
                        continue;
                    }
                    if selection.mode != SelectionMode::Multi {
                        out.push(Finding {
                            feature: feature.name.clone(),
                            view: list.name.clone(),
                            line: list.line,
                            code: Finding::REQUIRE_MULTI_CODE,
                            message: Finding::require_multi_message(&list.name, selection.mode),
                        });
                    }
                    for action in &selection.bulk_actions {
                        check_action_shape(feature, &list.name, action, &mut out);
                    }
                }
            }
        }
    }
    sort_findings(&mut out, |f| {
        (f.feature.clone(), f.view.clone(), f.code, f.line)
    });
    out
}

fn check_action_shape(
    feature: &Feature,
    view_name: &str,
    action: &CommandRef,
    out: &mut Vec<Finding>,
) {
    let command = match find_command(feature, action) {
        Some(command) => command,
        None => return,
    };

    if accepts_bulk_input_shape(feature, command) {
        return;
    }

    out.push(Finding {
        feature: feature.name.clone(),
        view: view_name.to_owned(),
        line: action.line,
        code: Finding::INPUT_SHAPE_CODE,
        message: Finding::input_shape_message(&action.name, &actual_shape(command)),
    });
}

fn find_command<'a>(feature: &'a Feature, cref: &CommandRef) -> Option<&'a Command> {
    if cref
        .feature
        .as_deref()
        .is_some_and(|name| name != feature.name)
    {
        return None;
    }
    feature
        .commands
        .iter()
        .find(|command| command.name == cref.name)
}

fn accepts_bulk_input_shape(feature: &Feature, command: &Command) -> bool {
    let CommandInput::Typed(fields) = &command.input else {
        return false;
    };
    let [field] = fields.as_slice() else {
        return false;
    };
    if !field.required {
        return false;
    }
    let prefixed = format!("{}_ids", feature.name);
    (field.name == "ids" || field.name == prefixed) && is_id_list(&field.type_ref, &feature.name)
}

fn is_id_list(type_ref: &TypeRef, feature_name: &str) -> bool {
    match type_ref {
        TypeRef::List(inner) => match inner.as_ref() {
            TypeRef::Id => true,
            TypeRef::QualifiedId(prefix) => prefix.eq_ignore_ascii_case(feature_name),
            _ => false,
        },
        _ => false,
    }
}

fn actual_shape(command: &Command) -> String {
    match &command.input {
        CommandInput::Empty => "{}".to_owned(),
        CommandInput::Short(fields) => {
            if fields.is_empty() {
                "{}".to_owned()
            } else {
                format!("{{ {} }}", fields.join(", "))
            }
        }
        CommandInput::Typed(fields) => {
            let parts: Vec<String> = fields
                .iter()
                .map(|field| format!("{}: {}", field.name, display_type(&field.type_ref)))
                .collect();
            format!("{{ {} }}", parts.join(", "))
        }
    }
}

fn display_type(type_ref: &TypeRef) -> String {
    match type_ref {
        TypeRef::Id => "ID".to_owned(),
        TypeRef::QualifiedId(prefix) => format!("{prefix}.ID"),
        TypeRef::List(inner) => format!("{}[]", display_type(inner)),
        TypeRef::Other(name) => name.clone(),
    }
}

fn mode_label(mode: SelectionMode) -> &'static str {
    match mode {
        SelectionMode::None => "none",
        SelectionMode::Single => "single",
        SelectionMode::Multi => "multi",
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

    fn id_list() -> TypeRef {
        TypeRef::List(Box::new(TypeRef::Id))
    }

    fn qualified_id_list(prefix: &str) -> TypeRef {
        TypeRef::List(Box::new(TypeRef::QualifiedId(prefix.to_owned())))
    }

    fn typed_cmd(name: &str, fields: Vec<(&str, TypeRef)>) -> Command {
        Command {
            name: name.to_owned(),
            input_fields: fields.iter().map(|(n, _)| (*n).to_owned()).collect(),
            input: CommandInput::Typed(
                fields
                    .into_iter()
                    .map(|(name, type_ref)| TypedSlot {
                        name: name.to_owned(),
                        type_label: display_type_label(&type_ref),
                        type_ref,
                        required: true,
                    })
                    .collect(),
            ),
            policy_atoms: vec![],
        }
    }

    fn display_type_label(type_ref: &TypeRef) -> String {
        match type_ref {
            TypeRef::Id => "ID".to_owned(),
            TypeRef::QualifiedId(prefix) => format!("{prefix}.ID"),
            TypeRef::List(inner) => format!("{}[]", display_type_label(inner)),
            TypeRef::Other(name) => name.clone(),
        }
    }

    fn list_view(selection: Option<SelectionDecl>) -> View {
        View::List(ViewList {
            name: "item_list".into(),
            at: Some("/items".into()),
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
            selection,
            sort: None,
            cells: vec![],
            actions: vec![],
            drawer: None,
            line: 20,
        })
    }

    fn selection(mode: SelectionMode, actions: Vec<CommandRef>) -> SelectionDecl {
        SelectionDecl {
            mode,
            bulk_actions: actions,
        }
    }

    fn module(view: View, commands: Vec<Command>) -> Module {
        Module {
            features: vec![Feature {
                name: "item".into(),
                resources: vec![],
                queries: vec![],
                commands,
                surfaces: vec![Surface {
                    feature: "item".into(),
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

    #[test]
    fn happy_path_ids() {
        let module = module(
            list_view(Some(selection(
                SelectionMode::Multi,
                vec![cref("delete", 21)],
            ))),
            vec![typed_cmd("delete", vec![("ids", id_list())])],
        );

        assert!(check(&module).is_empty());
    }

    #[test]
    fn happy_path_feature_ids() {
        let module = module(
            list_view(Some(selection(
                SelectionMode::Multi,
                vec![cref("delete", 21)],
            ))),
            vec![typed_cmd(
                "delete",
                vec![("item_ids", qualified_id_list("item"))],
            )],
        );

        assert!(check(&module).is_empty());
    }

    #[test]
    fn wrong_shape() {
        let module = module(
            list_view(Some(selection(
                SelectionMode::Multi,
                vec![cref("delete", 21)],
            ))),
            vec![typed_cmd("delete", vec![("item_id", TypeRef::Id)])],
        );

        let findings = check(&module);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, Finding::INPUT_SHAPE_CODE);
        assert!(findings[0].message.contains("bulk action 'delete'"));
        assert!(findings[0].message.contains("got { item_id: ID }"));
    }

    #[test]
    fn bulk_without_multi() {
        let module = module(
            list_view(Some(selection(
                SelectionMode::Single,
                vec![cref("delete", 21)],
            ))),
            vec![typed_cmd("delete", vec![("ids", id_list())])],
        );

        let findings = check(&module);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, Finding::REQUIRE_MULTI_CODE);
        assert_eq!(
            findings[0].message,
            "view 'item_list' declares bulk_actions but selection mode is single — bulk_actions requires 'selection multi'"
        );
    }

    #[test]
    fn missing_selection() {
        let module = module(
            list_view(None),
            vec![typed_cmd("delete", vec![("ids", id_list())])],
        );

        assert!(check(&module).is_empty());
    }
}
