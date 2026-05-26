    use lazuli_ir as ir;
    use crate::{ConventionSynthDiagnostic, synthesize_conventions};
    use super::{empty_feature, me_resource, req_field, req_unique_field, user_qn};

    /// me §6 — author wrote `query lookup_my_customer`; synth skips
    /// that name, records `AuthorOverride(Me)` in `synth_origins`. No
    /// duplicate query, no diagnostic when the signature matches.
    #[test]
    fn author_override_skips_synth_and_records_origin() {
        let mut feature = empty_feature("customer");
        feature.resources.push(me_resource(
            "Customer",
            vec![
                req_field("org", user_qn("Org")),
                req_unique_field("user", user_qn("User")),
                req_field("name", ir::TypeRef::Builtin(ir::BuiltinType::Text)),
            ],
        ));

        // Author wrote their own `lookup_my_customer` query (e.g.,
        // with a role-gated policy) — canonical-matching shape (no
        // params, Lookup variant).
        feature.queries.push(ir::Query::Lookup(ir::LookupQuery {
            name: "lookup_my_customer".to_owned(),
            public_contract: None,
            params: Vec::new(),
            keys: Vec::new(),
            scope: Vec::new(),
            scope_override: false,
            filters: Vec::new(),
            policy: ir::PolicyRef::Local("customer_admin".to_owned()),
            policy_expr: None,
            policy_when_denied: None,
            previous_names: Vec::new(),
            span_ref: None,
            owner_scope_sql: None,
        }));

        let diags = synthesize_conventions(&mut feature);
        assert!(
            diags.is_empty(),
            "expected no diagnostics for matching override, got {:?}",
            diags
        );

        // Exactly one `lookup_my_customer` — the author's.
        let count = feature
            .queries
            .iter()
            .filter(|q| q.name() == "lookup_my_customer")
            .count();
        assert_eq!(count, 1);

        // Author's policy preserved (not overwritten by synth).
        let q = feature
            .queries
            .iter()
            .find(|q| q.name() == "lookup_my_customer")
            .unwrap();
        match q {
            ir::Query::Lookup(lq) => {
                assert!(matches!(&lq.policy, ir::PolicyRef::Local(p) if p == "customer_admin"));
            }
            other => panic!("expected Lookup, got {:?}", other),
        }

        // §11 — synth_origins records `AuthorOverride(Me)`.
        assert_eq!(
            feature.synth_origins.get("lookup_my_customer"),
            Some(&ir::ConventionOrigin::AuthorOverride(ir::ConventionRef::Me))
        );
    }

    /// me §6.1 — `conventions [crud, me]` composes cleanly: 5 from
    /// crud + 1 from me = 6 entries, no naming collisions. All 6
    /// names appear in `synth_origins`.
    #[test]
    fn conventions_crud_and_me_compose_to_six_entries() {
        let mut feature = empty_feature("customer");
        let mut r = me_resource(
            "Customer",
            vec![
                req_field("org", user_qn("Org")),
                req_unique_field("user", user_qn("User")),
                req_field("name", ir::TypeRef::Builtin(ir::BuiltinType::Text)),
            ],
        );
        // Declare both bundles.
        r.conventions = vec![ir::ConventionRef::Crud, ir::ConventionRef::Me];
        feature.resources.push(r);

        let diags = synthesize_conventions(&mut feature);
        assert!(diags.is_empty(), "got diagnostics: {:?}", diags);

        // 3 crud commands + 0 me commands.
        let cmd_names: std::collections::BTreeSet<String> =
            feature.commands.iter().map(|c| c.name.clone()).collect();
        assert!(cmd_names.contains("create_customer"));
        assert!(cmd_names.contains("update_customer"));
        assert!(cmd_names.contains("delete_customer"));
        assert_eq!(cmd_names.len(), 3, "got commands: {:?}", cmd_names);

        // 2 crud queries + 1 me query.
        let q_names: std::collections::BTreeSet<String> = feature
            .queries
            .iter()
            .map(|q| q.name().to_owned())
            .collect();
        assert!(q_names.contains("lookup_customer"));
        assert!(q_names.contains("list_customers"));
        assert!(q_names.contains("lookup_my_customer"));
        assert_eq!(q_names.len(), 3, "got queries: {:?}", q_names);

        // §11 inspect — synth_origins has 6 entries: 5 crud + 1 me.
        assert_eq!(
            feature.synth_origins.len(),
            6,
            "expected 6 synth_origins entries, got {:?}",
            feature.synth_origins
        );
        // Spot-check the 5 crud entries.
        for name in [
            "create_customer",
            "update_customer",
            "delete_customer",
            "lookup_customer",
            "list_customers",
        ] {
            assert_eq!(
                feature.synth_origins.get(name),
                Some(&ir::ConventionOrigin::Synthesized(ir::ConventionRef::Crud)),
                "expected Synthesized(Crud) for `{}`",
                name
            );
        }
        // And the 1 me entry.
        assert_eq!(
            feature.synth_origins.get("lookup_my_customer"),
            Some(&ir::ConventionOrigin::Synthesized(ir::ConventionRef::Me))
        );
    }

    /// me §11.1 — `me_synth_no_actor_resolution` fires when the
    /// resource has neither `user` nor `org` and is not named `User`.
    /// No synth emitted for that resource.
    #[test]
    fn no_actor_resolution_diagnostic_when_no_user_no_org_not_user() {
        let mut feature = empty_feature("audit");
        feature.resources.push(me_resource(
            "AuditNote",
            vec![req_field(
                "note",
                ir::TypeRef::Builtin(ir::BuiltinType::Text),
            )],
        ));

        let diags = synthesize_conventions(&mut feature);
        assert!(
            diags.iter().any(|d| matches!(
                d,
                ConventionSynthDiagnostic::MeNoActorResolution { resource }
                    if resource == "AuditNote"
            )),
            "expected MeNoActorResolution for AuditNote, got {:?}",
            diags
        );

        // No `lookup_my_audit_note` synthesized.
        assert!(
            feature
                .queries
                .iter()
                .all(|q| q.name() != "lookup_my_audit_note"),
            "synth should skip the resource entirely on no actor axis"
        );
        // No entry in synth_origins.
        assert!(!feature.synth_origins.contains_key("lookup_my_audit_note"));
    }

    /// me §11.1 — `me_synth_signature_mismatch` fires when the author
    /// wrote a divergent shape (e.g., a `Query::List` named
    /// `lookup_my_<r>`; or a Lookup with non-empty params).
    #[test]
    fn divergent_author_signature_emits_mismatch_diagnostic() {
        let mut feature = empty_feature("traveler");
        feature.resources.push(me_resource(
            "Traveler",
            vec![
                req_field("org", user_qn("Org")),
                req_unique_field("user", user_qn("User")),
            ],
        ));

        // Author wrote a Lookup with non-empty params — diverges from
        // the canonical route-less + param-less shape.
        feature.queries.push(ir::Query::Lookup(ir::LookupQuery {
            name: "lookup_my_traveler".to_owned(),
            public_contract: None,
            params: vec![ir::TypedSlot {
                name: "extra".to_owned(),
                type_ref: ir::TypeRef::Builtin(ir::BuiltinType::Text),
                required: false,
                constraints: ir::FieldConstraints::default(),
                validate_skip: false,
            }],
            keys: Vec::new(),
            scope: Vec::new(),
            scope_override: false,
            filters: Vec::new(),
            policy: ir::PolicyRef::None,
            policy_expr: None,
            policy_when_denied: None,
            previous_names: Vec::new(),
            span_ref: None,
            owner_scope_sql: None,
        }));

        let diags = synthesize_conventions(&mut feature);
        assert!(
            diags.iter().any(|d| matches!(
                d,
                ConventionSynthDiagnostic::MeSignatureMismatch { resource, synth_name, .. }
                    if resource == "Traveler" && synth_name == "lookup_my_traveler"
            )),
            "expected MeSignatureMismatch for lookup_my_traveler, got {:?}",
            diags
        );

        // §6 — synth still records AuthorOverride(Me) so inspect can
        // render the override annotation.
        assert_eq!(
            feature.synth_origins.get("lookup_my_traveler"),
            Some(&ir::ConventionOrigin::AuthorOverride(ir::ConventionRef::Me))
        );
    }

    /// Sanity — resource without `conventions [me]` is a no-op for the
    /// `me` half of the synth (existing crud-no-op test covers the
    /// joint path; this one anchors the bundle-isolation property).
    #[test]
    fn resource_without_me_convention_is_no_op() {
        let mut feature = empty_feature("customer");
        let mut r = me_resource(
            "Customer",
            vec![
                req_field("org", user_qn("Org")),
                req_unique_field("user", user_qn("User")),
            ],
        );
        r.conventions = Vec::new();
        feature.resources.push(r);

        let diags = synthesize_conventions(&mut feature);
        assert!(diags.is_empty());
        assert!(feature.queries.is_empty());
        assert!(feature.synth_origins.is_empty());
    }
