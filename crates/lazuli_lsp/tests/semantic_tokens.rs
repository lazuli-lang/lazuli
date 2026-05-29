//! Wave H4 — end-to-end `semanticTokens/full` provider coverage.
//!
//! Exercises the **genuine production code path** the `semantic_tokens_full`
//! trait method runs (`lazuli_lsp::test_surface::{legend, encode_delta,
//! semantic_tokens_full}` — re-exported from `crate::semantic_tokens` for
//! exactly this, mirroring the existing `doctor_severity_parity` test's
//! `test_surface` discipline). Asserts:
//!
//! 1. the legend's token-type set IS the registry `SemanticToken` variant
//!    set, in registry order — the gate's "legend tracks the registry"
//!    guarantee;
//! 2. the `full` payload's deltas are well-formed: monotonic,
//!    non-overlapping, valid legend indices;
//! 3. a cross-check that a few classified tokens carry the token type the
//!    tmLanguage scope family would assign (consistency).

use lazuli_keywords::{SemanticToken, find};
use lazuli_lsp::test_surface::{SEMANTIC_TOKEN_ORDER, encode_delta, legend, semantic_tokens_full};
use lazuli_syntax::classify_tokens;
use tower_lsp::lsp_types::{SemanticTokenType, SemanticTokens};

/// The standard LSP token type each registry variant projects to. Mirror
/// of the private `lsp_token_type` mapping — kept here so the test fails
/// if the projection ever silently changes.
fn expected_lsp_type(token: SemanticToken) -> SemanticTokenType {
    match token {
        SemanticToken::Keyword => SemanticTokenType::KEYWORD,
        SemanticToken::Type => SemanticTokenType::TYPE,
        SemanticToken::Function => SemanticTokenType::FUNCTION,
        SemanticToken::Namespace => SemanticTokenType::NAMESPACE,
        SemanticToken::Decorator => SemanticTokenType::DECORATOR,
        SemanticToken::Modifier => SemanticTokenType::MODIFIER,
        SemanticToken::EnumMember => SemanticTokenType::ENUM_MEMBER,
        SemanticToken::String => SemanticTokenType::STRING,
        SemanticToken::Comment => SemanticTokenType::COMMENT,
        SemanticToken::Operator => SemanticTokenType::OPERATOR,
        SemanticToken::Variable => SemanticTokenType::VARIABLE,
        SemanticToken::Property => SemanticTokenType::PROPERTY,
    }
}

const DOC: &str = "\
feature billing
  resource Invoice
    org: Org required
    amount: @semantic.Money required
  command issue
    policy @policy.create
    note: \"feature resource not a keyword\"
";

#[test]
fn legend_token_types_are_the_registry_variants_in_order() {
    let leg = legend();

    // The legend exposes exactly one token type per registry variant, and
    // the i-th token type is the projection of the i-th registry variant
    // (SEMANTIC_TOKEN_ORDER) — so a token's `token_type` index can be read
    // back through the legend to the registry variant unambiguously.
    assert_eq!(
        leg.token_types.len(),
        SEMANTIC_TOKEN_ORDER.len(),
        "legend must declare exactly the registry variant count"
    );
    for (i, variant) in SEMANTIC_TOKEN_ORDER.iter().enumerate() {
        assert_eq!(
            leg.token_types[i],
            expected_lsp_type(*variant),
            "legend slot {i} must be the projection of registry variant {variant:?}"
        );
    }

    // A non-empty modifier vocabulary is declared (encoded tokens use a
    // zero bitset for now, but the legend must list what it could use).
    assert!(!leg.token_modifiers.is_empty());
}

