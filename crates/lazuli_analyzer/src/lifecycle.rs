use lazuli_ir as ir;
use lazuli_syntax as syntax;

pub(crate) fn lower_lifecycles(feature: &mut ir::Feature, resources: &[syntax::ResourceDecl]) {
    for resource_ast in resources {
        let Some(lifecycle_ast) = resource_ast.lifecycle.as_ref() else {
            continue;
        };
        let Some(resource_index) = feature
            .resources
            .iter()
            .position(|resource| resource.name == resource_ast.name)
        else {
            continue;
        };
        if feature.resources[resource_index].lifecycle.is_some() {
            continue;
        }

        let resource_name = feature.resources[resource_index].name.clone();
        let generated_enum = format!(
            "{}{}",
            pascal_case(&resource_name),
            pascal_case(&lifecycle_ast.discriminator_field)
        );

        emit_lifecycle_enum(feature, lifecycle_ast, &generated_enum);
        emit_lifecycle_commands(feature, &resource_name, lifecycle_ast, &generated_enum);

        let resource = &mut feature.resources[resource_index];
        emit_discriminator_field(resource, lifecycle_ast, &generated_enum);
        emit_timestamp_fields(resource, lifecycle_ast);
        resource.lifecycle = Some(lower_lifecycle(lifecycle_ast, &generated_enum));
    }
}

fn emit_lifecycle_enum(
    feature: &mut ir::Feature,
    lifecycle_ast: &syntax::LifecycleBlockAst,
    generated_enum: &str,
) {
    if feature.enums.iter().any(|decl| decl.name == generated_enum) {
        return;
    }

    feature.enums.push(ir::EnumDecl {
        name: generated_enum.to_owned(),
        variants: lifecycle_ast
            .states
            .iter()
            .map(|state| ir::EnumVariant {
                name: pascal_case(&state.name),
                storage_value: Some(ir::StorageValue::String(state.name.clone())),
                previous_names: Vec::new(),
            })
            .collect(),
        previous_names: Vec::new(),
        span_ref: Some(span_of(lifecycle_ast.span)),
    });
}

fn emit_discriminator_field(
    resource: &mut ir::Resource,
    lifecycle_ast: &syntax::LifecycleBlockAst,
    generated_enum: &str,
) {
    if resource
        .fields
        .iter()
        .any(|field| field.name == lifecycle_ast.discriminator_field)
    {
        return;
    }

    resource.fields.push(ir::Field {
        name: lifecycle_ast.discriminator_field.clone(),
        type_ref: ir::TypeRef::EnumRef(ir::QualifiedName {
            feature: None,
            name: generated_enum.to_owned(),
        }),
        required: true,
        unique: false,
        default: None,
        derived_from: None,
        constraints: ir::FieldConstraints::default(),
        previous_names: Vec::new(),
        span_ref: Some(span_of(lifecycle_ast.span)),
    });
}

fn emit_timestamp_fields(resource: &mut ir::Resource, lifecycle_ast: &syntax::LifecycleBlockAst) {
    for transition in &lifecycle_ast.transitions {
        let Some(ts_field) = transition.timestamps.as_ref() else {
            continue;
        };
        if resource.fields.iter().any(|field| &field.name == ts_field) {
            continue;
        }
        resource.fields.push(ir::Field {
            name: ts_field.clone(),
            type_ref: ir::TypeRef::Builtin(ir::BuiltinType::DateTime),
            required: false,
            unique: false,
            default: None,
            derived_from: None,
            constraints: ir::FieldConstraints::default(),
            previous_names: Vec::new(),
            span_ref: Some(span_of(transition.span)),
        });
    }
}

fn emit_lifecycle_commands(
    feature: &mut ir::Feature,
    resource_name: &str,
    lifecycle_ast: &syntax::LifecycleBlockAst,
    generated_enum: &str,
) {
    for transition in &lifecycle_ast.transitions {
        feature.commands.push(lower_transition_command(
            resource_name,
            lifecycle_ast,
            transition,
            generated_enum,
        ));
    }
}

