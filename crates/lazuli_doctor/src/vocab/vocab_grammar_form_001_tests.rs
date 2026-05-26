    use super::*;

    fn check_src(source: &str) -> Vec<Finding> {
        check(source, Path::new("features/test/test.lzi"))
    }

    fn feature(body: &str) -> String {
        format!("feature test\n  domain\n{body}")
    }

    #[test]
    fn positive_validates_resource_validator_fires() {
        let findings = check_src(&feature(
            "    resource Account\n      id: ID required\n      validates resource @validator.account\n",
        ));

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].old, "validates resource @validator.account");
        assert_eq!(findings[0].new, "validates @validator.account");
        assert_eq!(Finding::CODE, "VOCAB-GRAMMAR-FORM-001");
    }

    #[test]
    fn negative_canonical_validates_validator_does_not_fire() {
        assert!(
            check_src(&feature(
                "    resource Account\n      id: ID required\n      validates @validator.account\n",
            ))
            .is_empty()
        );
    }

    #[test]
    fn positive_validates_field_validator_fires() {
        let findings = check_src(&feature(
            "    resource Account\n      email: Text required\n      validates field email @validator.email\n",
        ));

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].old, "validates field email @validator.email");
        assert_eq!(findings[0].new, "validates @validator.email");
    }

    #[test]
    fn negative_resource_inline_validator_path_does_not_count_as_scoped_validator() {
        assert!(check_src(&feature(
            "    resource Account\n      id: ID required\n      validates resource \"./account.go\"\n",
        ))
        .is_empty());
    }

    #[test]
    fn positive_inline_previously_on_resource_header_fires() {
        let findings = check_src(&feature(
            "    resource Account previously migrated Customer\n      id: ID required\n",
        ));

        assert_eq!(findings.len(), 1);
        assert_eq!(
            findings[0].old,
            "resource Account previously migrated Customer"
        );
        assert!(
            findings[0]
                .new
                .contains("\n      previously migrated Customer")
        );
    }

    #[test]
    fn negative_child_previously_does_not_fire() {
        assert!(
            check_src(&feature(
                "    resource Account\n      previously migrated Customer\n      id: ID required\n",
            ))
            .is_empty()
        );
    }

    #[test]
    fn positive_validate_path_fires() {
        let findings = check_src(&feature(
            "    resource Account\n      id: ID required\n      validate \"./account.go\"\n",
        ));

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].old, "validate \"./account.go\"");
        assert_eq!(findings[0].new, "validates field <name> \"./account.go\"");
    }

    #[test]
    fn negative_command_validate_validator_does_not_fire() {
        assert!(
            check_src("feature test\n  command create\n    validate @validator.account\n")
                .is_empty()
        );
    }

    #[test]
    fn golden_combines_all_four_forms() {
        let findings = check_src(&feature(
            "    resource Account previously alias Customer\n      id: ID required\n      email: Text required previously migrated email_address\n      validates resource @validator.account\n      validates field email @validator.email\n      validate \"./account.go\"\n",
        ));

        assert_eq!(findings.len(), 5);
        assert!(
            findings
                .iter()
                .any(|f| f.old == "validates resource @validator.account")
        );
        assert!(
            findings
                .iter()
                .any(|f| f.old == "validates field email @validator.email")
        );
        assert!(
            findings
                .iter()
                .any(|f| f.old == "resource Account previously alias Customer")
        );
        assert!(
            findings
                .iter()
                .any(|f| f.old == "email: Text required previously migrated email_address")
        );
        assert!(
            findings
                .iter()
                .any(|f| f.old == "validate \"./account.go\"")
        );
    }
