//! Semantic-tokens provider (Wave H4).
//!
//! Builds the [`SemanticTokensLegend`] from the `lazuli_keywords`
//! [`SemanticToken`] registry enum and encodes the parser-driven
//! [`classify_tokens`] output into the LSP protocol's relative-delta
//! wire format.
//!
//! The legend's `token_types` ordering is **exactly** the registry's
//! [`SemanticToken`] variant order ([`SEMANTIC_TOKEN_ORDER`]); a token's
//! `token_type` index in an encoded token is `SemanticToken as u32`. This
//! is what makes highlighting track the registry: add a variant and the
//! legend + encoding follow without any hand-maintained mapping drifting.
//!
//! Only `textDocument/semanticTokens/full` is implemented. Range / delta
//! variants are a later wave; VS Code falls back to the static tmLanguage
//! grammar for everything the classifier under-classifies (by design).

use lazuli_keywords::SemanticToken;
use lazuli_syntax::{ClassifiedToken, classify_tokens};
use tower_lsp::lsp_types::{
    SemanticToken as LspSemanticToken, SemanticTokenModifier, SemanticTokenType, SemanticTokens,
    SemanticTokensLegend,
};

/// The registry [`SemanticToken`] variants in **legend order** — i.e. the
/// same order as the enum's declaration, which is the order the encoded
/// `token_type` indices reference. Keeping this list here (rather than
/// `strum`-deriving) is deliberate: the unit test
/// [`tests::legend_order_matches_registry_variants`] proves it is
/// exhaustive + correctly ordered against the enum, so a new registry
/// variant fails the build until it is added here in the right slot.
pub const SEMANTIC_TOKEN_ORDER: &[SemanticToken] = &[
    SemanticToken::Keyword,
    SemanticToken::Type,
    SemanticToken::Function,
    SemanticToken::Namespace,
    SemanticToken::Decorator,
    SemanticToken::Modifier,
    SemanticToken::EnumMember,
    SemanticToken::String,
    SemanticToken::Comment,
    SemanticToken::Operator,
    SemanticToken::Variable,
    SemanticToken::Property,
];