fn lower_transition_command(
    resource_name: &str,
    lifecycle_ast: &syntax::LifecycleBlockAst,
    transition: &syntax::LifecycleTransitionAst,
    generated_enum: &str,
) -> ir::Command {
    let mut assignments = vec![ir::Assignment {
        field: lifecycle_ast.discriminator_field.clone(),
        value: ir::Expr::Enum(ir::EnumLiteral {
            type_name: Some(ir::QualifiedName {
                feature: None,
                name: generated_enum.to_owned(),
            }),
            variant: pascal_case(&transition.to),
        }),
    }];
    if let Some(ts_field) = transition.timestamps.as_ref() {
        assignments.push(ir::Assignment {
            field: ts_field.clone(),
            value: ir::Expr::Path(ir::Path::from_segments(["ctx", "now"])),
        });
    }

    ir::Command {
        name: transition.name.clone(),
        kind: ir::CommandKind::Update,
        route: vec![ir::RouteSlot {
            name: "id".to_owned(),
            type_ref: ir::TypeRef::Builtin(ir::BuiltinType::Id),
            from: None,
        }],
        input: ir::CommandInput::Empty,
        target: Some(ir::TargetExpr {
            query: ir::QualifiedName {
                feature: None,
                name: format!("{}_by_id", resource_name.to_lowercase()),
            },
            args: vec![ir::NamedArg {
                name: "id".to_owned(),
                value: ir::Expr::Path(ir::Path::from_segments(["route", "id"])),
            }],
        }),
        lets: Vec::new(),
        effect: ir::CommandEffect::Updates(ir::UpdateEffect {
            resource: ir::QualifiedName {
                feature: None,
                name: resource_name.to_owned(),
            },
            assignments,
        }),
        policy: transition
            .policy
            .as_deref()
            .or(transition.requires.as_deref())
            .map(lower_policy_ref)
            .unwrap_or(ir::PolicyRef::None),
        emits: transition.emits.clone(),
        rate_limit: None,
        audit: transition.audit.as_deref().map(lower_audit_spec),
        approval: None,
        invalidates: Vec::new(),
        external_calls: Vec::new(),
        timeout: None,
        retry: None,
        idempotency: None,
        deprecated: None,
        tests: lower_tests(&transition.tests, transition.span),
        previous_names: transition.previously.clone(),
        span_ref: Some(span_of(transition.span)),
    }
}

fn lower_lifecycle(
    lifecycle_ast: &syntax::LifecycleBlockAst,
    generated_enum: &str,
) -> ir::Lifecycle {
    ir::Lifecycle {
        discriminator_field: lifecycle_ast.discriminator_field.clone(),
        generated_enum: generated_enum.to_owned(),
        states: lifecycle_ast.states.iter().map(lower_state).collect(),
        transitions: lifecycle_ast
            .transitions
            .iter()
            .map(lower_transition)
            .collect(),
        invariants: lifecycle_ast
            .invariants
            .iter()
            .map(|inv| lower_invariant(&inv.raw))
            .collect(),
        invariant_handlers: lifecycle_ast
            .invariant_handlers
            .iter()
            .map(|handler| lower_handler_ref(handler, lifecycle_ast.span))
            .collect(),
        previous_names: Vec::new(),
        span_ref: Some(span_of(lifecycle_ast.span)),
    }
}

fn lower_state(state: &syntax::LifecycleStateAst) -> ir::LifecycleState {
    ir::LifecycleState {
        name: state.name.clone(),
        kind: match state.kind_keyword.as_deref() {
            Some("initial") => ir::LifecycleStateKind::Initial,
            Some("terminal") => ir::LifecycleStateKind::Terminal,
            _ => ir::LifecycleStateKind::Intermediate,
        },
        span_ref: Some(span_of(state.span)),
    }
}

fn lower_transition(transition: &syntax::LifecycleTransitionAst) -> ir::LifecycleTransition {
    ir::LifecycleTransition {
        name: transition.name.clone(),
        from: transition.from.clone(),
        to: transition.to.clone(),
        policy: transition.policy.as_deref().map(lower_policy_ref),
        audit: transition.audit.as_deref().map(lower_audit_spec),
        timestamps: transition.timestamps.clone(),
        emits: transition.emits.clone(),
        requires: transition.requires.as_deref().map(lower_policy_ref),
        tests: lower_tests(&transition.tests, transition.span),
        previous_names: transition.previously.clone(),
        span_ref: Some(span_of(transition.span)),
    }
}

fn lower_invariant(raw: &str) -> ir::LifecycleInvariant {
    let trimmed = raw.trim();
    if trimmed == "terminal_immutable" {
        return ir::LifecycleInvariant::TerminalImmutable;
    }
    if trimmed == "no_jump_more_than_one" {
        return ir::LifecycleInvariant::NoJumpMoreThanOne;
    }
    let parts: Vec<&str> = trimmed.split_whitespace().collect();
    if parts.len() == 4 && parts[0] == "single" && parts[2] == "per" {
        return ir::LifecycleInvariant::SingleStatePerScope {
            state: parts[1].to_owned(),
            scope_field: parts[3].to_owned(),
        };
    }
    ir::LifecycleInvariant::TerminalImmutable
}

