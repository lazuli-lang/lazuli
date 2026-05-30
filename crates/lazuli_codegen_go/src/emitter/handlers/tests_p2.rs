#[test]
fn extension_declarations_emit_starter_stubs() {
    let mut feature = base_feature("customer");
    feature.extensions.push(extension(
        "risk_score",
        ExtensionContract::Function {
            input: TypeRef::UserDefined(QualifiedName {
                feature: None,
                name: "Customer".to_owned(),
            }),
            output: TypeRef::Builtin(BuiltinType::Integer),
        },
    ));
    let module = module_with_features(vec![feature]);

    let files = emit_handler_stubs(&module, "lazuli/test", &BTreeSet::new());

    let risk = files
        .iter()
        .find(|file| file.path == "app/features/customer/handlers/risk_score.go")
        .expect("risk_score declaration stub emitted");
    assert!(
        risk.contents
            .contains("//   Site: customer.extensions.fn.risk_score")
    );
}

#[test]
fn unresolved_signature_falls_back_to_any() {
    let mut feature = base_feature("customer");
    feature.uses.push("@fn.unknown".to_owned());
    let module = module_with_features(vec![feature]);

    let files = emit_handler_stubs(&module, "lazuli/test", &BTreeSet::new());
    let file = &files[0];

    assert!(
        file.contents
            .contains("func Unknown(ctx *lazuli.Ctx, input any) (any, error)")
    );
    assert!(file.contents.contains("var zero any"));
    assert!(
        file.contents
            .contains("return zero, errors.New(\"unknown not yet implemented\")")
    );
}

#[test]
fn many_and_unresolved_types_render_without_extra_imports() {
    assert_eq!(
        go_type_for_stub(&TypeRef::Many(Box::new(TypeRef::Builtin(BuiltinType::Id)))),
        "[]lazuli.ID"
    );
    assert_eq!(
        go_type_for_stub(&TypeRef::Unresolved(
            "@cap.Hashed(algorithm:argon2id)".to_owned()
        )),
        "lazuli.HashedRef"
    );
}

#[test]
fn sanitises_unsafe_path_and_function_name() {
    assert_eq!(
        handler_path("customer", "../risk.score"),
        "app/features/customer/handlers/risk_score.go"
    );
    assert_eq!(exported_func_name("123_score"), "Handler123Score");
}

#[test]
fn extracts_multiple_handler_refs_from_text() {
    let refs = extract_handler_refs(
        "score = @fn.risk_score(target) then @hook.before_create and @client.cell",
    );

    assert_eq!(
        refs,
        vec![
            HandlerRef {
                namespace: HandlerNamespace::Fn,
                name: "risk_score".to_owned(),
            },
            HandlerRef {
                namespace: HandlerNamespace::Hook,
                name: "before_create".to_owned(),
            },
        ]
    );
}

#[allow(dead_code)]
fn _typed_slot_compiles(_: TypedSlot) {}
