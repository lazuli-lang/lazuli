    use lazuli_ir as ir;
    use crate::synthesize_conventions;
    use super::{empty_feature, me_resource, req_field, req_unique_field, user_qn};

    /// me §5.3 row 1 — `user_keyed`: resource has `user: User required
    /// unique` + `org: Org required`. Emits SELECT with
    /// `WHERE org = ctx.User.OrgID AND "user" = ctx.User.ID`.
    #[test]
    fn user_keyed_mode_emits_org_and_user_key_clauses() {
        let mut feature = empty_feature("host");
        feature.resources.push(me_resource(
            "Host",
            vec![
                req_field("org", user_qn("Org")),
                req_unique_field("user", user_qn("User")),
                req_field("name", ir::TypeRef::Builtin(ir::BuiltinType::Text)),
            ],
        ));

        let diags = synthesize_conventions(&mut feature);
        assert!(diags.is_empty(), "expected no diagnostics, got {:?}", diags);

        let q_names: Vec<&str> = feature.queries.iter().map(|q| q.name()).collect();
        assert_eq!(q_names, vec!["lookup_my_host"]);

        let lookup = feature
            .queries
            .iter()
            .find(|q| q.name() == "lookup_my_host")
            .unwrap();
        match lookup {
            ir::Query::Lookup(lq) => {
                // Route-less + param-less per §5.2.
                assert!(
                    lq.params.is_empty(),
                    "expected no params, got {:?}",
                    lq.params
                );
                // Two key clauses: org + user.
                assert_eq!(lq.keys.len(), 2);
                assert_eq!(lq.keys[0].path.segments, vec!["org".to_owned()]);
                match &lq.keys[0].equals {
                    ir::Expr::Path(p) => assert_eq!(
                        p.segments,
                        vec!["ctx".to_owned(), "actor".to_owned(), "org_id".to_owned()]
                    ),
                    other => panic!("expected Expr::Path for org, got {:?}", other),
                }
                assert_eq!(lq.keys[1].path.segments, vec!["user".to_owned()]);
                match &lq.keys[1].equals {
                    ir::Expr::Path(p) => assert_eq!(
                        p.segments,
                        vec!["ctx".to_owned(), "actor".to_owned(), "user_id".to_owned()]
                    ),
                    other => panic!("expected Expr::Path for user, got {:?}", other),
                }
                assert!(matches!(&lq.policy, ir::PolicyRef::Local(p) if p == "authenticated"));
            }
            other => panic!("expected Lookup query, got {:?}", other),
        }

        // §11 inspect surface — synth_origins records Synthesized(Me).
        assert_eq!(
            feature.synth_origins.get("lookup_my_host"),
            Some(&ir::ConventionOrigin::Synthesized(ir::ConventionRef::Me))
        );
    }

    /// me §5.3 row 2 — `user_keyed_no_org`: `user: User required` and
    /// no `org` field. Emits SELECT with `WHERE "user" = ctx.User.ID`.
    #[test]
    fn user_keyed_no_org_mode_emits_user_only_key_clause() {
        let mut feature = empty_feature("profile");
        feature.resources.push(me_resource(
            "Profile",
            vec![
                req_unique_field("user", user_qn("User")),
                req_field("bio", ir::TypeRef::Builtin(ir::BuiltinType::Text)),
            ],
        ));

        let diags = synthesize_conventions(&mut feature);
        assert!(diags.is_empty(), "got diagnostics: {:?}", diags);

        let lookup = feature
            .queries
            .iter()
            .find(|q| q.name() == "lookup_my_profile")
            .unwrap();
        match lookup {
            ir::Query::Lookup(lq) => {
                assert!(lq.params.is_empty());
                // Single key clause on `user`.
                assert_eq!(lq.keys.len(), 1);
                assert_eq!(lq.keys[0].path.segments, vec!["user".to_owned()]);
                match &lq.keys[0].equals {
                    ir::Expr::Path(p) => assert_eq!(
                        p.segments,
                        vec!["ctx".to_owned(), "actor".to_owned(), "user_id".to_owned()]
                    ),
                    other => panic!("expected Expr::Path, got {:?}", other),
                }
            }
            other => panic!("expected Lookup query, got {:?}", other),
        }
    }

    /// me §5.3 row 3 — `org_keyed`: resource has `org: Org required`
    /// AND no `user: User required` field. Emits SELECT with
    /// `WHERE org_id = ctx.User.OrgID`.
    #[test]
    fn org_keyed_mode_emits_org_only_key_clause() {
        let mut feature = empty_feature("settings");
        feature.resources.push(me_resource(
            "OrgSettings",
            vec![
                req_field("org", user_qn("Org")),
                req_field("theme", ir::TypeRef::Builtin(ir::BuiltinType::Text)),
            ],
        ));

        let diags = synthesize_conventions(&mut feature);
        assert!(diags.is_empty(), "got diagnostics: {:?}", diags);

        let lookup = feature
            .queries
            .iter()
            .find(|q| q.name() == "lookup_my_org_settings")
            .unwrap();
        match lookup {
            ir::Query::Lookup(lq) => {
                assert!(lq.params.is_empty());
                assert_eq!(lq.keys.len(), 1);
                assert_eq!(lq.keys[0].path.segments, vec!["org".to_owned()]);
                match &lq.keys[0].equals {
                    ir::Expr::Path(p) => assert_eq!(
                        p.segments,
                        vec!["ctx".to_owned(), "actor".to_owned(), "org_id".to_owned()]
                    ),
                    other => panic!("expected Expr::Path, got {:?}", other),
                }
            }
            other => panic!("expected Lookup query, got {:?}", other),
        }
    }

    /// me §5.3 row 4 — `self_keyed`: the resource IS the User table.
    /// Emits SELECT with `WHERE id = ctx.User.ID`.
    #[test]
    fn self_keyed_mode_emits_id_key_clause_for_user_resource() {
        let mut feature = empty_feature("account");
        // resource User — no `user` field needed; the row IS the actor.
        feature.resources.push(me_resource(
            "User",
            vec![
                req_unique_field(
                    "email",
                    ir::TypeRef::Builtin(ir::BuiltinType::SemanticEmail),
                ),
                req_field("name", ir::TypeRef::Builtin(ir::BuiltinType::Text)),
            ],
        ));

        let diags = synthesize_conventions(&mut feature);
        assert!(diags.is_empty(), "got diagnostics: {:?}", diags);

        let lookup = feature
            .queries
            .iter()
            .find(|q| q.name() == "lookup_my_user")
            .unwrap();
        match lookup {
            ir::Query::Lookup(lq) => {
                assert!(lq.params.is_empty());
                assert_eq!(lq.keys.len(), 1);
                assert_eq!(lq.keys[0].path.segments, vec!["id".to_owned()]);
                match &lq.keys[0].equals {
                    ir::Expr::Path(p) => assert_eq!(
                        p.segments,
                        vec!["ctx".to_owned(), "actor".to_owned(), "user_id".to_owned()]
                    ),
                    other => panic!("expected Expr::Path, got {:?}", other),
                }
            }
            other => panic!("expected Lookup query, got {:?}", other),
        }
    }

