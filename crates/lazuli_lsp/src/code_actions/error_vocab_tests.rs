
    use super::*;

    fn uri() -> Url {
        Url::parse("file:///example.lzi").expect("valid URI")
    }

    #[test]
    fn scaffold_errors_offered_on_feature_header_without_errors_block() {
        let source = "feature billing\n  query.lookup me\n";
        let actions = error_vocab_code_actions(&source, &uri(), Position { line: 0, character: 0 });
        assert!(
            !actions.is_empty(),
            "expected a scaffold action on the feature header"
        );
    }

    #[test]
    fn no_actions_outside_error_vocab_lines() {
        let source = "feature billing\n  query.lookup me\n";
        let actions = error_vocab_code_actions(
            &source,
            &uri(),
            Position {
                line: 1,
                character: 6,
            },
        );
        assert!(actions.is_empty());
    }
