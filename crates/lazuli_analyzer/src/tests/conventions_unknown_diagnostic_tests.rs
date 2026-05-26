    //! ir-resource-conventions-crud Cell C1 — tests for the
    //! `conventions_unknown` diagnostic plumbing. Cell C2 (parser)
    //! will be the actual emit site; here we lock the suggestion
    //! helper + the error formatting so the parser's emission shape
    //! is stable before it lands.

    use crate::{AnalyzeError, CONVENTION_CATALOG, conventions_unknown_suggestion};

    #[test]
    fn catalog_contains_crud_and_me_today() {
        // crud §4.2 + me §4.2 — closed catalog is `{ crud, me }`.
        // Any further addition is an IR change requiring a proposal;
        // this test fails on accidental growth.
        assert_eq!(CONVENTION_CATALOG, &["crud", "me"]);
    }

    #[test]
    fn suggestion_for_single_char_typo_returns_crud() {
        // §4.3 names this exact case verbatim: `conventions [crd]`
        // suggests `crud` (single-character Levenshtein).
        assert_eq!(conventions_unknown_suggestion("crd"), Some("crud"));
    }

    #[test]
    fn suggestion_for_extra_char_typo_returns_crud() {
        // `crude` and `cruds` are also distance-1 from `crud`.
        assert_eq!(conventions_unknown_suggestion("crude"), Some("crud"));
        assert_eq!(conventions_unknown_suggestion("cruds"), Some("crud"));
    }

    #[test]
    fn suggestion_for_typo_resolves_to_me() {
        // `ir-resource-conventions-me.md` cell M1: typos distance-1
        // from `me` resolve to `me`. `m` (deletion), `mee`/`mes`
        // (insertion / substitution). Locks the nearest-match
        // behaviour now that the catalog has a second entry.
        assert_eq!(conventions_unknown_suggestion("m"), Some("me"));
        assert_eq!(conventions_unknown_suggestion("mee"), Some("me"));
        assert_eq!(conventions_unknown_suggestion("mes"), Some("me"));
    }

    #[test]
    fn suggestion_for_far_typo_returns_none() {
        // Distance 2+ from every catalog entry — no suggestion is
        // better than a misleading one.
        assert_eq!(conventions_unknown_suggestion("workflow"), None);
        assert_eq!(conventions_unknown_suggestion("xyz"), None);
        assert_eq!(conventions_unknown_suggestion(""), None);
    }

    #[test]
    fn suggestion_for_exact_match_returns_self() {
        // Defensive: if the parser somehow calls this with a known
        // identifier, the helper still resolves rather than failing.
        // (The parser shouldn't reach this path — exact matches don't
        // hit the unknown diagnostic — but the helper is total.)
        assert_eq!(conventions_unknown_suggestion("crud"), Some("crud"));
    }

    #[test]
    fn error_message_includes_suggestion_when_present() {
        let err = AnalyzeError::ConventionsUnknown {
            resource: "Customer".to_owned(),
            identifier: "crd".to_owned(),
            suggestion: Some("crud".to_owned()),
        };
        let msg = err.to_string();
        assert!(
            msg.contains("CONVENTIONS-UNKNOWN"),
            "missing diagnostic code: {msg}"
        );
        assert!(msg.contains("`Customer`"), "missing resource name: {msg}");
        assert!(msg.contains("`crd`"), "missing offending identifier: {msg}");
        assert!(
            msg.contains("did you mean `crud`?"),
            "missing suggestion clause: {msg}"
        );
    }

    #[test]
    fn error_message_omits_suggestion_clause_when_none() {
        let err = AnalyzeError::ConventionsUnknown {
            resource: "Customer".to_owned(),
            identifier: "workflow".to_owned(),
            suggestion: None,
        };
        let msg = err.to_string();
        assert!(
            msg.contains("CONVENTIONS-UNKNOWN"),
            "missing diagnostic code: {msg}"
        );
        assert!(msg.contains("`workflow`"));
        assert!(
            !msg.contains("did you mean"),
            "should not invent a suggestion when none was found: {msg}"
        );
    }
