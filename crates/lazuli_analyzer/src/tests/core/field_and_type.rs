    use lazuli_ir as ir;

    use lazuli_syntax::{parse_feature_skeletons, parse_lzx_document};

    use crate::auth::lower_auth_identity;
    use crate::query::parse_query_filter_line;
    use crate::resource::lower_validate_line;
    use crate::{
        AnalyzeError, lower_audit_block, lower_feature_skeleton, lower_lzx_document,
        lower_policy_atom_with_args, parse_cap_file_type, resolve_invalidates_targets,
        type_ref_from_syntax,
    };


    // -------------------------------------------------------------------------
    // Phase L Tier 2 — `@cap.File(...)` typing
    // -------------------------------------------------------------------------

    #[test]
    fn mfa_atom_with_args_lowers() {
        let atom = lower_policy_atom_with_args("@mfa.required(within:15m)");
        assert_eq!(atom.namespace, "mfa");
        assert_eq!(atom.name, "required");
        assert_eq!(atom.args.as_deref(), Some("within:15m"));
    }

    #[test]
    fn cap_pii_lowers() {
        let ty = type_ref_from_syntax("@cap.PII(class:contact,retention:90d,log_redact:true)");
        match ty {
            ir::TypeRef::Capability(ir::CapabilityRef::PII(pii)) => {
                assert_eq!(pii.class, "contact");
                assert_eq!(pii.retention.as_deref(), Some("90d"));
                assert_eq!(pii.log_redact, Some(true));
            }
            other => panic!("expected Capability::PII, got {other:?}"),
        }
    }

    fn lower_field_line(line: &str) -> ir::Field {
        let source = format!(
            "feature account\n  domain\n    resource Customer\n      {}\n",
            line
        );
        let features = lazuli_syntax::parse_feature_skeletons(&source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        feature
            .resources
            .into_iter()
            .next()
            .expect("resource")
            .fields
            .into_iter()
            .next()
            .expect("field")
    }

    #[test]
    fn field_with_pii_decorator_stacks_with_semantic() {
        let line = "cpf: @semantic.BrazilianCPF optional unique @cap.PII(class:\"identity\")";
        let field = lower_field_line(line);
        assert!(matches!(
            field.type_ref,
            ir::TypeRef::UserDefined(ref q) if q.name == "@semantic.BrazilianCPF"
        ));
        assert!(!field.required);
        assert!(field.unique);
        assert!(field.pii.is_some());
        assert_eq!(field.pii.as_ref().unwrap().class, "identity");
    }

    #[test]
    fn field_without_pii_decorator_has_none() {
        let field = lower_field_line("name: Text required");
        assert!(matches!(
            field.type_ref,
            ir::TypeRef::Builtin(ir::BuiltinType::Text)
        ));
        assert!(field.required);
        assert!(field.pii.is_none());
    }

    #[test]
    fn owner_axis_on_fk_field_lowers_into_ir() {
        // `ir-resource-conventions-owner-scope` §7 — happy path: a
        // user-defined FK field (here `host: Host required`) is the
        // only legal carrier for `@owner_axis(through: <ident>)`.
        let field = lower_field_line("host: Host required @owner_axis(through: user)");
        assert!(matches!(
            field.type_ref,
            ir::TypeRef::UserDefined(ref q) if q.name == "Host"
        ));
        let axis = field
            .owner_axis
            .as_ref()
            .expect("`@owner_axis(through: user)` must lower into ir::Field.owner_axis");
        assert_eq!(axis.through_column, "user");
    }

    #[test]
    fn owner_axis_on_primitive_field_emits_owner_axis_on_non_fk() {
        // `ir-resource-conventions-owner-scope` §11.1 —
        // `owner_axis_on_non_fk`. The annotation on a primitive field
        // (here `slug: Text`) is rejected at lowering: primitives carry
        // no ownership chain for the synth pass to walk.
        let source = "
feature catalog
  domain
    resource Property
      slug: Text @owner_axis(through: user)
";
        let features = lazuli_syntax::parse_feature_skeletons(source)
            .expect("parses (annotation is syntactic)");
        let err = lower_feature_skeleton(&features[0])
            .expect_err("lowering must reject @owner_axis on a non-FK field");
        match err {
            AnalyzeError::OwnerAxisOnNonFk { field, .. } => {
                assert_eq!(field, "slug");
            }
            other => panic!("expected OwnerAxisOnNonFk, got {other:?}"),
        }
    }

    #[test]
    fn field_with_pii_decorator_after_default_cleans_default() {
        let field = lower_field_line("name: Text required = anon @cap.PII(class:\"contact\")");
        assert_eq!(
            field.default,
            Some(ir::DefaultValue::EnumLiteral(ir::EnumLiteral {
                type_name: None,
                variant: "anon".to_owned(),
            }))
        );
        assert_eq!(field.pii.as_ref().unwrap().class, "contact");
    }

    #[test]
    fn audit_data_subject_lowers() {
        let spec = lower_audit_block("audit default\naudit data_subject user_id\n");
        assert_eq!(spec.subjects, vec!["default".to_owned()]);
        assert_eq!(spec.data_subject.as_deref(), Some("user_id"));
    }

    #[test]
    fn audit_before_after_lowers() {
        let spec = lower_audit_block("audit before, after\n");
        assert!(spec.record_before);
        assert!(spec.record_after);
    }

    #[test]
    fn audit_retain_lowers() {
        let spec = lower_audit_block("audit retain 90d\n");
        assert_eq!(spec.retain_for.as_deref(), Some("90d"));
    }

    #[test]
    fn validate_sanitize_html_lowers() {
        let constraints =
            lower_validate_line("validate sanitize_html(basic)").expect("valid profile");
        assert_eq!(
            constraints.sanitize_html,
            Some(ir::SanitizeHtmlProfile::Basic)
        );
    }

    #[test]
    fn validate_sanitize_html_rejects_unknown_profile() {
        let result = lower_validate_line("validate sanitize_html(unsafe)");
        assert!(matches!(
            result,
            Err(AnalyzeError::UnknownSanitizeHtmlProfile { .. })
        ));
    }

    #[test]
    fn validate_limits_lower() {
        let source = r#"
feature account
  domain
    resource Payload
      body: Json validate utf8_safe validate max_recursion:8 validate max_size:4096
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        let field = &feature.resources[0].fields[0];
        assert_eq!(field.constraints.utf8_safe, Some(true));
        assert_eq!(field.constraints.max_recursion, Some(8));
        assert_eq!(field.constraints.max_size, Some(4096));
    }

    #[test]
    fn validator_covers_pii_lowers() {
        let source = r#"
feature account
  domain
    resource Customer
      email: Text validator covers_pii
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        let field = &feature.resources[0].fields[0];
        assert_eq!(field.constraints.covers_pii.as_deref(), Some("covers_pii"));
    }

    #[test]
    fn command_route_token_kinds_lower() {
        let source = r#"
feature account
  command consume
    route opaque token: Text
    route signed_token
    returns Text
"#;
        let features = parse_feature_skeletons(source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        let command = &feature.commands[0];
        assert_eq!(command.route[0].name, "token");
        assert_eq!(command.route[0].kind, ir::RouteSlotKind::OpaqueToken);
        assert_eq!(command.route[1].name, "signed_token");
        assert_eq!(command.route[1].kind, ir::RouteSlotKind::SignedToken);
    }

    #[test]
    fn cap_file_auto_photo_policy_lowers() {
        let cap = parse_cap_file_type(
            "@cap.File(max_size:5mb,accept:image/jpeg,auto_photo_policy:@policy.host_only) optional",
        )
        .expect("cap file parses");
        assert_eq!(cap.auto_photo_policy.as_deref(), Some("@policy.host_only"));
    }

    #[test]
    fn type_ref_from_syntax_lowers_full_cap_file() {
        let ty =
            type_ref_from_syntax("@cap.File(max_size:25mb,accept:text/csv,visibility:private)");
        match ty {
            ir::TypeRef::Capability(ir::CapabilityRef::File(file)) => {
                assert_eq!(file.max_size.bytes, 25 * 1024 * 1024);
                assert!(matches!(file.max_size.literal, ir::FileSizeLiteral::Mb(25)));
                assert_eq!(file.accept.len(), 1);
                assert_eq!(file.accept[0].family, "text");
                assert_eq!(file.accept[0].subtype, "csv");
                assert_eq!(file.visibility, Some(ir::FileVisibility::Private));
                assert!(file.signed_ttl.is_none());
            }
            other => panic!("expected Capability::File, got {other:?}"),
        }
    }

    #[test]
    fn type_ref_from_syntax_lowers_multi_mime_cap_file() {
        let ty = type_ref_from_syntax(
            "@cap.File(max_size:100mb,accept:text/csv|application/vnd.ms-excel,visibility:signed,signed_ttl:1h)",
        );
        match ty {
            ir::TypeRef::Capability(ir::CapabilityRef::File(file)) => {
                assert_eq!(file.accept.len(), 2);
                assert_eq!(file.accept[1].family, "application");
                assert_eq!(file.accept[1].subtype, "vnd.ms-excel");
                assert_eq!(file.visibility, Some(ir::FileVisibility::Signed));
                assert_eq!(file.signed_ttl.as_deref(), Some("1h"));
            }
            other => panic!("expected Capability::File, got {other:?}"),
        }
    }

    #[test]
    fn type_ref_from_syntax_lowers_list_of_builtin() {
        let ty = type_ref_from_syntax("list of Text");
        match ty {
            ir::TypeRef::Many(inner) => {
                assert!(matches!(
                    *inner,
                    ir::TypeRef::Builtin(ir::BuiltinType::Text)
                ));
            }
            other => panic!("expected Many(Text), got {other:?}"),
        }
    }

    #[test]
    fn type_ref_from_syntax_lowers_list_of_user_defined_with_trailing_decorator() {
        let ty = type_ref_from_syntax("list of Post @client.visible");
        match ty {
            ir::TypeRef::Many(inner) => match *inner {
                ir::TypeRef::UserDefined(qname) => assert_eq!(qname.name, "Post"),
                other => panic!("expected Many(Post), got Many({other:?})"),
            },
            other => panic!("expected Many(Post), got {other:?}"),
        }
    }

    // Wave 0 (ir-returns-list-2026-05-22): `list <X>` (no "of") is the
    // canonical authoring form, parity with `api.output list of <X>`
    // and with pilots that commented-out `# returns list of <X>` blocks.
    #[test]
    fn type_ref_from_syntax_lowers_bare_list_builtin() {
        let ty = type_ref_from_syntax("list Text");
        match ty {
            ir::TypeRef::Many(inner) => {
                assert!(matches!(
                    *inner,
                    ir::TypeRef::Builtin(ir::BuiltinType::Text)
                ));
            }
            other => panic!("expected Many(Text), got {other:?}"),
        }
    }

    #[test]
    fn type_ref_from_syntax_lowers_bare_list_user_defined() {
        let ty = type_ref_from_syntax("list ReservationCard");
        match ty {
            ir::TypeRef::Many(inner) => match *inner {
                ir::TypeRef::UserDefined(qname) => assert_eq!(qname.name, "ReservationCard"),
                other => panic!("expected Many(UserDefined), got Many({other:?})"),
            },
            other => panic!("expected Many(...), got {other:?}"),
        }
    }

    // Case-insensitive `List <X>` parity with legacy `List of <X>`.
    #[test]
    fn type_ref_from_syntax_lowers_capital_list() {
        let ty = type_ref_from_syntax("List Post");
        match ty {
            ir::TypeRef::Many(inner) => match *inner {
                ir::TypeRef::UserDefined(qname) => assert_eq!(qname.name, "Post"),
                other => panic!("expected Many(Post), got Many({other:?})"),
            },
            other => panic!("expected Many(Post), got {other:?}"),
        }
    }

    #[test]
    fn type_ref_from_syntax_falls_through_when_cap_file_missing_max_size() {
        // No `max_size` arg → falls through to UserDefined so the LSP
        // shape diagnostic remains the canonical authority.
        let ty = type_ref_from_syntax("@cap.File(accept:text/csv)");
        assert!(matches!(ty, ir::TypeRef::UserDefined(_)));
    }

    #[test]
    fn type_ref_from_syntax_falls_through_when_cap_file_malformed_size() {
        // `25xy` is not a recognised size literal.
        let ty = type_ref_from_syntax("@cap.File(max_size:25xy,accept:text/csv)");
        assert!(matches!(ty, ir::TypeRef::UserDefined(_)));
    }

    #[test]
    fn type_ref_from_syntax_lifts_cap_hashed_argon2id() {
        // Phase L Tier 4 follow-up — `@cap.Hashed(algorithm:argon2id)`
        // now lowers into `CapabilityRef::Hashed(...)`.
        let ty = type_ref_from_syntax("@cap.Hashed(algorithm:argon2id)");
        match ty {
            ir::TypeRef::Capability(ir::CapabilityRef::Hashed(h)) => {
                assert_eq!(h.algorithm, ir::HashAlgorithm::Argon2id);
            }
            other => panic!("expected Capability::Hashed, got {other:?}"),
        }
    }

    #[test]
    fn type_ref_from_syntax_lifts_cap_token_typed() {
        let ty = type_ref_from_syntax("@cap.Token(ttl:24h,single_use:true,store:hashed)");
        match ty {
            ir::TypeRef::Capability(ir::CapabilityRef::Token(t)) => {
                assert_eq!(t.ttl, "24h");
                assert!(t.single_use);
                assert_eq!(t.store, ir::TokenStore::Hashed);
            }
            other => panic!("expected Capability::Token, got {other:?}"),
        }
    }

    #[test]
    fn type_ref_from_syntax_falls_through_on_unknown_hash_algorithm() {
        // Closed catalog: unknown algo falls through to UserDefined so
        // the LSP can surface a shape diagnostic.
        let ty = type_ref_from_syntax("@cap.Hashed(algorithm:scrypt)");
        assert!(matches!(ty, ir::TypeRef::UserDefined(_)));
    }
