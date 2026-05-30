
    use serde_json::json;

    use super::*;

    #[test]
    fn http_method_serializes_uppercase() {
        assert_eq!(
            serde_json::to_value(HttpMethod::Post).unwrap(),
            json!("POST")
        );
        assert_eq!(
            serde_json::to_value(HttpMethod::Patch).unwrap(),
            json!("PATCH")
        );
    }

    #[test]
    fn agent_output_kind_snake_case() {
        for (k, s) in [
            (AgentOutputKind::Text, "text"),
            (AgentOutputKind::Stream, "stream"),
            (AgentOutputKind::DiscriminatedEnum, "discriminated_enum"),
            (AgentOutputKind::DiscriminatedRecord, "discriminated_record"),
        ] {
            assert_eq!(serde_json::to_value(k).unwrap(), json!(s));
        }
    }

    #[test]
    fn tool_effect_serializes_snake_case() {
        assert_eq!(
            serde_json::to_value(ToolEffect::Write).unwrap(),
            json!("write")
        );
    }

    #[test]
    fn qualified_tool_ref_local_round_trips() {
        let r = QualifiedToolRef::Local {
            kind: ToolKind::QueryLookup,
            name: "by_id".to_owned(),
        };
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(v["target"], json!("Local"));
        assert_eq!(v["kind"], json!("query_lookup"));
        let back: QualifiedToolRef = serde_json::from_value(v).unwrap();
        assert_eq!(back, r);
    }

    #[test]
    fn qualified_tool_ref_adapter_serializes_dotted() {
        let r = QualifiedToolRef::Adapter {
            dotted: vec!["calendar".to_owned(), "create_event".to_owned()],
        };
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(v["target"], json!("Adapter"));
        assert_eq!(v["dotted"], json!(["calendar", "create_event"]));
        let back: QualifiedToolRef = serde_json::from_value(v).unwrap();
        assert_eq!(back, r);
    }

    #[test]
    fn discriminator_ref_enum_round_trips() {
        let d = DiscriminatorRef::Enum(QualifiedName {
            feature: None,
            name: "Status".to_owned(),
        });
        let v = serde_json::to_value(&d).unwrap();
        assert_eq!(v["kind"], json!("Enum"));
        let back: DiscriminatorRef = serde_json::from_value(v).unwrap();
        assert_eq!(back, d);
    }

    #[test]
    fn eval_assertion_kind_round_trips() {
        for k in [EvalAssertionKind::Allows, EvalAssertionKind::Denies] {
            let v = serde_json::to_value(k).unwrap();
            let back: EvalAssertionKind = serde_json::from_value(v).unwrap();
            assert_eq!(back, k);
        }
    }

    #[test]
    fn tools_calls_op_round_trips() {
        for op in [ToolsCallsOp::Includes, ToolsCallsOp::Excludes] {
            let v = serde_json::to_value(op).unwrap();
            let back: ToolsCallsOp = serde_json::from_value(v).unwrap();
            assert_eq!(back, op);
        }
    }
