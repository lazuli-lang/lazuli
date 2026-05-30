fn body_has_success_assertion(body: &str) -> bool {
    SUCCESS_ASSERTIONS.iter().any(|marker| {
        let mut search_from = 0;
        while let Some(rel) = body[search_from..].find(marker) {
            let abs = search_from + rel;
            let after = &body[abs + marker.len()..];
            // Defuse `assert.NotEmpty(t, err.Error())` and similar —
            // if the call's tail leads with an `err` token, it's an
            // error-side assertion masquerading as positive.
            let trimmed = after.trim_start();
            // Skip the leading `t, ` if present (testify convention).
            let after_t = if let Some(rest) = trimmed.strip_prefix("t,") {
                rest.trim_start()
            } else if let Some(rest) = trimmed.strip_prefix("t ,") {
                rest.trim_start()
            } else {
                trimmed
            };
            let is_err_target = ERR_TARGET_HINTS
                .iter()
                .any(|hint| after_t.starts_with(hint.trim_end_matches([',', ')'])));
            if !is_err_target {
                return true;
            }
            search_from = abs + marker.len();
        }
        false
    })
}

fn body_has_error_assertion(body: &str) -> bool {
    if ERROR_ASSERTIONS.iter().any(|m| body.contains(m)) {
        return true;
    }
    // `assert.Contains(t, err.Error(), ...)` / `require.Contains(...,
    // err.Error(), ...)` — detect by the `err.Error()` substring
    // inside any `Contains(` call.
    for marker in &["assert.Contains(", "require.Contains("] {
        let mut search_from = 0;
        while let Some(rel) = body[search_from..].find(marker) {
            let abs = search_from + rel;
            let tail = &body[abs..];
            // Cheap: look at the next 200 chars for `err.Error()`.
            let window_end = tail.len().min(200);
            if tail[..window_end].contains("err.Error()") {
                return true;
            }
            search_from = abs + marker.len();
        }
    }
    false
}

fn body_starts_with_skip(body: &str) -> bool {
    for raw_line in body.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        // Skip past common preamble lines: `ctx, cleanup := ...`,
        // `defer cleanup()`, `t.Helper()`, `t.Parallel()`.
        if line.starts_with("ctx")
            || line.starts_with("defer ")
            || line.starts_with("t.Helper(")
            || line.starts_with("t.Parallel(")
            || line.starts_with("//")
            || line.starts_with("var ")
        {
            continue;
        }
        return SKIP_PREFIXES.iter().any(|p| line.starts_with(p));
    }
    false
}

