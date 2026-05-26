//! Closed-catalog code list + catalog-detail coverage.

use lazuli_lsp::{
    ERROR_VOCAB_CODES, ERROR_VOCAB_DEFAULT_VALUES, ERROR_VOCAB_EXPOSE_4XX_FIELDS,
    ERROR_VOCAB_EXPOSE_5XX_FIELDS, error_vocab_code_builtin_en_us, error_vocab_code_detail,
    keyword_description,
};

#[test]
fn closed_catalog_codes_match_proposal() {
    // Proposal §2.C + DB-INTEGRITY-CATALOG-EXT (2026-05-19) — the
    // exact 12 codes the runtime ships in `error.go:142-156`.
    // Adding/removing one requires a proposal (Rule Zero). The order
    // here mirrors the canonical-semantics example.
    let expected = [
        "policy_denied",
        "validation_failed",
        "tenant_mismatch",
        "not_found",
        "rate_limited",
        "bad_request",
        "method_not_allowed",
        "integration_error",
        "unique_violation",
        "foreign_key_violation",
        "not_null_violation",
        "check_violation",
    ];
    assert_eq!(ERROR_VOCAB_CODES, &expected);
    for code in expected {
        assert!(
            error_vocab_code_detail(code).is_some(),
            "missing one-liner detail for `{code}`"
        );
        assert!(
            error_vocab_code_builtin_en_us(code).is_some(),
            "missing built-in en-US fallback for `{code}`"
        );
        assert!(
            keyword_description(code).is_some(),
            "missing keyword_description for `{code}` — needed for completion list detail"
        );
    }
}

#[test]
fn closed_catalog_4xx_fields_include_message_key() {
    // Proposal §2.G — `message_key` is the new opt-in field added on
    // top of the pre-existing 4xx catalog. 5xx deliberately excludes
    // `message`.
    assert!(ERROR_VOCAB_EXPOSE_4XX_FIELDS.contains(&"message"));
    assert!(ERROR_VOCAB_EXPOSE_4XX_FIELDS.contains(&"code"));
    assert!(ERROR_VOCAB_EXPOSE_4XX_FIELDS.contains(&"data"));
    assert!(ERROR_VOCAB_EXPOSE_4XX_FIELDS.contains(&"message_key"));
    assert!(ERROR_VOCAB_EXPOSE_5XX_FIELDS.contains(&"code"));
    assert!(ERROR_VOCAB_EXPOSE_5XX_FIELDS.contains(&"data"));
    assert!(
        !ERROR_VOCAB_EXPOSE_5XX_FIELDS.contains(&"message"),
        "`message` must NOT appear in the 5xx exposure catalog — see §2.C"
    );
}

#[test]
fn closed_catalog_default_values_are_hide_expose() {
    assert_eq!(ERROR_VOCAB_DEFAULT_VALUES, &["hide", "expose"]);
}

#[test]
fn each_closed_code_has_keyword_description() {
    // The completion list surfaces `keyword_description` as the `detail`
    // field — every code must therefore have a description so the user
    // sees an inline hint in the dropdown.
    for code in ERROR_VOCAB_CODES {
        let detail = keyword_description(code).unwrap_or_else(|| {
            panic!("missing keyword_description for `{code}` (needed by completion list)")
        });
        assert!(
            !detail.trim().is_empty(),
            "empty description for `{code}`"
        );
    }
}
