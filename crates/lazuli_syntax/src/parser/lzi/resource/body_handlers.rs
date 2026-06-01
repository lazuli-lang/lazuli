//! `resource` body handler cluster — the strip-prefix table used by
//! `parse_resource_decl` for keyword-style children (`previously`,
//! `tenancy`, `retention`, `validates`, `has_many`, `index`, `unique`,
//! `fts`). Lives separately from `mod.rs` so the parent stays Rails-thin.
//!
//! `lifecycle ` is dispatched inline (it owns a block walker) but keeps a
//! sentinel registry entry so the handler list documents the closed set.

use super::super::super::common::{SourceLine, line_error, line_error_owned};
use super::super::super::error::ParseError;
use super::super::parse_defaults_tenancy;
use super::super::types::LifecycleBlockAst;
use super::has_many::parse_resource_has_many;
use super::index::{parse_parenthesized_field_list_with_trailing, parse_resource_index_target};
use super::retention::parse_resource_retention;
use crate::ast::{
    CrudOverlayAst, DefaultsTenancy, InvariantDecl, ManyThroughAst, ResourceCompositeKey,
    ResourceConstraintAst, ResourceConventionAst, ResourceFieldDecl, ResourceHasMany,
    ResourceIndexAst, ResourceIndexMethodAst, ResourceLifecycleRoutesAst, ResourceLock,
    ResourcePolymorphicRefAst, ResourceRestrictOnDelete, ResourceRetention, ResourceUniqueAst,
    Span,
};

#[derive(Default)]
pub(super) struct ResourceBodyState {
    pub(super) previously: Vec<String>,
    pub(super) tenancy: Option<DefaultsTenancy>,
    pub(super) fields: Vec<ResourceFieldDecl>,
    pub(super) has_many: Vec<ResourceHasMany>,
    pub(super) soft_delete: bool,
    /// Spec 0015 — `soft_delete by` actor projection flag. Implies
    /// `soft_delete`. Projects a `deleted_by: ID` column.
    pub(super) soft_delete_actor: bool,
    pub(super) timestamps: bool,
    /// GAP-AUDIT-02 — `append_only` resource modifier (bare line).
    pub(super) append_only: bool,
    pub(super) retention: Option<ResourceRetention>,
    pub(super) validates: Vec<String>,
    pub(super) lifecycle: Option<LifecycleBlockAst>,
    /// CL.C.4 — resource-scoped `invariant <name>` blocks.
    pub(super) invariants: Vec<InvariantDecl>,
    /// Roadmap §1.5 (CL.C.2) — `lock` decorator.
    pub(super) lock: Option<ResourceLock>,
    /// Roadmap §1.5 (CL.C.2) — `composite_key` block.
    pub(super) composite_key: Option<ResourceCompositeKey>,
    /// `conventions [<name>, ...]` resource-level slot — closed catalog.
    /// See `docs/proposals/ir-resource-conventions-crud.md` §4.1.
    pub(super) conventions: Vec<ResourceConventionAst>,
    /// Spec 0018 — `crud` overlay block (analyzer-only). At most one per
    /// resource. Merged into the synthesized commands by the conventions
    /// pass; never reaches `ir::Resource`.
    pub(super) crud_overlay: Option<CrudOverlayAst>,
    /// Authored DDL constraints (`index on`, compound `unique`, `fts on`).
    pub(super) constraints: Vec<ResourceConstraintAst>,
    /// GAP-13 — `polymorphic_ref <type> <id> targets [...]` declarations.
    pub(super) polymorphic_refs: Vec<ResourcePolymorphicRefAst>,
    /// GAP-07 — `many_through <Junction> to <Partner>` block declarations.
    pub(super) many_through: Vec<ManyThroughAst>,
    /// router-w4 — `lifecycle_routes` block.
    pub(super) lifecycle_routes: Option<ResourceLifecycleRoutesAst>,
    /// Spec 0014 — `restrict on_delete references <relation> via <fk>
    /// [where <predicate>]` referential-guard clauses. Repeatable.
    pub(super) restrict_on_delete: Vec<ResourceRestrictOnDelete>,
}

pub(super) type ResourceBodyHandler =
    for<'a> fn(&SourceLine<'a>, &str, &mut ResourceBodyState) -> Result<(), ParseError>;

