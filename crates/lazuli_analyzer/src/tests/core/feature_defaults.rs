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


    #[test]
    fn type_ref_from_syntax_lifts_semantic_currency() {
        let ty = type_ref_from_syntax("@semantic.Currency");
        assert!(matches!(
            ty,
            ir::TypeRef::Builtin(ir::BuiltinType::SemanticCurrency)
        ));
    }

    #[test]
    fn lower_feature_without_auth_keeps_field_none() {
        let source = r#"
feature customer
  agent simple
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
"#;
        let features = lazuli_syntax::parse_feature_skeletons(source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        assert!(feature.auth.is_none());
    }

    // -------------------------------------------------------------------------
    // Phase L Tier 4a — `defaults` lowering
    // -------------------------------------------------------------------------

    #[test]
    fn lower_feature_defaults_full_block() {
        let source = r#"
feature customer
  defaults
    tenancy org
    timestamps
    policy_for jobs, webhooks: @actor.system
"#;
        let features = lazuli_syntax::parse_feature_skeletons(source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        assert!(matches!(feature.defaults.tenancy, Some(ir::Tenancy::Org)));
        assert!(feature.defaults.timestamps);
        match feature.defaults.policy.as_ref().expect("policy") {
            ir::PolicyRef::Atom(atom) => assert_eq!(atom, "actor.system"),
            other => panic!("expected @actor.system atom, got {other:?}"),
        }
    }

    #[test]
    fn lower_feature_defaults_absent_keeps_default() {
        let source = r#"
feature customer
  agent simple
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
"#;
        let features = lazuli_syntax::parse_feature_skeletons(source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        assert!(feature.defaults.tenancy.is_none());
        assert!(!feature.defaults.timestamps);
        assert!(feature.defaults.policy.is_none());
    }

    #[test]
    fn lower_feature_defaults_custom_tenancy() {
        let source = r#"
feature pinned
  defaults
    tenancy workspace
"#;
        let features = lazuli_syntax::parse_feature_skeletons(source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        match feature.defaults.tenancy.as_ref().expect("axis") {
            ir::Tenancy::Custom(axis) => assert_eq!(axis, "workspace"),
            other => panic!("expected custom axis, got {other:?}"),
        }
    }

    // -------------------------------------------------------------------------
    // Phase L Tier 4c — `resource` lowering
    // -------------------------------------------------------------------------

    #[test]
    fn lower_feature_resource_lifts_retention_and_derived() {
        let source = r#"
feature customer
  domain
    resource Customer
      name: Text required
      score: Integer = 0
      is_high_value: Boolean derived from score > 80
      has_many notes: CustomerNote inverse customer

      soft_delete
      retention 7y then anonymize
      validates @validator.tier_check
"#;
        let features = lazuli_syntax::parse_feature_skeletons(source).expect("parses");
        let feature = lower_feature_skeleton(&features[0]).expect("lowers");
        assert_eq!(feature.resources.len(), 1);
        let r = &feature.resources[0];
        assert_eq!(r.name, "Customer");
        assert!(r.soft_delete);
        let ret = r.retention.as_ref().expect("retention");
        assert_eq!(ret.duration, "7y");
        assert!(matches!(ret.action, ir::RetentionAction::Anonymize));
        let derived = r
            .fields
            .iter()
            .find(|f| f.name == "is_high_value")
            .expect("is_high_value");
        assert_eq!(derived.derived_from.as_deref(), Some("score > 80"));
        // validates @validator.tier_check projects onto `Resource.validate`
        // for single-entry authoring.
        assert!(r.validate.is_some());
    }
