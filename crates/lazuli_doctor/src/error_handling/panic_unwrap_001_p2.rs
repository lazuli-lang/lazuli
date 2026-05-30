/// Replace every `"..."` and raw-string body on a single line with spaces
/// so substring-detection ignores string-literal content. This is a
/// pragmatic line-local heuristic — multi-line strings (continuation `\`
/// or unclosed raw strings) aren't tracked here. The caller's `scan_file`
/// state machine already silences entire `#[cfg(test)] mod tests`
/// blocks, which is where multi-line fixture strings typically live.
fn strip_string_literals(line: &str) -> String {
    let bytes = line.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        // Detect raw string start: `r"..."` or `r#"..."#` (one or more `#`).
        if c == b'r' && i + 1 < bytes.len() && (bytes[i + 1] == b'"' || bytes[i + 1] == b'#') {
            let mut j = i + 1;
            let mut hash_count = 0;
            while j < bytes.len() && bytes[j] == b'#' {
                hash_count += 1;
                j += 1;
            }
            if j < bytes.len() && bytes[j] == b'"' {
                // Skip the body; replace with spaces up to closing
                // `"` followed by hash_count `#`s.
                out.extend(std::iter::repeat_n(b' ', j - i + 1));
                let mut k = j + 1;
                while k < bytes.len() {
                    if bytes[k] == b'"' {
                        let mut h = 0;
                        while h < hash_count && k + 1 + h < bytes.len() && bytes[k + 1 + h] == b'#'
                        {
                            h += 1;
                        }
                        if h == hash_count {
                            out.extend(std::iter::repeat_n(b' ', 1 + hash_count));
                            k += 1 + hash_count;
                            break;
                        }
                    }
                    out.push(b' ');
                    k += 1;
                }
                i = k;
                continue;
            }
        }
        // Regular string literal: `"..."` with backslash escapes.
        if c == b'"' {
            out.push(b' ');
            let mut j = i + 1;
            while j < bytes.len() {
                if bytes[j] == b'\\' && j + 1 < bytes.len() {
                    out.push(b' ');
                    out.push(b' ');
                    j += 2;
                    continue;
                }
                if bytes[j] == b'"' {
                    out.push(b' ');
                    j += 1;
                    break;
                }
                out.push(b' ');
                j += 1;
            }
            i = j;
            continue;
        }
        out.push(c);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| line.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(source: &str) -> RustSourceFile {
        RustSourceFile {
            crate_name: "lazuli_test".to_owned(),
            relative_path: PathBuf::from("crates/lazuli_test/src/lib.rs"),
            absolute_path: PathBuf::from("/abs/crates/lazuli_test/src/lib.rs"),
            source: source.to_owned(),
            loc_count: source.lines().count(),
            is_library_src: true,
        }
    }

    #[test]
    fn bare_unwrap_fires() {
        let f = file("pub fn x() { let r: Result<u32, ()> = Ok(0); r.unwrap(); }\n");
        let findings = check(&[f]);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].construct, ".unwrap()");
    }

    #[test]
    fn unwrap_or_default_does_not_fire() {
        let f = file(
            "pub fn x() {\n  let r: Result<u32, ()> = Ok(0);\n  let v = r.unwrap_or_default();\n}\n",
        );
        assert!(
            check(&[f]).is_empty(),
            "unwrap_or_default is not panic-prone"
        );
    }

    #[test]
    fn unwrap_or_else_does_not_fire() {
        let f = file("pub fn x(r: Result<u32, ()>) -> u32 { r.unwrap_or_else(|_| 0) }\n");
        assert!(check(&[f]).is_empty());
    }

    #[test]
    fn expect_fires() {
        let f = file(
            r#"pub fn x() { let r: Result<u32, ()> = Ok(0); r.expect("oops"); }
"#,
        );
        let findings = check(&[f]);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].construct, ".expect(...)");
    }

    #[test]
    fn expect_err_does_not_fire() {
        let f = file("pub fn x() { let r: Result<u32, ()> = Err(()); r.expect_err(\"ok\"); }\n");
        assert!(check(&[f]).is_empty());
    }

    #[test]
    fn panic_macro_fires() {
        let f = file("pub fn x() { panic!(\"never\"); }\n");
        let findings = check(&[f]);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].construct, "panic!(...)");
    }

    #[test]
    fn todo_unimplemented_unreachable_fire() {
        let f = file(
            "pub fn a() { todo!() }\npub fn b() { unimplemented!() }\npub fn c() { unreachable!() }\n",
        );
        let findings = check(&[f]);
        assert_eq!(findings.len(), 3);
    }

    #[test]
    fn unwrap_inside_cfg_test_mod_is_silent() {
        let f = file(
            "pub fn x() {}\n#[cfg(test)]\nmod tests {\n  use super::*;\n  #[test]\n  fn t() { Some(1u32).unwrap(); }\n}\n",
        );
        assert!(
            check(&[f]).is_empty(),
            "unwrap inside #[cfg(test)] mod is allowed"
        );
    }

    #[test]
    fn unwrap_in_test_function_outside_module_still_fires() {
        // `#[test] fn t() { ... }` at crate root (no enclosing
        // #[cfg(test)] mod) — strictly speaking the body still ships.
        // Test infrastructure relies on `mod tests` discipline; freestanding
        // #[test] fns are unusual and still scanned.
        let f = file("#[test]\nfn t() { Some(1u32).unwrap(); }\n");
        let findings = check(&[f]);
        assert_eq!(findings.len(), 1);
    }

    #[test]
    fn line_comments_with_unwrap_do_not_fire() {
        let f = file("pub fn x() { /* nothing */ }\n// example: x.unwrap();\n");
        assert!(check(&[f]).is_empty());
    }

    #[test]
    fn rustdoc_lines_with_unwrap_do_not_fire() {
        let f = file("/// Example: `value.unwrap()` in a test.\npub fn x() {}\n");
        assert!(check(&[f]).is_empty());
    }

    #[test]
    fn const_decls_with_panic_keyword_do_not_fire() {
        let f =
            file("pub const PANIC_DOC: &str = \"calls panic!(...) on failure\";\npub fn x() {}\n");
        assert!(check(&[f]).is_empty());
    }

    #[test]
    fn non_library_files_are_skipped() {
        let mut f = file("pub fn x() { panic!() }\n");
        f.is_library_src = false;
        assert!(check(&[f]).is_empty());
    }

    #[test]
    fn rails_test_sibling_files_are_skipped() {
        // Files named `<name>_tests.rs` are Rails-style sibling test
        // modules included from a parent via `include!()`. The rule's
        // `#[cfg(test)] mod` depth tracking can't see the parent, so we
        // skip these whole files.
        let mut f = file("pub fn x() { panic!() }\n");
        f.relative_path = PathBuf::from("crates/lazuli_test/src/foo_tests.rs");
        assert!(check(&[f]).is_empty());
    }

    #[test]
    fn files_under_src_tests_are_skipped() {
        // Rails-style: tests live under `src/tests/...` next to the
        // module they exercise. Whole sub-tree is test code.
        let mut f = file("pub fn x() { panic!() }\n");
        f.relative_path = PathBuf::from("crates/lazuli_test/src/tests/core/workflow.rs");
        assert!(check(&[f]).is_empty());
    }

    #[test]
    fn plain_src_file_still_fires() {
        // Regression guard: `tests` substring in a file path that isn't
        // a sibling test file or under `src/tests/` should still fire.
        let mut f = file("pub fn x() { panic!() }\n");
        f.relative_path = PathBuf::from("crates/lazuli_test/src/contests/mod.rs");
        assert_eq!(check(&[f]).len(), 1);
    }

    #[test]
    fn message_includes_construct_and_path() {
        let f = file("pub fn x() { panic!(\"\") }\n");
        let finding = check(&[f]).into_iter().next().unwrap();
        let msg = finding.message();
        assert!(msg.contains("panic!"));
        assert!(msg.contains("lib.rs"));
        assert!(msg.contains(":1"));
    }
}