pub(super) fn resource_body_handlers() -> &'static [(&'static str, ResourceBodyHandler)] {
    &[
        ("previously ", handle_resource_previously),
        ("tenancy ", handle_resource_tenancy),
        ("retention ", handle_resource_retention),
        ("validates ", handle_resource_validates),
        ("has_many ", handle_resource_has_many),
        ("lifecycle ", handle_resource_lifecycle),
        ("index ", handle_resource_index),
        ("unique ", handle_resource_unique),
        ("fts ", handle_resource_fts),
        ("polymorphic_ref ", handle_resource_polymorphic_ref),
    ]
}

fn handle_resource_lifecycle(
    line: &SourceLine<'_>,
    _rest: &str,
    _state: &mut ResourceBodyState,
) -> Result<(), ParseError> {
    Err(line_error(
        line,
        "internal: lifecycle should be dispatched inline before registry",
    ))
}

fn handle_resource_previously(
    _line: &SourceLine<'_>,
    rest: &str,
    state: &mut ResourceBodyState,
) -> Result<(), ParseError> {
    state.previously.push(rest.trim().to_owned());
    Ok(())
}

fn handle_resource_tenancy(
    line: &SourceLine<'_>,
    rest: &str,
    state: &mut ResourceBodyState,
) -> Result<(), ParseError> {
    let axis = rest.trim();
    if axis.is_empty() {
        return Err(line_error(
            line,
            "`resource tenancy` requires an axis (`org`, `team`, `none`, or a custom name)",
        ));
    }
    state.tenancy = Some(parse_defaults_tenancy(axis));
    Ok(())
}

fn handle_resource_retention(
    line: &SourceLine<'_>,
    rest: &str,
    state: &mut ResourceBodyState,
) -> Result<(), ParseError> {
    state.retention = Some(parse_resource_retention(line, rest)?);
    Ok(())
}

fn handle_resource_validates(
    _line: &SourceLine<'_>,
    rest: &str,
    state: &mut ResourceBodyState,
) -> Result<(), ParseError> {
    state.validates.push(rest.trim().to_owned());
    Ok(())
}

fn handle_resource_has_many(
    line: &SourceLine<'_>,
    rest: &str,
    state: &mut ResourceBodyState,
) -> Result<(), ParseError> {
    state.has_many.push(parse_resource_has_many(line, rest)?);
    Ok(())
}

fn handle_resource_index(
    line: &SourceLine<'_>,
    rest: &str,
    state: &mut ResourceBodyState,
) -> Result<(), ParseError> {
    let rest = rest.trim();
    let Some(target) = rest.strip_prefix("on ") else {
        return Err(line_error(
            line,
            "`index` requires `on <field>` or `on (<field>, ...)`",
        ));
    };
    let (fields, method) = parse_resource_index_target(line, target.trim())?;
    state
        .constraints
        .push(ResourceConstraintAst::Index(ResourceIndexAst {
            fields,
            method,
            full_text: false,
            span: Span::new(line.start, line.end),
        }));
    Ok(())
}

fn handle_resource_unique(
    line: &SourceLine<'_>,
    rest: &str,
    state: &mut ResourceBodyState,
) -> Result<(), ParseError> {
    let rest = rest.trim();
    // GAP-NEW-001 — three accepted forms:
    //   unique (<field>, ...)                  → table UNIQUE constraint
    //   unique (<field>, ...) when <predicate> → partial unique index
    //   unique <field> when <predicate>        → partial unique index
    // The bare single-field head is only legal when `when` follows; the
    // parenthesized form remains required otherwise so the constraint
    // stays unambiguous with the field-level `unique` modifier.
    let (fields, trailing) = if rest.starts_with('(') {
        let (fields, trailing) = parse_parenthesized_field_list_with_trailing(line, rest)?;
        (fields, trailing.trim().to_owned())
    } else {
        let mut parts = rest.splitn(2, char::is_whitespace);
        let field = parts.next().unwrap_or("").trim();
        let trailing = parts.next().unwrap_or("").trim();
        if field.is_empty() || trailing.is_empty() {
            return Err(line_error(
                line,
                "`unique` resource constraints use `unique (<field>, ...)` or \
                 `unique <field> when <predicate>`",
            ));
        }
        (vec![field.to_owned()], trailing.to_owned())
    };
    let when = parse_unique_when_trailing(line, trailing.trim())?;
    state
        .constraints
        .push(ResourceConstraintAst::Unique(ResourceUniqueAst {
            fields,
            when,
            span: Span::new(line.start, line.end),
        }));
    Ok(())
}