fn body_has_any_assertion_marker(body: &str) -> bool {
    // Any `assert.` / `require.` / `t.Error` call counts as "some
    // assertion exists, just not in our catalog". Used to defuse
    // false positives on custom assertion helpers — we only fire
    // when the body is GENUINELY empty.
    body.contains("assert.") || body.contains("require.") || body.contains("t.Error")
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn mk_file(path: &str, source: &str) -> GoHandlerSourceFile {
        GoHandlerSourceFile {
            feature_name: "account".to_owned(),
            bucket: "handlers".to_owned(),
            relative_path: PathBuf::from(path),
            absolute_path: PathBuf::from(format!("/abs/{}", path)),
            source: source.to_owned(),
            loc_count: source.lines().count(),
            is_test: path.ends_with("_test.go"),
        }
    }

    #[test]
    fn positive_only_require_error_fires() {
        let src = "package h\n\nfunc TestFoo(t *testing.T) {\n  require.Error(t, err)\n}\n";
        let f = mk_file("features/account/handlers/foo_test.go", src);
        let findings = check(&[f]);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].body_kinds[0].0, "TestFoo");
        assert_eq!(findings[0].body_kinds[0].1, TestBodyKind::ErrorOnly);
        assert_eq!(Finding::CODE, "TEST-FAILURE-ONLY-COVERAGE-001");
    }

    #[test]
    fn positive_skip_fires() {
        let src = "package h\n\nfunc TestFoo(t *testing.T) {\n  t.Skip(\"needs creds\")\n}\n";
        let f = mk_file("features/account/handlers/foo_test.go", src);
        let findings = check(&[f]);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].body_kinds[0].1, TestBodyKind::Skip);
    }

    #[test]
    fn positive_pilot_incident_replay_fires() {
        // Verbatim shape of the 2026-05-27 hostpoint incident: one
        // ErrorOnly body + one Skip body.
        let src = r#"package h

func TestRegisterWithGoogle_ReturnsNotImplementedStub(t *testing.T) {
    ctx, cleanup := testsupport.Setup(t)
    defer cleanup()
    _, err := RegisterWithGoogle(ctx, gen.Input{IDToken: "any"})
    require.Error(t, err)
    assert.Contains(t, err.Error(), "not implemented")
}

func TestRegisterWithGoogle_RealGoogleOIDCFlow(t *testing.T) {
    t.Skip("requires Google OIDC credentials")
}
"#;
        let f = mk_file(
            "features/account/handlers/register_with_google_test.go",
            src,
        );
        let findings = check(&[f]);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].body_kinds.len(), 2);
        let kinds: Vec<_> = findings[0].body_kinds.iter().map(|(_, k)| *k).collect();
        assert!(kinds.contains(&TestBodyKind::ErrorOnly));
        assert!(kinds.contains(&TestBodyKind::Skip));
        let msg = findings[0].message();
        assert!(msg.contains("TestRegisterWithGoogle_ReturnsNotImplementedStub"));
        assert!(msg.contains("TestRegisterWithGoogle_RealGoogleOIDCFlow"));
        assert!(msg.contains("ErrorOnly"));
        assert!(msg.contains("Skip"));
    }

    #[test]
    fn positive_empty_body_fires() {
        let src = "package h\n\nfunc TestFoo(t *testing.T) {}\n";
        let f = mk_file("features/account/handlers/foo_test.go", src);
        let findings = check(&[f]);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].body_kinds[0].1, TestBodyKind::Empty);
    }

    #[test]
    fn negative_success_assertion_silent() {
        let src = r#"package h

func TestFoo(t *testing.T) {
    result, err := DoIt(ctx)
    require.NoError(t, err)
    assert.Equal(t, "ok", result)
}
"#;
        let f = mk_file("features/account/handlers/foo_test.go", src);
        assert!(check(&[f]).is_empty());
    }

    #[test]
    fn negative_mixed_success_and_failure_silent() {
        // File has a Success body AND an ErrorOnly body — mixed
        // coverage is the healthy shape; rule does not fire.
        let src = r#"package h

func TestFoo_Success(t *testing.T) {
    result, err := DoIt(ctx)
    require.NoError(t, err)
    assert.NotEmpty(t, result)
}

func TestFoo_RejectsBad(t *testing.T) {
    _, err := DoIt(ctx)
    require.Error(t, err)
}
"#;
        let f = mk_file("features/account/handlers/foo_test.go", src);
        assert!(check(&[f]).is_empty());
    }

    #[test]
    fn negative_reject_suffix_silent() {
        let src = "package h\n\nfunc TestFoo(t *testing.T) {\n  require.Error(t, err)\n}\n";
        let f = mk_file("features/account/handlers/validate_reject_test.go", src);
        assert!(check(&[f]).is_empty());
    }

    #[test]
    fn negative_invalid_suffix_silent() {
        let src = "package h\n\nfunc TestFoo(t *testing.T) {\n  require.Error(t, err)\n}\n";
        let f = mk_file("features/account/handlers/parse_invalid_test.go", src);
        assert!(check(&[f]).is_empty());
    }

    #[test]
    fn negative_staging_path_silent() {
        let src = "package h\n\nfunc TestFoo(t *testing.T) {\n  require.Error(t, err)\n}\n";
        let f = mk_file("features/account/handlers/staging/foo_test.go", src);
        assert!(check(&[f]).is_empty());
    }

    #[test]
    fn negative_file_allow_comment_silences() {
        let src = r#"# doctor:allow TEST-FAILURE-ONLY-COVERAGE-001 — reason "covered in e2e"
package h

func TestFoo(t *testing.T) {
    require.Error(t, err)
}
"#;
        let f = mk_file("features/account/handlers/foo_test.go", src);
        assert!(check(&[f]).is_empty());
    }

    #[test]
    fn negative_no_test_functions_silent() {
        // Helper-only file (no `func Test*`). Out of scope.
        let src = "package h\n\nfunc setUp(t *testing.T) {}\n";
        let f = mk_file("features/account/handlers/helpers_test.go", src);
        assert!(check(&[f]).is_empty());
    }

    #[test]
    fn negative_assert_notempty_on_err_does_not_count_as_success() {
        // `assert.NotEmpty(t, err.Error())` is an error-side
        // assertion in disguise — must NOT promote to success.
        let src = r#"package h

func TestFoo(t *testing.T) {
    _, err := DoIt(ctx)
    require.Error(t, err)
    assert.NotEmpty(t, err.Error())
}
"#;
        let f = mk_file("features/account/handlers/foo_test.go", src);
        let findings = check(&[f]);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].body_kinds[0].1, TestBodyKind::ErrorOnly);
    }

    #[test]
    fn negative_non_test_go_file_silent() {
        // The walker yields handler `.go` files too; the rule must
        // only consider `*_test.go`.
        let src = "package h\n\nfunc Foo() {}\n";
        let f = mk_file("features/account/handlers/foo.go", src);
        // Manually clear is_test (the helper sets it via filename).
        let mut f = f;
        f.is_test = false;
        assert!(check(&[f]).is_empty());
    }

    #[test]
    fn message_lists_kinds_per_function() {
        let src = r#"package h

func TestA(t *testing.T) {
    require.Error(t, err)
}

func TestB(t *testing.T) {
    t.Skip("nope")
}
"#;
        let f = mk_file("features/account/handlers/foo_test.go", src);
        let findings = check(&[f]);
        let msg = findings[0].message();
        assert!(msg.contains("TestA: ErrorOnly"));
        assert!(msg.contains("TestB: Skip"));
        assert!(msg.contains("doctor:allow"));
    }
}
