
    use super::super::super::parse_feature_skeletons;

    #[test]
    fn parses_slug_field_decorator() {
        let source = "
feature blog
  resource Post
    slug: Text @slug required
    title: Text required
";
        let features = parse_feature_skeletons(source).unwrap();
        let r = &features[0].resources[0];
        assert_eq!(r.fields.len(), 2);
        // First field is the slug field; `@slug` peeled, type clean.
        assert_eq!(r.fields[0].name, "slug");
        assert!(r.fields[0].slug, "`@slug` should peel into Field.slug");
        assert!(r.fields[0].required);
        assert!(
            !r.fields[0].type_text.contains("@slug"),
            "@slug should be stripped from type_text; got: {}",
            r.fields[0].type_text
        );
        // Second field has no `@slug`.
        assert!(!r.fields[1].slug);
    }

    #[test]
    fn slug_decorator_coexists_with_unique_modifier() {
        let source = "
feature blog
  resource Post
    slug: Text @slug required unique
";
        let features = parse_feature_skeletons(source).unwrap();
        let f = &features[0].resources[0].fields[0];
        assert!(f.slug);
        assert!(f.unique);
        assert!(f.required);
    }

    // -------------------------------------------------------------------
    // `ir-resource-conventions-owner-scope` Cell O1 — `@owner_axis(through: <ident>)`
    // -------------------------------------------------------------------

    #[test]
    fn parses_owner_axis_decorator_with_through_ident() {
        let source = "
feature catalog
  resource Property
    org: Org required
    host: Host required @owner_axis(through: user)
    name: Text required
";
        let features = parse_feature_skeletons(source).unwrap();
        let property = &features[0].resources[0];
        let host_field = &property.fields[1];
        assert_eq!(host_field.name, "host");
        let axis = host_field
            .owner_axis
            .as_ref()
            .expect("`@owner_axis(through: user)` should peel into ResourceFieldDecl.owner_axis");
        assert_eq!(axis.through_column, "user");
        assert!(
            !host_field.type_text.contains("@owner_axis"),
            "@owner_axis should be stripped from type_text; got: {}",
            host_field.type_text,
        );
        // The neighbouring fields stay axis-free.
        assert!(property.fields[0].owner_axis.is_none());
        assert!(property.fields[2].owner_axis.is_none());
    }

    #[test]
    fn owner_axis_rejects_string_literal_argument() {
        let source = "
feature catalog
  resource Property
    host: Host required @owner_axis(through: \"user\")
";
        let err = parse_feature_skeletons(source).expect_err(
            "string literal in @owner_axis(through: ...) must be a parse error per §7.1",
        );
        let message = format!("{err}");
        assert!(
            message.contains("requires a bare identifier"),
            "got: {message}",
        );
    }

    // -------------------------------------------------------------------
    // GAP-12 — `target @feature.<feature>.<Resource>` cross-feature FK
    // -------------------------------------------------------------------

    #[test]
    fn parses_cross_feature_target_on_id_field() {
        let source = "
feature agency
  uses department
  resource Agency
    name: Text required
    default_department_id: ID target @feature.department.Department
";
        let features = parse_feature_skeletons(source).unwrap();
        let agency = &features[0].resources[0];
        let fk = &agency.fields[1];
        assert_eq!(fk.name, "default_department_id");
        let target = fk
            .cross_feature_target
            .as_ref()
            .expect("`target @feature.department.Department` should peel into the typed slot");
        assert_eq!(target.feature, "department");
        assert_eq!(target.resource, "Department");
        // The `target ...` clause must be stripped from type_text.
        assert!(
            !fk.type_text.contains("target"),
            "target clause should be stripped from type_text; got: {}",
            fk.type_text
        );
        assert_eq!(fk.type_text.trim(), "ID");
    }

    #[test]
    fn cross_feature_target_requires_feature_and_resource() {
        let source = "
feature agency
  resource Agency
    dep_id: ID target @feature.department
";
        assert!(
            parse_feature_skeletons(source).is_err(),
            "single-segment `@feature.department` must be a parse error"
        );
    }

    // -------------------------------------------------------------------
    // W3 GAP-03 — `computed_date from <base> offset <offset>`
    // -------------------------------------------------------------------

    #[test]
    fn parses_computed_date_with_field_offset() {
        use crate::ast::{ComputedDateBaseAst, ComputedDateOffsetAst};
        let source = "
feature campaign
  resource Campaign
    campaign_start: Date required
    offset_days: Integer required
    due_date: Date computed_date from campaign_start offset offset_days
";
        let features = parse_feature_skeletons(source).unwrap();
        let campaign = &features[0].resources[0];
        let due = &campaign.fields[2];
        assert_eq!(due.name, "due_date");
        let cd = due
            .computed_date
            .as_ref()
            .expect("`computed_date from ... offset ...` should peel into the typed slot");
        assert_eq!(cd.base, ComputedDateBaseAst::Field("campaign_start".into()));
        assert_eq!(
            cd.offset,
            ComputedDateOffsetAst::Field("offset_days".into())
        );
        // `computed_date ...` clause must be stripped from type_text.
        assert_eq!(due.type_text.trim(), "Date");
        assert!(!due.type_text.contains("computed_date"));
        // Sibling fields carry no computed_date.
        assert!(campaign.fields[0].computed_date.is_none());
        assert!(campaign.fields[1].computed_date.is_none());
    }

    #[test]
    fn parses_computed_date_with_integer_literal_offset() {
        use crate::ast::{ComputedDateBaseAst, ComputedDateOffsetAst};
        let source = "
feature campaign
  resource Campaign
    campaign_start: Date required
    due_date: Date computed_date from campaign_start offset 30
";
        let features = parse_feature_skeletons(source).unwrap();
        let due = &features[0].resources[0].fields[1];
        let cd = due.computed_date.as_ref().expect("computed_date present");
        assert_eq!(cd.base, ComputedDateBaseAst::Field("campaign_start".into()));
        assert_eq!(cd.offset, ComputedDateOffsetAst::Literal(30));
        assert_eq!(due.type_text.trim(), "Date");
    }

    // -------------------------------------------------------------------
    // W4 GAP-08 — `schedule_rule from @fn.<name>(<arg>) offset <offset>`
    // -------------------------------------------------------------------

    #[test]
    fn parses_schedule_rule_with_fn_base_and_field_offset() {
        use crate::ast::{ComputedDateBaseAst, ComputedDateOffsetAst};
        let source = "
feature activity
  resource Activity
    offset_days: Integer required
    rule: Text required
    due_date: Date schedule_rule from @fn.activity_date_rule(input.rule) offset offset_days
";
        let features = parse_feature_skeletons(source).unwrap();
        let activity = &features[0].resources[0];
        let due = &activity.fields[2];
        assert_eq!(due.name, "due_date");
        let cd = due
            .computed_date
            .as_ref()
            .expect("`schedule_rule from @fn...(...) offset ...` should peel into the typed slot");
        assert_eq!(
            cd.base,
            ComputedDateBaseAst::Rule {
                rule: "input.rule".into(),
                fn_ref: "activity_date_rule".into(),
            }
        );
        assert_eq!(cd.offset, ComputedDateOffsetAst::Field("offset_days".into()));
        // The `schedule_rule ...` clause must be stripped from type_text.
        assert_eq!(due.type_text.trim(), "Date");
        assert!(!due.type_text.contains("schedule_rule"));
    }

    #[test]
    fn schedule_rule_missing_fn_base_is_a_parse_error() {
        let source = "
feature activity
  resource Activity
    due_date: Date schedule_rule from start_date offset 7
";
        assert!(
            parse_feature_skeletons(source).is_err(),
            "`schedule_rule` requires an `@fn.<name>(<arg>)` base, not a bare field"
        );
    }

    #[test]
    fn schedule_rule_missing_offset_is_a_parse_error() {
        let source = "
feature activity
  resource Activity
    due_date: Date schedule_rule from @fn.pick_date(input.rule)
";
        assert!(
            parse_feature_skeletons(source).is_err(),
            "`schedule_rule from @fn(...)` without `offset <offset>` must be a parse error"
        );
    }

    #[test]
    fn computed_date_missing_offset_keyword_is_a_parse_error() {
        let source = "
feature campaign
  resource Campaign
    campaign_start: Date required
    due_date: Date computed_date from campaign_start
";
        assert!(
            parse_feature_skeletons(source).is_err(),
            "`computed_date from <base>` without `offset <offset>` must be a parse error"
        );
    }

    #[test]
    fn computed_date_and_derived_from_are_mutually_exclusive() {
        let source = "
feature campaign
  resource Campaign
    campaign_start: Date required
    due_date: Date computed_date from campaign_start offset 30 derived from now()
";
        assert!(
            parse_feature_skeletons(source).is_err(),
            "combining `computed_date` and `derived from` on one field must be a parse error"
        );
    }

    #[test]
    fn owner_axis_without_arguments_is_a_parse_error() {
        let source = "
feature catalog
  resource Property
    host: Host required @owner_axis
";
        let err = parse_feature_skeletons(source)
            .expect_err("bare @owner_axis must be rejected — annotation requires (through: ...)");
        let message = format!("{err}");
        assert!(
            message.contains("`@owner_axis` requires `(through: <ident>)`"),
            "got: {message}",
        );
    }