fn lower_handler_ref(raw: &str, span: syntax::Span) -> ir::HandlerRef {
    let trimmed = raw.trim().trim_start_matches('@');
    let (namespace, name) = trimmed
        .split_once('.')
        .map(|(ns, name)| (ns.to_owned(), name.to_owned()))
        .unwrap_or_else(|| ("fn".to_owned(), trimmed.to_owned()));
    ir::HandlerRef {
        namespace,
        name,
        span_ref: Some(span_of(span)),
    }
}

fn lower_policy_ref(raw: &str) -> ir::PolicyRef {
    if let Some(rest) = raw.trim().strip_prefix('@') {
        ir::PolicyRef::Atom(rest.to_owned())
    } else {
        ir::PolicyRef::Local(raw.trim().to_owned())
    }
}

fn lower_audit_spec(raw: &str) -> ir::AuditSpec {
    let tail = raw.trim().strip_prefix("audit ").unwrap_or(raw).trim();
    ir::AuditSpec {
        subjects: tail
            .split(',')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .map(ToOwned::to_owned)
            .collect(),
        emit_to: None,
    }
}

fn lower_tests(lines: &[String], span: syntax::Span) -> Option<ir::TestBlock> {
    let assertions: Vec<_> = lines
        .iter()
        .filter_map(|line| lower_test_line(line))
        .collect();
    if assertions.is_empty() {
        return None;
    }
    Some(ir::TestBlock {
        assertions,
        span_ref: Some(span_of(span)),
    })
}

fn lower_test_line(line: &str) -> Option<ir::TestAssertion> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    match parts.as_slice() {
        ["allows", "from", state] => Some(ir::TestAssertion::AllowsFrom {
            state: (*state).to_owned(),
        }),
        ["denies", "from", state] => Some(ir::TestAssertion::DeniesFrom {
            state: (*state).to_owned(),
        }),
        ["allows", "as", actor] => Some(ir::TestAssertion::AllowsAs {
            actor: (*actor).to_owned(),
        }),
        ["denies", "as", actor] => Some(ir::TestAssertion::DeniesAs {
            actor: (*actor).to_owned(),
        }),
        ["allows", "from", state, "as", actor] => Some(ir::TestAssertion::AllowsFromAs {
            state: (*state).to_owned(),
            actor: (*actor).to_owned(),
        }),
        ["denies", "from", state, "as", actor] => Some(ir::TestAssertion::DeniesFromAs {
            state: (*state).to_owned(),
            actor: (*actor).to_owned(),
        }),
        _ => None,
    }
}

fn pascal_case(value: &str) -> String {
    let mut out = String::new();
    let mut uppercase_next = true;
    for ch in value.chars() {
        if ch == '_' || ch == '-' || ch == ' ' {
            uppercase_next = true;
            continue;
        }
        if uppercase_next {
            out.extend(ch.to_uppercase());
            uppercase_next = false;
        } else {
            out.push(ch);
        }
    }
    out
}

