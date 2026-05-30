
    use super::*;

    fn p() -> &'static Path {
        Path::new("features/account/handlers/register_with_google_test.go")
    }

    #[test]
    fn hostpoint_reproducer_fires() {
        // Verbatim shape from
        // app/features/account/handlers/register_with_google_test.go:25.
        let source = "\
package accounthandlers

import \"testing\"

func TestRegisterWithGoogle_StubReturnsNotImplemented(t *testing.T) {
\t_, err := RegisterWithGoogle(ctx, input)
\tassert.Contains(t, err.Error(), \"not implemented\")
}
";
        let findings = check(source, p());
        assert_eq!(findings.len(), 1, "expected one finding, got {findings:?}");
        assert_eq!(findings[0].line, 7);
        assert_eq!(findings[0].matched_vocab, "not implemented");
        assert_eq!(findings[0].call_site, "assert.Contains(");
        assert_eq!(Finding::CODE, "TEST-PINS-STUB-VOCAB-001");
    }

    #[test]
    fn legitimate_assertion_silent() {
        // Real contract assertion against an enumerated error variant.
        let source = "\
package h
func TestLogin(t *testing.T) {
\t_, err := Login(ctx, in)
\tassert.Contains(t, err.Error(), \"invalid_credentials\")
}
";
        assert!(check(source, p()).is_empty());
    }

    #[test]
    fn comment_with_vocab_silent() {
        // Line-comment containing TODO must not fire.
        let source = "\
package h
func TestX(t *testing.T) {
\t// TODO: revisit when stub is replaced
\tassert.NoError(t, err)
}
";
        assert!(check(source, p()).is_empty());
    }

    #[test]
    fn block_comment_with_vocab_silent() {
        let source = "\
package h
/*
not implemented yet — see ticket
*/
func TestX(t *testing.T) {
\tassert.NoError(t, err)
}
";
        assert!(check(source, p()).is_empty());
    }

    #[test]
    fn test_function_name_with_vocab_silent() {
        // Function name itself contains "NotImplemented" but no
        // assertion call pins stub-vocab.
        let source = "\
package h
func TestNotImplementedReturns500(t *testing.T) {
\tresp := DoRequest()
\tassert.Equal(t, 500, resp.StatusCode)
}
";
        let findings = check(source, p());
        assert!(
            findings.is_empty(),
            "function name vocab must not fire; got {findings:?}"
        );
    }

    #[test]
    fn string_literal_outside_assertion_silent() {
        // `var x = "TODO"` — literal exists but not inside an
        // assertion call from the catalog.
        let source = "\
package h
func TestX(t *testing.T) {
\tvar x = \"TODO\"
\t_ = x
}
";
        assert!(check(source, p()).is_empty());
    }

    #[test]
    fn t_log_with_vocab_silent() {
        // t.Log is diagnostic-only and NOT in the catalog.
        let source = "\
package h
func TestX(t *testing.T) {
\tt.Log(\"not implemented yet\")
\tassert.NoError(t, err)
}
";
        assert!(check(source, p()).is_empty());
    }

    #[test]
    fn doctor_allow_opt_out_silences() {
        let source = "\
# doctor:allow TEST-PINS-STUB-VOCAB-001 — reason \"Phase 1.1 stub explicitly preserved\"
package h
func TestX(t *testing.T) {
\tassert.Contains(t, err.Error(), \"not implemented\")
}
";
        assert!(check(source, p()).is_empty());
    }

    #[test]
    fn require_equal_stub_fires() {
        let source = "\
package h
func TestX(t *testing.T) {
\trequire.Equal(t, \"stub\", got)
}
";
        let findings = check(source, p());
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].matched_vocab, "stub");
        assert_eq!(findings[0].call_site, "require.Equal(");
    }

    #[test]
    fn t_skip_with_not_ready_fires() {
        let source = "\
package h
func TestX(t *testing.T) {
\tt.Skip(\"not yet implemented\")
}
";
        let findings = check(source, p());
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].call_site, "t.Skip(");
        // Longest-match wins per STUB_VOCAB ordering.
        assert_eq!(findings[0].matched_vocab, "not yet implemented");
    }

    #[test]
    fn t_fatal_not_implemented_fires() {
        let source = "\
package h
func TestX(t *testing.T) {
\tt.Fatal(\"not implemented\")
}
";
        let findings = check(source, p());
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].call_site, "t.Fatal(");
    }

    #[test]
    fn case_insensitive_match() {
        let source = "\
package h
func TestX(t *testing.T) {
\tassert.Contains(t, err.Error(), \"Not Implemented\")
}
";
        let findings = check(source, p());
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].matched_vocab, "not implemented");
    }

    #[test]
    fn phase_one_dot_one_milestone_vocab_fires() {
        // Lazuli-specific milestone vocab from the spec.
        let source = "\
package h
func TestX(t *testing.T) {
\tassert.Contains(t, msg, \"Phase 1.1 stub\")
}
";
        let findings = check(source, p());
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].matched_vocab, "phase 1.1");
    }

    #[test]
    fn coming_soon_marketing_vocab_fires() {
        let source = "\
package h
func TestX(t *testing.T) {
\tassert.Equal(t, \"coming soon\", banner)
}
";
        let findings = check(source, p());
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].matched_vocab, "coming soon");
    }

    #[test]
    fn trailing_line_comment_with_vocab_silent() {
        // The actual assertion is fine; vocab is hidden in trailing //.
        let source = "\
package h
func TestX(t *testing.T) {
\tassert.NoError(t, err) // TODO: tighten when stub lands
}
";
        assert!(check(source, p()).is_empty());
    }

    #[test]
    fn multiple_assertions_each_fire() {
        let source = "\
package h
func TestA(t *testing.T) { assert.Contains(t, e.Error(), \"not implemented\") }
func TestB(t *testing.T) { require.Equal(t, \"stub\", got) }
";
        let findings = check(source, p());
        assert_eq!(findings.len(), 2);
    }

    #[test]
    fn strings_contains_pin_fires() {
        let source = "\
package h
func TestX(t *testing.T) {
\tif strings.Contains(err.Error(), \"not implemented\") {
\t\tt.Errorf(\"unexpected\")
\t}
}
";
        let findings = check(source, p());
        // strings.Contains AND t.Errorf are both catalog entries — but
        // t.Errorf's literal "unexpected" doesn't match. Exactly one.
        assert_eq!(findings.len(), 1, "got {findings:?}");
        assert_eq!(findings[0].call_site, "strings.Contains(");
    }

    #[test]
    fn message_renders_path_line_and_vocab() {
        let source = "\
package h
func TestX(t *testing.T) {
\tassert.Contains(t, err.Error(), \"not implemented\")
}
";
        let finding = check(source, p()).into_iter().next().unwrap();
        let msg = finding.message();
        assert!(msg.contains("register_with_google_test.go"));
        assert!(msg.contains(":3"));
        assert!(msg.contains("not implemented"));
        assert!(msg.contains("assert.Contains"));
    }

    #[test]
    fn code_constant_is_stable() {
        assert_eq!(Finding::CODE, "TEST-PINS-STUB-VOCAB-001");
    }