#[test]
fn full_payload_deltas_are_well_formed() {
    let SemanticTokens { result_id, data } = semantic_tokens_full(DOC);
    assert!(
        result_id.is_none(),
        "full (non-delta) payload carries no result id"
    );
    assert!(
        !data.is_empty(),
        "the document classifies at least some tokens"
    );

    let legend_len = SEMANTIC_TOKEN_ORDER.len() as u32;

    // Decode the relative deltas back to absolute (line, col), asserting
    // monotonic order, no same-line overlap, valid legend index, non-zero
    // length, and zero modifier bitset.
    let mut abs_line = 0u32;
    let mut abs_start = 0u32;
    let mut prev: Option<(u32, u32, u32)> = None; // (line, start, len)
    for t in &data {
        assert!(t.token_type < legend_len, "token_type within legend bounds");
        assert!(t.length > 0, "zero-length tokens are malformed");
        assert_eq!(t.token_modifiers_bitset, 0, "no modifiers emitted yet");

        if t.delta_line == 0 {
            abs_start += t.delta_start;
        } else {
            abs_line += t.delta_line;
            abs_start = t.delta_start;
        }

        if let Some((pl, ps, plen)) = prev {
            assert!(
                (abs_line, abs_start) >= (pl, ps),
                "decoded token positions must be monotonic"
            );
            if abs_line == pl {
                assert!(
                    ps + plen <= abs_start,
                    "same-line tokens must not overlap (prev end {} > start {})",
                    ps + plen,
                    abs_start
                );
            }
        }
        prev = Some((abs_line, abs_start, t.length));
    }
}

#[test]
fn full_payload_matches_classifier_one_for_one() {
    // The encoded `full` payload is exactly the delta encoding of
    // `classify_tokens` — same count, same decoded positions.
    let classified = classify_tokens(DOC);
    let SemanticTokens { data, .. } = semantic_tokens_full(DOC);
    assert_eq!(data.len(), classified.len());
    assert_eq!(data, encode_delta(&classified));
}

/// Cross-check: the token type the classifier assigns to a literal must
/// be consistent with the tmLanguage scope family the registry records
/// for that literal. We map a handful of scope-leaf prefixes to the
/// SemanticToken they imply and assert agreement — proving the two
/// highlighting surfaces don't disagree on a token's nature.
#[test]
fn classifier_tokens_agree_with_tmlanguage_scope_family() {
    // (literal, the scope-family → SemanticToken expectation we assert)
    // `feature`/`resource`/`command` are `keyword.control.*` → Keyword.
    for kw in ["feature", "resource", "command"] {
        let spec = find(kw).unwrap_or_else(|| panic!("`{kw}` must be in the registry"));
        assert!(
            spec.scope.starts_with("keyword.control."),
            "`{kw}` tmLanguage scope `{}` should be keyword.control.*",
            spec.scope
        );
        assert_eq!(
            spec.token,
            SemanticToken::Keyword,
            "`{kw}` must classify as Keyword to match its keyword.control.* scope"
        );
    }

    // `@policy` / `@semantic` are `entity.name.tag.decorator.lazuli`
    // → Decorator.
    for dec in ["@policy", "@semantic"] {
        let spec = find(dec).unwrap_or_else(|| panic!("`{dec}` must be in the registry"));
        assert_eq!(spec.scope, "entity.name.tag.decorator.lazuli");
        assert_eq!(spec.token, SemanticToken::Decorator);
    }

    // And the live classifier output agrees on the document: the `feature`
    // head and the `@policy` decorator are present with those types.
    let toks = classify_tokens(DOC);
    let feature_tok = toks
        .iter()
        .find(|t| t.line == 0 && t.start_col == 0)
        .expect("`feature` token at 0:0");
    assert_eq!(feature_tok.token, SemanticToken::Keyword);

    // Decorator on the `policy @policy.create` line.
    let policy_line = DOC
        .lines()
        .position(|l| l.trim() == "policy @policy.create")
        .unwrap();
    let policy_col = DOC
        .lines()
        .nth(policy_line)
        .unwrap()
        .find("@policy")
        .unwrap();
    let dec_tok = toks
        .iter()
        .find(|t| t.line == policy_line && t.start_col == policy_col)
        .expect("`@policy` decorator token");
    assert_eq!(dec_tok.token, SemanticToken::Decorator);
}
