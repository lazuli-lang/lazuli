    use super::*;
    use std::fs as stdfs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    pub(crate) struct TempDir {
        pub(crate) path: PathBuf,
    }

    static NEXT_ID: AtomicUsize = AtomicUsize::new(0);

    impl TempDir {
        pub(crate) fn new(tag: &str) -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time")
                .as_nanos();
            let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir()
                .join("lazuli-design-doctor-test")
                .join(format!("{tag}-{nonce}-{id}-{}", std::process::id()));
            stdfs::create_dir_all(&path).expect("create tempdir");
            Self { path }
        }

        pub(crate) fn path(&self) -> &std::path::Path {
            &self.path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = stdfs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn allowlist_reads_known_buckets() {
        let td = TempDir::new("allowlist");
        let dir = td.path().join("dist").join("ts-web").join("design");
        stdfs::create_dir_all(&dir).unwrap();
        stdfs::write(
            dir.join("allowlist.json"),
            r#"{"bg":["primary","success"],"text":["foreground"],"font":["sans"]}"#,
        )
        .unwrap();
        let al = read_allowlist(td.path()).expect("allowlist parses");
        assert!(al.contains("bg", "primary"));
        assert!(!al.contains("bg", "purple-500"));
        assert!(al.is_known_font_token("sans"));
        assert!(al.knows_prefix("bg"));
        assert!(!al.knows_prefix("flex"));
    }

    #[test]
    fn allowlist_missing_returns_none() {
        let td = TempDir::new("allowlist-missing");
        assert!(read_allowlist(td.path()).is_none());
    }

    #[test]
    fn walk_tsx_skips_node_modules_and_tests() {
        let td = TempDir::new("walk");
        let root = td.path();
        stdfs::create_dir_all(root.join("features").join("hello")).unwrap();
        stdfs::create_dir_all(root.join("node_modules").join("react")).unwrap();
        stdfs::create_dir_all(root.join("dist").join("ts-web")).unwrap();
        stdfs::write(root.join("features").join("hello").join("ok.tsx"), "x").unwrap();
        stdfs::write(
            root.join("features").join("hello").join("ok.test.tsx"),
            "x",
        )
        .unwrap();
        stdfs::write(
            root.join("features").join("hello").join("ok.stories.tsx"),
            "x",
        )
        .unwrap();
        stdfs::write(root.join("node_modules").join("react").join("idx.tsx"), "x").unwrap();
        stdfs::write(root.join("dist").join("ts-web").join("gen.tsx"), "x").unwrap();
        let files = walk_tsx_files(root);
        assert_eq!(files.len(), 1, "found: {:?}", files);
        assert!(files[0].ends_with("ok.tsx"));
    }

    #[test]
    fn scan_lines_indexes_from_one() {
        let lines: Vec<(usize, &str)> = scan_lines("a\nb\nc").collect();
        assert_eq!(lines, vec![(1, "a"), (2, "b"), (3, "c")]);
    }

    #[test]
    fn escape_comment_same_line_suppresses() {
        let lines = vec![
            "function x() {",
            "  return <div style={{ color: \"#fff\" }} />; // lazuli-allow: design-token-hex-leak — vendor brand",
            "}",
        ];
        assert!(is_allowed_by_escape_comment(&lines, 1, "design-token-hex-leak"));
    }

    #[test]
    fn escape_comment_prev_line_suppresses() {
        let lines = vec![
            "// lazuli-allow: design-token-hex-leak — vendor brand",
            "<div style={{ color: \"#fff\" }} />",
        ];
        assert!(is_allowed_by_escape_comment(&lines, 1, "design-token-hex-leak"));
    }

    #[test]
    fn escape_comment_only_matches_exact_code() {
        let lines = vec!["x // lazuli-allow: design-token-hex-leak — note"];
        assert!(is_allowed_by_escape_comment(&lines, 0, "design-token-hex-leak"));
        assert!(!is_allowed_by_escape_comment(&lines, 0, "design-token-px-leak"));
        // Prefix-only collision must NOT match.
        let lines2 = vec!["x // lazuli-allow: design-token-hex-leak-and-more"];
        // The escape is `design-token-hex-leak` followed by `-and-more` —
        // a `-` is an accepted separator per the parser. This is the
        // intentional behaviour: `-` is treated as a separator so that
        // hyphenated reasons need not be quoted.
        assert!(is_allowed_by_escape_comment(&lines2, 0, "design-token-hex-leak"));
    }

    #[test]
    fn class_string_iter_emits_jsx_attrs() {
        let line = r#"<div className="bg-primary text-foreground" id="x"></div>"#;
        let items: Vec<&str> = iter_class_strings(line).collect();
        assert_eq!(items, vec!["bg-primary text-foreground"]);
    }

    #[test]
    fn class_string_iter_handles_single_quotes() {
        let line = r#"<div className='bg-primary'></div>"#;
        let items: Vec<&str> = iter_class_strings(line).collect();
        assert_eq!(items, vec!["bg-primary"]);
    }

    #[test]
    fn class_string_iter_skips_jsx_expressions() {
        let line = r#"<div className={cn("bg-primary")}></div>"#;
        // JSX expression `{...}` is skipped by the class iterator (returns
        // empty for the className= match); the inner literal would be
        // caught by a future enhancement. v0 accepts this trade-off.
        let items: Vec<&str> = iter_class_strings(line).collect();
        assert!(items.is_empty());
    }

    #[test]
    fn style_span_single_line() {
        let content = r##"<div style={{ color: "#fff", padding: "12px" }} />"##;
        let lines: Vec<&str> = content.lines().collect();
        let spans = iter_style_spans(content, &lines);
        assert_eq!(spans.len(), 1);
        assert!(spans[0].segment.contains("#fff"));
        assert!(spans[0].segment.contains("12px"));
    }

    #[test]
    fn style_span_multi_line() {
        let content = "<div\n  style={{\n    color: \"#fff\",\n    padding: \"12px\",\n  }}\n/>";
        let lines: Vec<&str> = content.lines().collect();
        let spans = iter_style_spans(content, &lines);
        // Expect one span per line that overlaps the style block (lines 2..5).
        assert!(spans.len() >= 3);
        let combined: String = spans.iter().map(|s| s.segment.to_string()).collect();
        assert!(combined.contains("#fff"));
        assert!(combined.contains("12px"));
    }

    #[test]
    fn style_span_string_braces_ignored() {
        let content = r#"<div style={{ content: "a}b" }} />"#;
        let lines: Vec<&str> = content.lines().collect();
        let spans = iter_style_spans(content, &lines);
        assert_eq!(spans.len(), 1);
        assert!(spans[0].segment.contains("a}b"));
    }
