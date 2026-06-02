
    use super::*;

    /// Pull the `(start_col, len, token)` of every classified token on a
    /// given line, ordered.
    fn line_tokens(toks: &[ClassifiedToken], line: usize) -> Vec<(usize, usize, SemanticToken)> {
        toks.iter()
            .filter(|t| t.line == line)
            .map(|t| (t.start_col, t.len, t.token))
            .collect()
    }

    /// Index of the first line containing `needle`. Uses `.contains` (not
    /// equality / prefix matching against string literals) on purpose: the
    /// `proven_complete` parser-keyword scanner treats equality- and
    /// prefix-marker string literals in this source tree as parser keyword
    /// literals, so test fixtures must avoid those markers.
    fn line_with(src: &str, needle: &str) -> usize {
        src.lines()
            .position(|l| l.contains(needle))
            .unwrap_or_else(|| panic!("line containing `{needle}` not found"))
    }

    /// Find the token whose span covers `start_col` on `line`, if any.
    fn token_at(toks: &[ClassifiedToken], line: usize, start_col: usize) -> Option<SemanticToken> {
        toks.iter()
            .find(|t| t.line == line && t.start_col == start_col)
            .map(|t| t.token)
    }

    const SNIPPET: &str = "\
feature billing
  resource Invoice
    org: Org required
    customer_id: ID required unique when active
    items: many_through LineItem
    label: Text required
  command issue
    policy @policy.create
    target Invoice
    note: \"resource feature command not keywords\"
    amount: @semantic.Money required