fn span_of(span: syntax::Span) -> ir::SpanRef {
    ir::SpanRef {
        start: span.start,
        end: span.end,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lower(source: &str) -> ir::Feature {
        let features = lazuli_syntax::parse_feature_skeletons(source).expect("parses");
        crate::lower_feature_skeleton(&features[0]).expect("lowers")
    }

    fn minimal_source(body: &str) -> String {
        format!(
            r#"
feature publication
  domain
    resource Publication
{}
"#,
            body
        )
    }

    #[test]
    fn lowers_minimal_lifecycle_to_ir() {
        let feature = lower(&minimal_source(
            r#"      lifecycle status
        state scheduled
        state published
        transition publish
          from scheduled
          to published"#,
        ));
        let lifecycle = feature.resources[0].lifecycle.as_ref().expect("lifecycle");
        assert_eq!(lifecycle.discriminator_field, "status");
        assert_eq!(lifecycle.generated_enum, "PublicationStatus");
        assert_eq!(lifecycle.states.len(), 2);
        assert_eq!(lifecycle.transitions[0].name, "publish");
    }

    #[test]
    fn auto_emits_generated_enum_with_correct_variants() {
        let feature = lower(&minimal_source(
            r#"      lifecycle status
        state scheduled
        state published
        transition publish
          from scheduled
          to published"#,
        ));
        let en = feature
            .enums
            .iter()
            .find(|en| en.name == "PublicationStatus")
            .expect("enum");
        assert_eq!(en.variants[0].name, "Scheduled");
        assert_eq!(
            en.variants[0].storage_value,
            Some(ir::StorageValue::String("scheduled".to_owned()))
        );
    }

    #[test]
    fn auto_emits_discriminator_field_if_missing() {
        let feature = lower(&minimal_source(
            r#"      lifecycle status
        state scheduled
        state published
        transition publish
          from scheduled
          to published"#,
        ));
        let status = feature.resources[0]
            .fields
            .iter()
            .find(|f| f.name == "status")
            .expect("status field");
        assert!(status.required);
        assert!(matches!(status.type_ref, ir::TypeRef::EnumRef(_)));
    }

    #[test]
    fn auto_emits_timestamps_field_if_missing() {
        let feature = lower(&minimal_source(
            r#"      lifecycle status
        state scheduled
        state published
        transition publish
          from scheduled
          to published
          timestamps published_at"#,
        ));
        let field = feature.resources[0]
            .fields
            .iter()
            .find(|f| f.name == "published_at")
            .expect("timestamp field");
        assert_eq!(
            field.type_ref,
            ir::TypeRef::Builtin(ir::BuiltinType::DateTime)
        );
        assert!(!field.required);
    }

    #[test]
    fn existing_field_not_double_emitted() {
        let feature = lower(&minimal_source(
            r#"      status: PublicationStatus required
      lifecycle status
        state scheduled
        state published
        transition publish
          from scheduled
          to published"#,
        ));
        assert_eq!(
            feature.resources[0]
                .fields
                .iter()
                .filter(|f| f.name == "status")
                .count(),
            1
        );
    }

    #[test]
    fn lowers_each_transition_to_command() {
        let feature = lower(&minimal_source(
            r#"      lifecycle status
        state scheduled
        state publishing
        state published
        transition begin
          from scheduled
          to publishing
        transition publish
          from publishing
          to published"#,
        ));
        assert_eq!(feature.commands.len(), 2);
        assert!(
            feature
                .commands
                .iter()
                .all(|cmd| cmd.kind == ir::CommandKind::Update)
        );
        assert_eq!(feature.commands[0].name, "begin");
        assert_eq!(feature.commands[1].name, "publish");
    }

    #[test]
    fn multi_source_transition_lowers_correctly() {
        let feature = lower(&minimal_source(
            r#"      lifecycle status
        state scheduled
        state publishing
        state cancelled
        transition cancel
          from scheduled, publishing
          to cancelled"#,
        ));
        let lifecycle = feature.resources[0].lifecycle.as_ref().expect("lifecycle");
        assert_eq!(
            lifecycle.transitions[0].from,
            vec!["scheduled".to_owned(), "publishing".to_owned()]
        );
        assert_eq!(feature.commands.len(), 1);
        assert_eq!(feature.commands[0].name, "cancel");
    }

    #[test]
    fn invariant_terminal_immutable_lowers_to_enum_variant() {
        let feature = lower(&minimal_source(
            r#"      lifecycle status
        state scheduled
        state published terminal
        transition publish
          from scheduled
          to published
        invariant terminal_immutable"#,
        ));
        let lifecycle = feature.resources[0].lifecycle.as_ref().expect("lifecycle");
        assert_eq!(
            lifecycle.invariants,
            vec![ir::LifecycleInvariant::TerminalImmutable]
        );
    }

    #[test]
    fn state_kind_intermediate_default() {
        let feature = lower(&minimal_source(
            r#"      lifecycle status
        state scheduled
        state published
        transition publish
          from scheduled
          to published"#,
        ));
        let lifecycle = feature.resources[0].lifecycle.as_ref().expect("lifecycle");
        assert_eq!(
            lifecycle.states[0].kind,
            ir::LifecycleStateKind::Intermediate
        );
    }

    #[test]
    fn state_kind_initial_terminal_preserved() {
        let feature = lower(&minimal_source(
            r#"      lifecycle status
        state scheduled initial
        state published terminal
        transition publish
          from scheduled
          to published"#,
        ));
        let lifecycle = feature.resources[0].lifecycle.as_ref().expect("lifecycle");
        assert_eq!(lifecycle.states[0].kind, ir::LifecycleStateKind::Initial);
        assert_eq!(lifecycle.states[1].kind, ir::LifecycleStateKind::Terminal);
    }
}