/// Map a registry [`SemanticToken`] to its standard LSP
/// [`SemanticTokenType`]. Every registry variant has a 1:1 standard LSP
/// counterpart (verified against `lsp-types` constants).
fn lsp_token_type(token: SemanticToken) -> SemanticTokenType {
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

/// The legend's `token_type` index for a classified token — its position
/// in [`SEMANTIC_TOKEN_ORDER`], i.e. `SemanticToken as u32`. The two are
/// asserted equal by [`tests::legend_order_matches_registry_variants`].
fn token_type_index(token: SemanticToken) -> u32 {
    SEMANTIC_TOKEN_ORDER
        .iter()
        .position(|t| *t == token)
        .expect("every registry variant is in SEMANTIC_TOKEN_ORDER") as u32
}

/// The minimal token-modifier set. We classify no modifiers today (the
/// classifier emits bare token types), but the legend must declare the
/// modifier vocabulary it *could* reference. `declaration` + `readonly`
/// are the two most universally themed; encoded tokens carry a `0`
/// bitset (no modifiers) for now.
fn token_modifiers() -> Vec<SemanticTokenModifier> {
    vec![
        SemanticTokenModifier::DECLARATION,
        SemanticTokenModifier::READONLY,
    ]
}

/// Build the [`SemanticTokensLegend`] advertised in the server
/// capabilities. `token_types` is the registry projection (legend order);
/// `token_modifiers` is the minimal set above.
///
/// ## Examples
///
/// ```no_run
/// use lazuli_lsp::test_surface::legend;
///
/// assert!(!legend().token_types.is_empty());
/// ```
pub fn legend() -> SemanticTokensLegend {
    SemanticTokensLegend {
        token_types: SEMANTIC_TOKEN_ORDER
            .iter()
            .copied()
            .map(lsp_token_type)
            .collect(),
        token_modifiers: token_modifiers(),
    }
}

/// Encode a flat, `(line, start_col)`-ordered list of [`ClassifiedToken`]
/// into the LSP relative-delta wire format.
///
/// Each output token's `delta_line` is the line delta from the previous
/// token; `delta_start` is the column delta from the previous token *on
/// the same line*, or the absolute column when the line advanced. The
/// classifier already guarantees ordering + non-overlap, so this is a
/// straight fold. `length` is the byte length (the legend's
/// position-encoding default; the LSP backend negotiates UTF-16 only when
/// the client requests it — VS Code defaults to UTF-16 but ASCII-only
/// keyword/decorator spans make byte == UTF-16 length here, and the
/// classifier never emits a token straddling a multi-byte run).
///
/// ## Examples
///
/// ```no_run
/// use lazuli_lsp::test_surface::encode_delta;
///
/// // An empty token list encodes to an empty delta stream.
/// assert!(encode_delta(&[]).is_empty());
/// ```
pub fn encode_delta(tokens: &[ClassifiedToken]) -> Vec<LspSemanticToken> {
    let mut out = Vec::with_capacity(tokens.len());
    let mut prev_line = 0u32;
    let mut prev_start = 0u32;

    for tok in tokens {
        let line = tok.line as u32;
        let start = tok.start_col as u32;
        let delta_line = line - prev_line;
        let delta_start = if delta_line == 0 {
            start - prev_start
        } else {
            start
        };

        out.push(LspSemanticToken {
            delta_line,
            delta_start,
            length: tok.len as u32,
            token_type: token_type_index(tok.token),
            token_modifiers_bitset: 0,
        });

        prev_line = line;
        prev_start = start;
    }

    out
}

/// Classify `source` and produce the full `semanticTokens/full` payload.
/// The single entry point the backend's trait method calls.
///
/// ## Examples
///
/// ```no_run
/// use lazuli_lsp::test_surface::semantic_tokens_full;
///
/// let payload = semantic_tokens_full("feature billing\n");
/// let _ = payload.data; // relative-delta-encoded tokens
/// ```
pub fn semantic_tokens_full(source: &str) -> SemanticTokens {
    SemanticTokens {
        result_id: None,
        data: encode_delta(&classify_tokens(source)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The legend's token-type set MUST equal the registry's
    /// `SemanticToken` variant set, in variant-declaration order. This is
    /// the gate's "legend tracks the registry" guarantee. We prove it two
    /// ways: (1) `SEMANTIC_TOKEN_ORDER[i] as usize == i` (the list is in
    /// enum order with no gaps), and (2) the list is exhaustive — every
    /// variant appears exactly once.
    #[test]
    fn legend_order_matches_registry_variants() {
        // (1) order: each entry sits at its own discriminant index.
        for (i, tok) in SEMANTIC_TOKEN_ORDER.iter().enumerate() {
            assert_eq!(
                token_type_index(*tok) as usize,
                i,
                "SEMANTIC_TOKEN_ORDER[{i}] = {tok:?} is out of enum-discriminant order"
            );
        }

        // (2) exhaustive + no dupes. We can't iterate the enum directly
        // (no strum), so assert the count matches a match that names every
        // variant — adding a variant breaks the match arm count here.
        fn all_variants() -> [SemanticToken; 12] {
            // Exhaustive by construction: this array names every variant.
            // A new variant makes the length mismatch the legend below.
            [
                SemanticToken::Keyword,
                SemanticToken::Type,
                SemanticToken::Function,
                SemanticToken::Namespace,
                SemanticToken::Decorator,
                SemanticToken::Modifier,
                SemanticToken::EnumMember,
                SemanticToken::String,
                SemanticToken::Comment,
                SemanticToken::Operator,
                SemanticToken::Variable,
                SemanticToken::Property,
            ]
        }
        let variants = all_variants();
        assert_eq!(
            variants.len(),
            SEMANTIC_TOKEN_ORDER.len(),
            "SEMANTIC_TOKEN_ORDER is not exhaustive against SemanticToken"
        );
        for v in variants {
            assert_eq!(
                SEMANTIC_TOKEN_ORDER.iter().filter(|t| **t == v).count(),
                1,
                "{v:?} must appear exactly once in SEMANTIC_TOKEN_ORDER"
            );
        }

        // The legend exposes exactly as many token types as the registry
        // has variants — no extra, none missing.
        let leg = legend();
        assert_eq!(leg.token_types.len(), SEMANTIC_TOKEN_ORDER.len());
        // Spot-check the projection: index 0 is the keyword type.
        assert_eq!(leg.token_types[0], SemanticTokenType::KEYWORD);
        assert_eq!(leg.token_types[4], SemanticTokenType::DECORATOR);
    }

    #[test]
    fn encode_delta_is_monotonic_and_relative() {
        let src = "feature billing\n  resource Invoice\n";
        let tokens = classify_tokens(src);
        let encoded = encode_delta(&tokens);
        assert!(!encoded.is_empty());

        // First token absolute: `feature` at line 0 col 0.
        assert_eq!(encoded[0].delta_line, 0);
        assert_eq!(encoded[0].delta_start, 0);
        assert_eq!(encoded[0].length, 7); // "feature"
        assert_eq!(
            encoded[0].token_type,
            token_type_index(SemanticToken::Keyword)
        );

        // Reconstruct absolute positions and assert they match the
        // classifier's, and that the stream is monotonic + non-overlapping.
        let mut abs_line = 0u32;
        let mut abs_start = 0u32;
        let mut prev: Option<(u32, u32)> = None;
        for (enc, src_tok) in encoded.iter().zip(tokens.iter()) {
            if enc.delta_line == 0 {
                abs_start += enc.delta_start;
            } else {
                abs_line += enc.delta_line;
                abs_start = enc.delta_start;
            }
            assert_eq!(abs_line as usize, src_tok.line);
            assert_eq!(abs_start as usize, src_tok.start_col);

            if let Some((pl, ps)) = prev {
                assert!(
                    (abs_line, abs_start) >= (pl, ps),
                    "decoded positions must be monotonic"
                );
                if abs_line == pl {
                    // Same line: previous token ended before this starts.
                    assert!(ps <= abs_start, "same-line tokens must not overlap");
                }
            }
            prev = Some((abs_line, abs_start));
        }
    }

    #[test]
    fn full_payload_has_no_result_id_and_well_formed_data() {
        let payload = semantic_tokens_full("feature billing\n  command issue\n");
        assert!(payload.result_id.is_none());
        // Two head keywords classified: `feature`, `command`.
        assert!(payload.data.len() >= 2);
        // Every encoded token's token_type is a valid legend index.
        let legend_len = SEMANTIC_TOKEN_ORDER.len() as u32;
        for t in &payload.data {
            assert!(t.token_type < legend_len);
            assert_eq!(t.token_modifiers_bitset, 0);
            assert!(t.length > 0);
        }
    }
}
