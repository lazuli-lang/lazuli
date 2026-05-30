
    use super::*;

    #[test]
    fn lzx_platform_serde_snake_case() {
        assert_eq!(
            serde_json::to_value(LzxPlatform::Web).unwrap(),
            serde_json::json!("web")
        );
    }

    #[test]
    fn lzx_resume_arm_kind_wildcard_serde_tagged() {
        let v = serde_json::to_value(LzxResumeArmKind::Wildcard).unwrap();
        assert_eq!(v["kind"], "wildcard");
    }

    #[test]
    fn lzx_view_test_assertion_feature_and_span() {
        let a = LzxViewTestAssertion::AllowsExtension {
            feature: "billing".into(),
            span: Span::new(5, 10),
        };
        assert_eq!(a.feature(), "billing");
        assert_eq!(a.span(), Span::new(5, 10));
    }
