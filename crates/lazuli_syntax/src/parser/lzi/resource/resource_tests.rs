
    use super::super::parse_feature_skeletons;

    #[test]
    fn resource_full_block_parses() {
        let source = r#"
feature customer
  domain
    resource Customer
      previously migrated Account
      owner: User optional
      name: Text required
      email: @semantic.Email @pii.contact required
      lifecycle_stage: CustomerStatus = lead
        previously migrated status
      score: Integer @pii.derived = 0
      external_id: @cap.Encrypted(key:@key.tenant) @pii.external optional
      is_high_value: Boolean derived from score > 80
      has_many notes: CustomerNote inverse customer

      soft_delete
      retention 7y then anonymize

      validates @validator.tier_check
"#;
        let features = parse_feature_skeletons(source).unwrap();
        let resources = &features[0].resources;
        assert_eq!(resources.len(), 1);
        let r = &resources[0];
        assert_eq!(r.name, "Customer");
        assert_eq!(r.previously, vec!["migrated Account"]);
        assert!(r.soft_delete);
        let ret = r.retention.as_ref().expect("retention");
        assert_eq!(ret.duration, "7y");
        assert!(matches!(
            ret.action,
            crate::ResourceRetentionAction::Anonymize
        ));
        assert_eq!(r.validates, vec!["@validator.tier_check"]);
        assert_eq!(r.has_many.len(), 1);
        assert_eq!(r.has_many[0].name, "notes");
        assert_eq!(r.has_many[0].type_text, "CustomerNote");
        assert_eq!(r.has_many[0].inverse.as_deref(), Some("customer"));
        // 7 fields (owner, name, email, lifecycle_stage, score, external_id,
        // is_high_value).
        assert_eq!(r.fields.len(), 7);
        let lifecycle = r
            .fields
            .iter()
            .find(|f| f.name == "lifecycle_stage")
            .expect("lifecycle_stage");
        assert_eq!(lifecycle.type_text, "CustomerStatus");
        assert_eq!(lifecycle.default.as_deref(), Some("lead"));
        assert_eq!(lifecycle.previously, vec!["migrated status"]);
        let derived = r
            .fields
            .iter()
            .find(|f| f.name == "is_high_value")
            .expect("is_high_value");
        assert_eq!(derived.derived_from.as_deref(), Some("score > 80"));
        let external = r
            .fields
            .iter()
            .find(|f| f.name == "external_id")
            .expect("external_id");
        assert!(external.optional);
        assert!(
            external
                .type_text
                .starts_with("@cap.Encrypted(key:@key.tenant)")
        );
    }

    #[test]
    fn resource_append_only_modifier_parses() {
        // W4 GAP-AUDIT-02 — bare `append_only` resource modifier.
        let source = r#"
feature ledger
  resource Entry
    amount: Integer required
    append_only
"#;
        let features = parse_feature_skeletons(source).unwrap();
        let entry = &features[0].resources[0];
        assert!(entry.append_only, "`append_only` should lift onto the flag");
    }

    #[test]
    fn resource_without_append_only_defaults_false() {
        let source = r#"
feature ledger
  resource Entry
    amount: Integer required
"#;
        let features = parse_feature_skeletons(source).unwrap();
        assert!(!features[0].resources[0].append_only);
    }

    #[test]
    fn resource_retention_invalid_action_errors() {
        let source = r#"
feature customer
  domain
    resource Customer
      name: Text required
      retention 7y then incinerate
"#;
        let err = parse_feature_skeletons(source).unwrap_err();
        let message = format!("{err}");
        assert!(
            message.contains("anonymize"),
            "error should list valid actions: {message}"
        );
    }

    #[test]
    fn many_through_block_parses_with_partner_and_payload() {
        // GAP-07 — `many_through <Junction> to <Partner>` block with a
        // single payload field at grandchild indent.
        let source = r#"
feature staffing
  resource Job
    title: Text required
    many_through JobMember to User
      role_in_job: Text required
"#;
        let features = parse_feature_skeletons(source).unwrap();
        let job = &features[0].resources[0];
        assert_eq!(job.many_through.len(), 1);
        let mt = &job.many_through[0];
        assert_eq!(mt.name, "JobMember");
        assert_eq!(mt.partner, "User");
        assert_eq!(mt.payload.len(), 1);
        assert_eq!(mt.payload[0].name, "role_in_job");
        assert_eq!(mt.payload[0].type_text, "Text");
        assert!(mt.payload[0].required);
    }

    #[test]
    fn many_through_round_trips_through_serde() {
        // GAP-07 — AST serde round-trip preserves the junction + payload.
        let source = r#"
feature staffing
  resource Job
    title: Text required
    many_through JobMember to User
      role_in_job: Text required
      rate: Integer optional
"#;
        let job = parse_feature_skeletons(source).unwrap().remove(0).resources.remove(0);
        let json = serde_json::to_string(&job).unwrap();
        let back: crate::ast::ResourceDecl = serde_json::from_str(&json).unwrap();
        assert_eq!(back.many_through, job.many_through);
        assert_eq!(back.many_through[0].payload.len(), 2);
    }

    #[test]
    fn many_through_without_to_errors() {
        let source = r#"
feature staffing
  resource Job
    title: Text required
    many_through JobMember
      role_in_job: Text required
"#;
        let err = parse_feature_skeletons(source).unwrap_err();
        assert!(format!("{err}").contains("to <PartnerResource>"));
    }

    #[test]
    fn many_through_without_payload_errors() {
        // A junction with no metadata is a plain `has_many`, not a
        // `many_through`.
        let source = r#"
feature staffing
  resource Job
    title: Text required
    many_through JobMember to User
"#;
        let err = parse_feature_skeletons(source).unwrap_err();
        assert!(format!("{err}").contains("at least one payload field"));
    }

    #[test]
    fn soft_delete_deleted_by_parses_actor_form() {
        // Spec 0015 — `soft_delete by` sets BOTH `soft_delete` (back-compat
        // base) and the new `soft_delete_actor` flag. Bare `soft_delete`
        // leaves `soft_delete_actor` false (deleted_at-only, unchanged).
        let actor_src = r#"
feature billing
  domain
    resource Invoice
      amount: Integer required
      soft_delete by
"#;
        let features = parse_feature_skeletons(actor_src).unwrap();
        let r = &features[0].resources[0];
        assert!(r.soft_delete, "`soft_delete by` implies soft_delete");
        assert!(
            r.soft_delete_actor,
            "`soft_delete by` must set the actor-projection flag"
        );

        // Bare `soft_delete` is back-compat: actor flag stays false.
        let bare_src = r#"
feature billing
  domain
    resource Invoice
      amount: Integer required
      soft_delete
"#;
        let bare = parse_feature_skeletons(bare_src).unwrap();
        let br = &bare[0].resources[0];
        assert!(br.soft_delete);
        assert!(
            !br.soft_delete_actor,
            "bare `soft_delete` must NOT project deleted_by"
        );
    }

