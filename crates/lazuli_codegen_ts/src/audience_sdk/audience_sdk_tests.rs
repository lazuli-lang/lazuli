
    use super::*;

    use lazuli_codegen_spec::{
        FieldKind, QueryKind, RuntimeArg, RuntimeCommand, RuntimeEffect, RuntimeFeature,
        RuntimeField, RuntimeInput, RuntimeQuery, RuntimeResource, Tenancy,
    };

    use crate::lzx_audience_slot::ir::{Audience, PolicyAtom, View};

    // -----------------------------------------------------------------------
    // Fixtures
    // -----------------------------------------------------------------------

    fn slug_resource() -> RuntimeResource {
        RuntimeResource {
            name: "slug".to_owned(),
            tenancy: Tenancy::Org,
            soft_delete: false,
            retention: None,
            fields: vec![
                RuntimeField {
                    name: "key".to_owned(),
                    kind: FieldKind::Text,
                },
                RuntimeField {
                    name: "title".to_owned(),
                    kind: FieldKind::Text,
                },
            ],
        }
    }

    fn admin_only_command(name: &str) -> RuntimeCommand {
        RuntimeCommand {
            short_name: name.to_owned(),
            policy_name: "@policy.admin_only".to_owned(),
            policy_atoms: vec![("scope".to_owned(), "workspace_admin".to_owned())],
            rate_limit: String::new(),
            validators: vec![],
            effect: RuntimeEffect::CreatesFromInput,
            inputs: vec![RuntimeInput {
                field_name: "Key".to_owned(),
                kind: FieldKind::Text,
            }],
            emits: vec![],
            invalidates: vec![],
            deprecated: None,
        }
    }

    fn member_command(name: &str) -> RuntimeCommand {
        RuntimeCommand {
            short_name: name.to_owned(),
            policy_name: "@policy.member_read".to_owned(),
            policy_atoms: vec![("scope".to_owned(), "workspace_member".to_owned())],
            rate_limit: String::new(),
            validators: vec![],
            effect: RuntimeEffect::CreatesFromInput,
            inputs: vec![RuntimeInput {
                field_name: "Key".to_owned(),
                kind: FieldKind::Text,
            }],
            emits: vec![],
            invalidates: vec![],
            deprecated: None,
        }
    }

    fn ungated_query(name: &str, kind: QueryKind) -> RuntimeQuery {
        RuntimeQuery {
            short_name: name.to_owned(),
            kind,
            policy_name: String::new(),
            policy_atoms: vec![],
            args: vec![RuntimeArg {
                field_name: "ID".to_owned(),
                kind: FieldKind::Integer,
                optional: false,
            }],
            cache: None,
            paginate: 0,
            filters: vec![],
            search: None,
            lookup_by: vec![],
        }
    }

    fn slug_feature() -> RuntimeFeature {
        RuntimeFeature {
            name: "slug".to_owned(),
            source_path: "features/slug/slug.lzi".to_owned(),
            resources: vec![slug_resource()],
            commands: vec![
                admin_only_command("create"),
                admin_only_command("delete"),
                member_command("rename"),
            ],
            queries: vec![
                ungated_query("list", QueryKind::List),
                ungated_query("by_key", QueryKind::Lookup),
            ],
        }
    }

    fn admin_audience() -> Audience {
        Audience {
            name: "admin".to_owned(),
            requires: vec![PolicyAtom {
                namespace: "scope".to_owned(),
                name: "workspace_admin".to_owned(),
                args: None,
            }],
            views: Vec::<View>::new(),
            ux: Default::default(),
            span_ref: None,
        }
    }

    fn public_audience() -> Audience {
        Audience {
            name: "public".to_owned(),
            requires: vec![PolicyAtom {
                namespace: "scope".to_owned(),
                name: "workspace_member".to_owned(),
                args: None,
            }],
            views: Vec::<View>::new(),
            ux: Default::default(),
            span_ref: None,
        }
    }

    // -----------------------------------------------------------------------
    // Tests
    // -----------------------------------------------------------------------

    /// Spec §7.2 — admin audience admits a command whose effective
    /// policy resolves to `@scope.workspace_admin`.
    #[test]
    fn admin_audience_admits_admin_command() {
        let feature = slug_feature();
        let projection = compute_audience_projection(&feature, &[admin_audience()]);

        assert!(projection.allowed_commands.contains("create"));
        assert!(projection.allowed_commands.contains("delete"));
        assert_eq!(projection.audiences, vec!["admin".to_owned()]);
    }

    /// Spec §7.2 — public audience does NOT admit an admin_only
    /// command (intersection of required atoms is empty).
    #[test]
    fn public_audience_excludes_admin_only_command() {
        let feature = slug_feature();
        let projection = compute_audience_projection(&feature, &[public_audience()]);

        assert!(!projection.allowed_commands.contains("create"));
        assert!(!projection.allowed_commands.contains("delete"));
        assert!(
            projection.allowed_commands.contains("rename"),
            "member-gated command should be admitted for public audience"
        );
    }

    /// Multiple audiences union — admin + public together admit both
    /// admin-only and member commands. Audience names appear sorted
    /// in the projection regardless of input order.
    #[test]
    fn multiple_audiences_union_correctly() {
        let feature = slug_feature();
        let projection =
            compute_audience_projection(&feature, &[public_audience(), admin_audience()]);

        assert!(projection.allowed_commands.contains("create"));
        assert!(projection.allowed_commands.contains("delete"));
        assert!(projection.allowed_commands.contains("rename"));
        assert_eq!(
            projection.audiences,
            vec!["admin".to_owned(), "public".to_owned()],
            "audience names should be sorted regardless of input order"
        );
    }

    /// Empty audiences → empty projection. No commands or queries
    /// admitted. This drives the `AUDIENCE-EMPTY-SDK` doctor warning
    /// when it lands.
    #[test]
    fn empty_audience_list_returns_empty_projection() {
        let feature = slug_feature();
        let projection = compute_audience_projection(&feature, &[]);

        assert!(projection.allowed_commands.is_empty());
        assert!(projection.allowed_queries.is_empty());
        assert!(projection.audiences.is_empty());
        assert!(projection.is_empty());
    }

    /// `emit_feature_sdk_filtered` strips filtered commands from the
    /// emitted TS. The public bundle has no `deleteSlug` const because
    /// the projection excluded `delete`.
    #[test]
    fn emit_filtered_sdk_excludes_admin_commands_for_public() {
        let feature = slug_feature();
        let projection = compute_audience_projection(&feature, &[public_audience()]);
        let output = emit_feature_sdk_filtered(&feature, &projection);

        // `rename` is the only member-gated command — its identifier
        // is `renameSlug` per the runtime emitter's convention.
        assert!(
            output.contains("renameSlug"),
            "expected renameSlug to be emitted; got:\n{output}"
        );
        // `create` and `delete` MUST be absent (admin-only).
        assert!(
            !output.contains("createSlug"),
            "createSlug should be filtered out of public bundle; got:\n{output}"
        );
        assert!(
            !output.contains("deleteSlug"),
            "deleteSlug should be filtered out of public bundle; got:\n{output}"
        );
    }

    /// Deterministic emission — same projection produces identical
    /// bytes regardless of how the audiences vec is permuted.
    #[test]
    fn projection_emission_is_deterministic() {
        let feature = slug_feature();
        let proj_a = compute_audience_projection(&feature, &[admin_audience(), public_audience()]);
        let proj_b = compute_audience_projection(&feature, &[public_audience(), admin_audience()]);
        assert_eq!(proj_a, proj_b);

        let out_a = emit_feature_sdk_filtered(&feature, &proj_a);
        let out_b = emit_feature_sdk_filtered(&feature, &proj_b);
        assert_eq!(out_a, out_b);
    }

    /// All queries are admitted whenever the audience set is
    /// non-empty (v0 IR — queries have no policy gate). This locks
    /// the documented contract so a future tightening is an explicit
    /// IR change, not a silent regression.
    #[test]
    fn queries_admitted_for_any_nonempty_audience() {
        let feature = slug_feature();

        for aud in [admin_audience(), public_audience()] {
            let projection = compute_audience_projection(&feature, &[aud]);
            assert!(projection.allowed_queries.contains("list"));
            assert!(projection.allowed_queries.contains("by_key"));
        }
    }