/// GAP-NEW-001 — interpret the text after a `unique` field list. Either
/// empty (unconditional UNIQUE constraint) or `when <predicate>` (partial
/// unique index). The predicate text is captured verbatim; the analyzer
/// runs it through the shared closed-predicate parser.
fn parse_unique_when_trailing(
    line: &SourceLine<'_>,
    trailing: &str,
) -> Result<Option<String>, ParseError> {
    if trailing.is_empty() {
        return Ok(None);
    }
    let Some(predicate) = trailing.strip_prefix("when ") else {
        return Err(line_error(
            line,
            "`unique (...)` only accepts an optional `when <predicate>` clause",
        ));
    };
    let predicate = predicate.trim();
    if predicate.is_empty() {
        return Err(line_error(
            line,
            "`unique ... when` requires a predicate (e.g. `when is_default = true`)",
        ));
    }
    Ok(Some(predicate.to_owned()))
}

fn handle_resource_fts(
    line: &SourceLine<'_>,
    rest: &str,
    state: &mut ResourceBodyState,
) -> Result<(), ParseError> {
    let rest = rest.trim();
    let Some(target) = rest.strip_prefix("on ") else {
        return Err(line_error(line, "`fts` requires `on (<field>, ...)`"));
    };
    let (fields, trailing) = parse_parenthesized_field_list_with_trailing(line, target.trim())?;
    let method = match trailing.trim() {
        "" => None,
        "gin" => Some(ResourceIndexMethodAst::Gin),
        other => {
            return Err(line_error_owned(
                line,
                format!("`fts on (...)` only accepts an optional `gin` modifier (got `{other}`)"),
            ));
        }
    };
    state
        .constraints
        .push(ResourceConstraintAst::Index(ResourceIndexAst {
            fields,
            method,
            full_text: true,
            span: Span::new(line.start, line.end),
        }));
    Ok(())
}

/// GAP-13 — parse `polymorphic_ref <type_field> <id_field> targets
/// [A, B, C]`. The two field names are bare identifiers; the target list
/// is a bracketed comma-separated PascalCase resource-name list.
fn handle_resource_polymorphic_ref(
    line: &SourceLine<'_>,
    rest: &str,
    state: &mut ResourceBodyState,
) -> Result<(), ParseError> {
    let rest = rest.trim();
    let Some(targets_idx) = rest.find(" targets ") else {
        return Err(line_error(
            line,
            "`polymorphic_ref` requires `<type_field> <id_field> targets [A, B, ...]`",
        ));
    };
    let head = rest[..targets_idx].trim();
    let mut head_parts = head.split_whitespace();
    let type_field = head_parts.next().unwrap_or("").trim();
    let id_field = head_parts.next().unwrap_or("").trim();
    if type_field.is_empty() || id_field.is_empty() || head_parts.next().is_some() {
        return Err(line_error(
            line,
            "`polymorphic_ref` takes exactly two field names before `targets`: \
             `polymorphic_ref <type_field> <id_field> targets [...]`",
        ));
    }
    let list = rest[targets_idx + " targets ".len()..].trim();
    let list = list
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .ok_or_else(|| {
            line_error(
                line,
                "`polymorphic_ref ... targets` requires a bracketed list `[A, B, ...]`",
            )
        })?;
    let targets: Vec<String> = list
        .split(',')
        .map(|t| t.trim().to_owned())
        .filter(|t| !t.is_empty())
        .collect();
    if targets.is_empty() {
        return Err(line_error(
            line,
            "`polymorphic_ref ... targets [...]` requires at least one target resource",
        ));
    }
    state.polymorphic_refs.push(ResourcePolymorphicRefAst {
        type_field: type_field.to_owned(),
        id_field: id_field.to_owned(),
        targets,
        span: Span::new(line.start, line.end),
    });
    Ok(())
}

#[cfg(test)]
mod body_handlers_tests {
    use super::super::super::parse_feature_skeletons;
    use crate::ast::{ResourceConstraintAst, ResourceIndexMethodAst};

