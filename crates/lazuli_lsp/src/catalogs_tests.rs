
    use super::*;

    #[test]
    fn every_catalog_entry_has_a_detail_string() {
        // Smoke test: every value in each closed catalog must yield a
        // hover description. A catalog that grows without a matching
        // detail arm is the exact regression this guards against.
        for v in RESOURCE_LOCK_STRATEGY_VALUES {
            assert!(
                resource_lock_strategy_detail(v).is_some(),
                "missing lock detail: {v}"
            );
        }
        for v in ERROR_PAGE_STATUS_VALUES {
            assert!(
                error_page_status_detail(v).is_some(),
                "missing error_page detail: {v}"
            );
        }
        for v in AUTH_CATALOG_VALUES {
            assert!(
                auth_catalog_detail(v).is_some(),
                "missing auth detail: {v}"
            );
        }
        for v in AUTH_REFRESH_THEFT_ACTION_VALUES {
            assert!(
                auth_refresh_theft_action_detail(v).is_some(),
                "missing theft action detail: {v}"
            );
        }
        for v in OBSERVABILITY_CATALOG_VALUES {
            assert!(
                observability_catalog_detail(v).is_some(),
                "missing observability detail: {v}"
            );
        }
        for v in NOTIFICATION_DIGEST_TEMPLATE_STRATEGY_VALUES {
            assert!(
                notification_digest_template_strategy_detail(v).is_some(),
                "missing notification digest detail: {v}"
            );
        }
        for v in DEPLOY_STRATEGY_VALUES {
            assert!(
                deploy_strategy_detail(v).is_some(),
                "missing deploy strategy detail: {v}"
            );
        }
        for code in ERROR_VOCAB_CODES {
            assert!(
                error_vocab_code_detail(code).is_some(),
                "missing error vocab detail: {code}"
            );
            assert!(
                error_vocab_code_builtin_en_us(code).is_some(),
                "missing error vocab builtin: {code}"
            );
        }
    }

    #[test]
    fn unknown_values_resolve_to_none() {
        assert!(resource_lock_strategy_detail("not-a-strategy").is_none());
        assert!(error_page_status_detail("999").is_none());
        assert!(auth_catalog_detail("nope").is_none());
        assert!(error_vocab_code_detail("not_a_code").is_none());
    }
