/// (4) Behavioural: for EACH validated block, a bogus child key emits
/// exactly one `app-block-child-contract` ERROR — and no valid child does.
#[test]
fn every_validated_app_block_flags_a_bogus_child() {
    for &block in VALIDATED_APP_BLOCKS {
        let header = block_header(block);
        let opener = block_opener(block).unwrap_or("");
        let indent = child_indent(block);

        // ── bogus child → exactly one ERROR ──
        let bogus = format!("app Demo\n{header}{opener}{indent}zzznonsense_xyz value\n");
        let diags = diagnostics_for(&bogus);
        let errors: Vec<&Diagnostic> = diags
            .iter()
            .filter(|d| {
                matches!(
                    d.code.as_ref(),
                    Some(tower_lsp::lsp_types::NumberOrString::String(c))
                        if c == "app-block-child-contract"
                )
            })
            .collect();
        assert_eq!(
            errors.len(),
            1,
            "block `{block}`: bogus child `zzznonsense_xyz` must emit exactly one \
             app-block-child-contract ERROR; got {} for source:\n{bogus}",
            errors.len()
        );
        assert_eq!(
            errors[0].severity,
            Some(DiagnosticSeverity::ERROR),
            "block `{block}`: app-block-child-contract must be ERROR severity"
        );

        // ── a real child of this block → ZERO app-block-child-contract ──
        let valid_key = manifest_child_keys(block)
            .next()
            .expect("validated block has ≥1 registry child key");
        let valid_src = format!("app Demo\n{header}{opener}{indent}{valid_key} value\n");
        let valid_diags = diagnostics_for(&valid_src);
        let valid_errors = valid_diags
            .iter()
            .filter(|d| {
                matches!(
                    d.code.as_ref(),
                    Some(tower_lsp::lsp_types::NumberOrString::String(c))
                        if c == "app-block-child-contract"
                )
            })
            .count();
        assert_eq!(
            valid_errors, 0,
            "block `{block}`: valid child `{valid_key}` must NOT emit app-block-child-contract; \
             got {valid_errors} for source:\n{valid_src}"
        );
    }
}

/// The headline repro: `locale` / `fallbacks` (the user's actual typo of
/// `fallback`) is an ERROR that suggests the right key, and the legitimate
/// non-keyword bodies do NOT false-fire.
#[test]
fn locale_fallbacks_typo_is_flagged_with_a_suggestion() {
    let source = "app Demo\n  locale\n    fallbacks en-US\n";
    let diags = diagnostics_for(source);
    let hits: Vec<&Diagnostic> = diags
        .iter()
        .filter(|d| {
            matches!(
                d.code.as_ref(),
                Some(tower_lsp::lsp_types::NumberOrString::String(c))
                    if c == "app-block-child-contract"
            )
        })
        .collect();
    assert_eq!(
        hits.len(),
        1,
        "locale/fallbacks must flag exactly one ERROR"
    );
    assert!(
        hits[0].message.contains("fallback"),
        "message should suggest `fallback`; got: {}",
        hits[0].message
    );

    // No false positive: valid locale children + a `pt-BR: en-US` indent-6
    // fallback body (carries `:`) + an `encryption` `key @key.x` (carries `@`)
    // must all stay silent.
    let clean = "app Demo\n  locale\n    default \"en-US\"\n    supported en-US, pt-BR\n    fallback en-US -> pt-BR\n  encryption\n    key @key.tenant\n      source env.X\n";
    let clean_diags = diagnostics_for(clean);
    let clean_hits = clean_diags
        .iter()
        .filter(|d| {
            matches!(
                d.code.as_ref(),
                Some(tower_lsp::lsp_types::NumberOrString::String(c))
                    if c == "app-block-child-contract"
            )
        })
        .count();
    assert_eq!(
        clean_hits, 0,
        "valid locale/encryption bodies must not emit app-block-child-contract; got {clean_hits}"
    );
}
