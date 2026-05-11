//! Phase L — LSP test for `auth` block hover + closed-catalog
//! completion shape. Drives `keyword_description`,
//! `AUTH_CATALOG_VALUES`, and `auth_catalog_detail` directly because
//! they are the public surface the LSP server exposes to clients via
//! `hover` and `completion`.

use lazuli_lsp::{AUTH_CATALOG_VALUES, auth_catalog_detail, keyword_description};

#[test]
fn auth_hover_covers_eleven_keywords() {
    // The proposal pins exactly 11 hovers (plus `hash`, kept for
    // grammar symmetry with `verify`).
    let required = [
        "auth",
        "identity",
        "password",
        "algorithm",
        "oauth",
        "mfa",
        "sessions",
        "ttl",
        "refresh",
        "enroll",
        "verify",
        "adapter",
        "hash",
    ];
    for keyword in required {
        let hover = keyword_description(keyword);
        assert!(
            hover.is_some(),
            "missing hover for `{keyword}` — expected from docs/proposals/bucket-auth-cycle.md §Doctor/LSP",
        );
    }
}

#[test]
fn algorithm_hover_mentions_closed_catalog() {
    let hover = keyword_description("algorithm").expect("hover");
    assert!(hover.contains("argon2id"), "{hover}");
    assert!(hover.contains("bcrypt"), "{hover}");
}

#[test]
fn oauth_hover_mentions_closed_catalog() {
    let hover = keyword_description("oauth").expect("hover");
    assert!(hover.contains("google"), "{hover}");
    assert!(hover.contains("github"), "{hover}");
}

#[test]
fn mfa_hover_mentions_closed_catalog() {
    let hover = keyword_description("mfa").expect("hover");
    assert!(hover.contains("totp"), "{hover}");
}

#[test]
fn refresh_hover_mentions_default() {
    let hover = keyword_description("refresh").expect("hover");
    assert!(hover.contains("Default"), "{hover}");
    assert!(hover.contains("false"), "{hover}");
}

#[test]
fn auth_catalog_values_cover_all_four_closed_catalogs() {
    // `algorithm` → argon2id | bcrypt
    assert!(AUTH_CATALOG_VALUES.contains(&"argon2id"));
    assert!(AUTH_CATALOG_VALUES.contains(&"bcrypt"));
    // `oauth` → google | github | microsoft | apple
    assert!(AUTH_CATALOG_VALUES.contains(&"google"));
    assert!(AUTH_CATALOG_VALUES.contains(&"github"));
    assert!(AUTH_CATALOG_VALUES.contains(&"microsoft"));
    assert!(AUTH_CATALOG_VALUES.contains(&"apple"));
    // `mfa` → totp
    assert!(AUTH_CATALOG_VALUES.contains(&"totp"));
    // `refresh` → true | false
    assert!(AUTH_CATALOG_VALUES.contains(&"true"));
    assert!(AUTH_CATALOG_VALUES.contains(&"false"));
}

#[test]
fn auth_catalog_detail_covers_every_value() {
    for value in AUTH_CATALOG_VALUES {
        assert!(
            auth_catalog_detail(value).is_some(),
            "missing catalog detail for `{value}`",
        );
    }
}