    fn resource_with(lines: &[&str]) -> crate::ast::ResourceDecl {
        let mut source = String::from(
            "\nfeature customer\n  resource Customer\n    workspace: Workspace required\n    email: Text required\n    tags: list of Text\n",
        );
        for line in lines {
            source.push_str("    ");
            source.push_str(line);
            source.push('\n');
        }
        parse_feature_skeletons(&source)
            .expect("resource DDL authoring should parse")
            .remove(0)
            .resources
            .remove(0)
    }

    #[test]
    fn parses_single_column_index_on_parenthesized_field() {
        let resource = resource_with(&["index on (workspace)"]);
        match &resource.constraints[0] {
            ResourceConstraintAst::Index(index) => {
                assert_eq!(index.fields, vec!["workspace"]);
                assert_eq!(index.method, None);
                assert!(!index.full_text);
            }
            other => panic!("expected index constraint, got {other:?}"),
        }
    }

    #[test]
    fn parses_single_column_index_with_gin_modifier() {
        let resource = resource_with(&["index on tags gin"]);
        match &resource.constraints[0] {
            ResourceConstraintAst::Index(index) => {
                assert_eq!(index.fields, vec!["tags"]);
                assert_eq!(index.method, Some(ResourceIndexMethodAst::Gin));
                assert!(!index.full_text);
            }
            other => panic!("expected index constraint, got {other:?}"),
        }
    }

    #[test]
    fn parses_compound_unique_constraint() {
        let resource = resource_with(&["unique (workspace, email)"]);
        match &resource.constraints[0] {
            ResourceConstraintAst::Unique(unique) => {
                assert_eq!(unique.fields, vec!["workspace", "email"]);
                assert_eq!(unique.when, None);
            }
            other => panic!("expected unique constraint, got {other:?}"),
        }
    }

    #[test]
    fn parses_conditional_unique_bare_field_when_predicate() {
        // GAP-NEW-001 — `unique <field> when <pred>` partial-index form.
        let resource = resource_with(&["unique is_default when is_default = true"]);
        match &resource.constraints[0] {
            ResourceConstraintAst::Unique(unique) => {
                assert_eq!(unique.fields, vec!["is_default"]);
                assert_eq!(unique.when.as_deref(), Some("is_default = true"));
            }
            other => panic!("expected conditional unique constraint, got {other:?}"),
        }
    }

    #[test]
    fn parses_conditional_unique_parenthesized_when_predicate() {
        let resource = resource_with(&["unique (workspace, email) when email != \"\""]);
        match &resource.constraints[0] {
            ResourceConstraintAst::Unique(unique) => {
                assert_eq!(unique.fields, vec!["workspace", "email"]);
                assert_eq!(unique.when.as_deref(), Some("email != \"\""));
            }
            other => panic!("expected conditional unique constraint, got {other:?}"),
        }
    }

    #[test]
    fn rejects_bare_unique_field_without_when() {
        let source = String::from(
            "\nfeature customer\n  resource Customer\n    email: Text required\n    unique email\n",
        );
        assert!(parse_feature_skeletons(&source).is_err());
    }

    #[test]
    fn parses_polymorphic_ref_with_targets() {
        // GAP-13 — `polymorphic_ref <type> <id> targets [...]`.
        let resource = resource_with(&[
            "polymorphic_ref entity_type entity_id targets [Job, Activity, Customer]",
        ]);
        assert_eq!(resource.polymorphic_refs.len(), 1);
        let pr = &resource.polymorphic_refs[0];
        assert_eq!(pr.type_field, "entity_type");
        assert_eq!(pr.id_field, "entity_id");
        assert_eq!(pr.targets, vec!["Job", "Activity", "Customer"]);
    }

    #[test]
    fn rejects_polymorphic_ref_without_targets() {
        let source = String::from(
            "\nfeature ops\n  resource Note\n    body: Text required\n    polymorphic_ref entity_type entity_id\n",
        );
        assert!(parse_feature_skeletons(&source).is_err());
    }

    #[test]
    fn parses_resource_fts_as_full_text_gin_index() {
        let resource = resource_with(&["fts on (email, tags)"]);
        match &resource.constraints[0] {
            ResourceConstraintAst::Index(index) => {
                assert_eq!(index.fields, vec!["email", "tags"]);
                assert_eq!(index.method, None);
                assert!(index.full_text);
            }
            other => panic!("expected full-text index constraint, got {other:?}"),
        }
    }
}
