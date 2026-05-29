    // Doctor auth_session tenant / extra-columns / callsite tests
    // Split from crates/lazuli_cli/src/doctor/tests.rs.

    use super::test_support_packages::*;
    use super::test_support_core::*;
    use crate::doctor::*;
    use crate::doctor::aggregators::auth::auth_diagnostics;

    // -------------------------------------------------------------------------
    // AUTH-SESSION-* doctor codes — tenant-pin shim validation
    // -------------------------------------------------------------------------

    fn auth_fact_with_extra_columns(
        feature: &str,
        sessions_resource: &str,
        extra_columns: Vec<ir::SessionExtraColumn>,
    ) -> AuthFacts {
        AuthFacts {
            feature: feature.to_owned(),
            auth: ir::Auth {
                identity: ir::AuthIdentity {
                    field: ir::FieldRef {
                        resource: ir::QualifiedName {
                            feature: None,
                            name: "User".to_owned(),
                        },
                        field: "email".to_owned(),
                    },
                    public_contract: None,
                },
                password: None,
                sessions: Some(ir::AuthSessions {
                    resource: ir::QualifiedName {
                        feature: None,
                        name: sessions_resource.to_owned(),
                    },
                    ttl: "7 days".to_owned(),
                    refresh: false,
                    extra_columns,
                    access_ttl: None,
                    rotation: None,
                    cookie: None,
                }),
                mfa: None,
                oauth: vec![],
                span_ref: None,
            },
            path: PathBuf::from(format!("features/{feature}/{feature}.lzi")),
            line: 1,
            identity_line: 1,
            password_line: None,
            password_algorithm_line: None,
            sessions_line: Some(5),
            sessions_resource_line: Some(6),
            mfa_line: None,
            oauth_lines: BTreeMap::new(),
        }
    }

    fn extra_id_column(field_name: &str) -> ir::SessionExtraColumn {
        ir::SessionExtraColumn {
            field_name: field_name.to_owned(),
            column_name: format!("{field_name}_id"),
            go_type: "lazuli.ID".to_owned(),
            references: Some("Org".to_owned()),
            required: true,
        }
    }

    fn extra_non_id_column(field_name: &str) -> ir::SessionExtraColumn {
        ir::SessionExtraColumn {
            field_name: field_name.to_owned(),
            column_name: field_name.to_owned(),
            go_type: "string".to_owned(),
            references: None,
            required: true,
        }
    }

    fn call_auth_diagnostics(facts: &[AuthFacts]) -> Vec<DoctorDiagnostic> {
        let mut feature_resources: BTreeMap<String, BTreeMap<String, ResourceFact>> =
            BTreeMap::new();
        for fact in facts {
            if let Some(sessions) = fact.auth.sessions.as_ref() {
                let mut resources: BTreeMap<String, ResourceFact> = BTreeMap::new();
                resources.insert(
                    sessions.resource.name.clone(),
                    ResourceFact {
                        path: fact.path.clone(),
                        line: 1,
                        fields: BTreeMap::new(),
                    },
                );
                feature_resources.insert(fact.feature.clone(), resources);
            }
        }
        auth_diagnostics(
            facts,
            &feature_resources,
            &BTreeMap::new(),
            &BTreeMap::new(),
            None,
        )
    }

    #[test]
    fn auth_session_tenant_001_fires_on_non_id_go_type() {
        let fact = auth_fact_with_extra_columns(
            "auth_feature",
            "TenantSession",
            vec![extra_non_id_column("region")],
        );
        let diagnostics = call_auth_diagnostics(&[fact]);
        let codes: BTreeSet<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(
            codes.contains("AUTH-SESSION-TENANT-001"),
            "expected AUTH-SESSION-TENANT-001, got {codes:?}"
        );
    }

    #[test]
    fn auth_session_tenant_001_does_not_fire_on_id_type() {
        let fact = auth_fact_with_extra_columns(
            "auth_feature",
            "TenantSession",
            vec![extra_id_column("org")],
        );
        let diagnostics = call_auth_diagnostics(&[fact]);
        let codes: BTreeSet<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(
            !codes.contains("AUTH-SESSION-TENANT-001"),
            "AUTH-SESSION-TENANT-001 must not fire for lazuli.ID columns; got {codes:?}"
        );
    }

    #[test]
    fn auth_session_extra_001_fires_on_two_extra_columns() {
        let fact = auth_fact_with_extra_columns(
            "auth_feature",
            "TenantSession",
            vec![extra_id_column("org"), extra_id_column("workspace")],
        );
        let diagnostics = call_auth_diagnostics(&[fact]);
        let codes: BTreeSet<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(
            codes.contains("AUTH-SESSION-EXTRA-001"),
            "expected AUTH-SESSION-EXTRA-001 for 2 extra columns; got {codes:?}"
        );
        let errors: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.code == "AUTH-SESSION-EXTRA-001")
            .collect();
        assert_eq!(
            errors[0].severity,
            DoctorSeverity::Error,
            "AUTH-SESSION-EXTRA-001 must be error severity"
        );
    }

    #[test]
    fn auth_session_extra_001_does_not_fire_on_one_extra_column() {
        let fact = auth_fact_with_extra_columns(
            "auth_feature",
            "TenantSession",
            vec![extra_id_column("org")],
        );
        let diagnostics = call_auth_diagnostics(&[fact]);
        let codes: BTreeSet<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(
            !codes.contains("AUTH-SESSION-EXTRA-001"),
            "AUTH-SESSION-EXTRA-001 must not fire for a single extra column; got {codes:?}"
        );
    }

    #[test]
    fn auth_session_extra_001_does_not_fire_when_no_extra_columns() {
        let fact = auth_fact_with_extra_columns("auth_feature", "TenantSession", vec![]);
        let diagnostics = call_auth_diagnostics(&[fact]);
        let codes: BTreeSet<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(
            !codes.contains("AUTH-SESSION-EXTRA-001"),
            "AUTH-SESSION-EXTRA-001 must not fire when extra_columns is empty; got {codes:?}"
        );
    }

    #[test]
    fn auth_session_callsite_001_fires_on_issue_session_call_in_handler() {
        let root = temp_project_root("callsite-001-fires");
        let handler_path = root
            .join("features")
            .join("auth_feature")
            .join("handlers")
            .join("login.go");
        write_file(
            &handler_path,
            r#"package handlers

import "github.com/lazuli-lang/lazuli/runtime/go/lazuli/auth"

func Login(ctx *lazuli.Ctx, input LoginInput) (string, error) {
    token, _, err := auth.IssueSession(ctx, db, userID, auth.SessionAttrs{})
    return token, err
}
"#,
        );

        let fact = auth_fact_with_extra_columns(
            "auth_feature",
            "TenantSession",
            vec![extra_id_column("org")],
        );
        let diagnostics = check_auth_session_callsite_001(&[fact], &root);
        let codes: BTreeSet<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(
            codes.contains("AUTH-SESSION-CALLSITE-001"),
            "expected AUTH-SESSION-CALLSITE-001 for auth.IssueSession in user handler; got {codes:?}"
        );
    }

    #[test]
    fn auth_session_callsite_001_does_not_fire_when_no_extra_columns() {
        let root = temp_project_root("callsite-001-no-extra");
        let handler_path = root
            .join("features")
            .join("auth_feature")
            .join("handlers")
            .join("login.go");
        write_file(
            &handler_path,
            r#"package handlers

func Login(ctx *lazuli.Ctx, input LoginInput) (string, error) {
    token, _, err := auth.IssueSession(ctx, db, userID, auth.SessionAttrs{})
    return token, err
}
"#,
        );

        let fact = auth_fact_with_extra_columns("auth_feature", "TenantSession", vec![]);
        let diagnostics = check_auth_session_callsite_001(&[fact], &root);
        let codes: BTreeSet<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(
            !codes.contains("AUTH-SESSION-CALLSITE-001"),
            "AUTH-SESSION-CALLSITE-001 must not fire when session has no extra columns; got {codes:?}"
        );
    }

    #[test]
    fn auth_session_callsite_001_skips_gen_go_files() {
        let root = temp_project_root("callsite-001-skip-gen");
        let gen_path = root
            .join("features")
            .join("auth_feature")
            .join("handlers")
            .join("login.gen.go");
        write_file(
            &gen_path,
            "func Login() { auth.IssueSession(ctx, db, id, auth.SessionAttrs{}) }\n",
        );

        let fact = auth_fact_with_extra_columns(
            "auth_feature",
            "TenantSession",
            vec![extra_id_column("org")],
        );
        let diagnostics = check_auth_session_callsite_001(&[fact], &root);
        let codes: BTreeSet<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(
            !codes.contains("AUTH-SESSION-CALLSITE-001"),
            "AUTH-SESSION-CALLSITE-001 must not fire for .gen.go files; got {codes:?}"
        );
    }

    // ---------------------------------------------------------------
    // Roadmap §1.2 — HTTP hygiene contracts: cookie / proxy / limits.
    // Each block ships one diagnostic code that fires on any of its
    // closed-catalog violations.