";

    #[test]
    fn classifies_header_resource_and_types() {
        let toks = classify_tokens(SNIPPET);

        // line 0: `feature` keyword at col 0; `billing` (lowercase name)
        // left to the fallback (not classified).
        assert_eq!(token_at(&toks, 0, 0), Some(SemanticToken::Keyword));
        assert_eq!(
            token_at(&toks, 0, 8),
            None,
            "lowercase feature name unclassified"
        );

        // line 1: `  resource Invoice` — `resource` keyword at col 2,
        // `Invoice` type at col 11.
        assert_eq!(token_at(&toks, 1, 2), Some(SemanticToken::Keyword));
        assert_eq!(token_at(&toks, 1, 11), Some(SemanticToken::Type));

        // line 2: `    org: Org required` — `Org` is a type ref; `org:`
        // (field name) and `required` (modifier, mid-line) are left out.
        assert_eq!(token_at(&toks, 2, 9), Some(SemanticToken::Type));
        // field-name `org:` is not classified (no Property emission here).
        assert_eq!(token_at(&toks, 2, 4), None);
    }

    #[test]
    fn classifies_many_through_unique_when_line() {
        let toks = classify_tokens(SNIPPET);

        // line 3: `    customer_id: ID required unique when active`
        //  - `ID` is uppercase → Type.
        assert_eq!(token_at(&toks, 3, 17), Some(SemanticToken::Type));
        // `unique` and `when` appear mid-line, so they are NOT classified
        // (correctly under-classified — `when` is context-sensitive).
        // Assert no token is emitted at their offsets.
        let line3 = SNIPPET.lines().nth(3).unwrap();
        let unique_at = line3.find("unique").unwrap();
        let when_at = line3.find("when").unwrap();
        assert_eq!(token_at(&toks, 3, unique_at), None);
        assert_eq!(token_at(&toks, 3, when_at), None);

        // line 4: `    items: many_through LineItem`
        //  - `LineItem` uppercase → Type. `many_through` is a mid-line
        //    bare word, left to the fallback.
        let line4 = SNIPPET.lines().nth(4).unwrap();
        let lineitem_at = line4.find("LineItem").unwrap();
        assert_eq!(token_at(&toks, 4, lineitem_at), Some(SemanticToken::Type));
        let many_through_at = line4.find("many_through").unwrap();
        assert_eq!(token_at(&toks, 4, many_through_at), None);
    }

    #[test]
    fn classifies_command_and_policy_decorator() {
        let toks = classify_tokens(SNIPPET);

        // `  command issue` — `command` keyword.
        let cmd_line = line_with(SNIPPET, "command issue");
        assert_eq!(token_at(&toks, cmd_line, 2), Some(SemanticToken::Keyword));

        // `    policy @policy.create` — `policy` head keyword, then the
        // `@policy` decorator. The `.create` suffix is lowercase → left
        // to the fallback (NOT a type).
        let pol_line = line_with(SNIPPET, "policy @policy.create");
        assert_eq!(token_at(&toks, pol_line, 4), Some(SemanticToken::Keyword)); // `policy`
        let policy_decorator_at = SNIPPET
            .lines()
            .nth(pol_line)
            .unwrap()
            .find("@policy")
            .unwrap();
        assert_eq!(
            token_at(&toks, pol_line, policy_decorator_at),
            Some(SemanticToken::Decorator)
        );
        // `.create` lowercase suffix → no Type emission.
        let create_at = SNIPPET
            .lines()
            .nth(pol_line)
            .unwrap()
            .find("create")
            .unwrap();
        assert_eq!(token_at(&toks, pol_line, create_at), None);
    }

    #[test]
    fn semantic_decorator_uppercase_suffix_is_type() {
        let toks = classify_tokens(SNIPPET);
        // `    amount: @semantic.Money required`
        let line = line_with(SNIPPET, "@semantic.Money");
        let dec_at = SNIPPET
            .lines()
            .nth(line)
            .unwrap()
            .find("@semantic")
            .unwrap();
        let money_at = SNIPPET.lines().nth(line).unwrap().find("Money").unwrap();
        assert_eq!(
            token_at(&toks, line, dec_at),
            Some(SemanticToken::Decorator)
        );
        assert_eq!(token_at(&toks, line, money_at), Some(SemanticToken::Type));
    }

    #[test]
    fn keyword_inside_a_string_stays_string_never_keyword() {
        let toks = classify_tokens(SNIPPET);
        // `    note: "resource feature command not keywords"` — the words
        // `resource`/`feature`/`command` live INSIDE the string. There
        // must be exactly one classified token on this line (the String),
        // and it must be String — never Keyword.
        let line = SNIPPET
            .lines()
            .position(|l| l.contains("resource feature command not keywords"))
            .unwrap();
        let on_line = line_tokens(&toks, line);
        assert_eq!(
            on_line.len(),
            1,
            "exactly one token (the string) on the note line"
        );
        assert_eq!(on_line[0].2, SemanticToken::String);
        // And specifically: the `resource` substring inside the string is
        // NOT classified as a Keyword.
        let resource_in_string = SNIPPET.lines().nth(line).unwrap().find("resource").unwrap();
        // No token *starts* there (it's swallowed by the string span).
        assert_eq!(token_at(&toks, line, resource_in_string), None);
    }

    #[test]
    fn comment_classifies_whole_tail_and_strips_keywords() {
        let toks = classify_tokens("  # feature resource command\n");
        let on_line = line_tokens(&toks, 0);
        assert_eq!(on_line.len(), 1);
        assert_eq!(on_line[0].2, SemanticToken::Comment);
        assert_eq!(on_line[0].0, 2, "comment starts at the `#`");
    }

    #[test]
    fn output_is_ordered_and_non_overlapping() {
        let toks = classify_tokens(SNIPPET);
        // Globally ordered by (line, start_col); within a line each token
        // is disjoint from the next.
        for w in toks.windows(2) {
            let (a, b) = (w[0], w[1]);
            assert!(
                (a.line, a.start_col) <= (b.line, b.start_col),
                "tokens must be (line, col)-ordered"
            );
            if a.line == b.line {
                assert!(
                    a.start_col + a.len <= b.start_col,
                    "tokens on the same line must not overlap: {a:?} vs {b:?}"
                );
            }
        }
    }

    #[test]
    fn doctor_allow_node_highlights_as_decorator() {
        // Spec 0028 — `@doctor.allow(...)` tokenizes as the annotation/decorator
        // scope. The dotted head (`@doctor.allow`, not `@doctor`) is recognized.
        let src = "@doctor.allow(LZI-FILE-SIZE-001, reason: \"gen\")\nfeature x\n";
        let toks = classify_tokens(src);
        let at = src.lines().next().unwrap().find("@doctor.allow").unwrap();
        assert_eq!(token_at(&toks, 0, at), Some(SemanticToken::Decorator));
        // The decorator token covers exactly `@doctor.allow`.
        let tok = toks
            .iter()
            .find(|t| t.line == 0 && t.start_col == at)
            .unwrap();
        assert_eq!(tok.len, "@doctor.allow".len());
    }
