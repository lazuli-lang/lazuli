
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
    fn bare_variant_fires() {
        let src: String =
            "#[derive(Debug, thiserror::Error)]\npub enum ParseError {\n    Bad,\n}\n".to_string();
        let findings = check(&[file(&src)]);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].variant_name, "Bad");
        assert_eq!(findings[0].enum_name, "ParseError");
    }

    #[test]
    fn variant_with_doc_only_is_silent() {
        let src: String = "#[derive(Debug, thiserror::Error)]\npub enum ParseError {\n    /// Something bad happened.\n    Bad,\n}\n".to_string();
        assert!(check(&[file(&src)]).is_empty());
    }

    #[test]
    fn variant_with_error_attr_only_is_silent() {
        let src: String = "#[derive(Debug, thiserror::Error)]\npub enum ParseError {\n    #[error(\"something bad\")]\n    Bad,\n}\n".to_string();
        assert!(check(&[file(&src)]).is_empty());
    }

    #[test]
    fn variant_with_both_doc_and_error_attr_is_silent() {
        let src: String = "#[derive(Debug, thiserror::Error)]\npub enum ParseError {\n    /// A bad thing.\n    #[error(\"bad\")]\n    Bad,\n}\n".to_string();
        assert!(check(&[file(&src)]).is_empty());
    }

    #[test]
    fn multiple_variants_independent() {
        let src: String = "#[derive(Debug, thiserror::Error)]\npub enum ParseError {\n    /// First — has doc.\n    First,\n    Second,\n    #[error(\"third\")]\n    Third,\n    Fourth,\n}\n".to_string();
        let findings = check(&[file(&src)]);
        assert_eq!(findings.len(), 2);
        let names: Vec<_> = findings.iter().map(|f| f.variant_name.as_str()).collect();
        assert!(names.contains(&"Second"));
        assert!(names.contains(&"Fourth"));
    }

    #[test]
    fn tuple_and_struct_shaped_variants_are_recognized() {
        let src: String = "#[derive(Debug, thiserror::Error)]\npub enum ApiError {\n    Tuple(String),\n    Struct { code: u16 },\n}\n".to_string();
        let findings = check(&[file(&src)]);
        assert_eq!(findings.len(), 2);
    }

    #[test]
    fn non_error_enum_does_not_fire() {
        let src: String =
            "#[derive(Debug, Clone)]\npub enum Status {\n    Open,\n    Closed,\n}\n".to_string();
        assert!(check(&[file(&src)]).is_empty());
    }

    #[test]
    fn private_enum_does_not_fire() {
        let src: String =
            "#[derive(Debug, thiserror::Error)]\nenum Internal {\n    Bad,\n}\n".to_string();
        assert!(check(&[file(&src)]).is_empty());
    }

    #[test]
    fn non_library_files_are_skipped() {
        let src: String =
            "#[derive(Debug, thiserror::Error)]\npub enum ParseError {\n    Bad,\n}\n".to_string();
        let mut f = file(&src);
        f.is_library_src = false;
        assert!(check(&[f]).is_empty());
    }

    #[test]
    fn struct_variant_with_doc_silences() {
        let src: String = "#[derive(Debug, thiserror::Error)]\npub enum ApiError {\n    /// Code-bearing failure.\n    Struct { code: u16 },\n}\n".to_string();
        assert!(check(&[file(&src)]).is_empty());
    }

    #[test]
    fn message_includes_variant_and_enum() {
        let src: String =
            "#[derive(Debug, thiserror::Error)]\npub enum ParseError {\n    Bad,\n}\n".to_string();
        let finding = check(&[file(&src)]).into_iter().next().unwrap();
        let msg = finding.message();
        assert!(msg.contains("ParseError"));
        assert!(msg.contains("Bad"));
        assert!(msg.contains("lib.rs"));
    }

    #[test]
    fn two_enums_in_one_file_independent() {
        let src: String = "#[derive(Debug, thiserror::Error)]\npub enum First {\n    Bad,\n}\n\n#[derive(Debug, Clone)]\npub enum Second {\n    Open,\n}\n".to_string();
        let findings = check(&[file(&src)]);
        // Only the first enum is in scope; second isn't derive-Error.
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].enum_name, "First");
    }

    #[test]
    fn non_exhaustive_variant_attr_silences_via_doc() {
        // Variants can carry their own `#[non_exhaustive]` — verify it
        // doesn't confuse the back-walk (doc still silences).
        let src: String = "#[derive(Debug, thiserror::Error)]\npub enum ParseError {\n    /// A bad thing.\n    #[non_exhaustive]\n    Bad,\n}\n".to_string();
        assert!(check(&[file(&src)]).is_empty());
    }

    #[test]
    fn variant_with_only_non_exhaustive_still_fires() {
        // Variant has `#[non_exhaustive]` but no doc and no
        // `#[error(...)]` — still opaque to logs and rustdoc.
        let src: String = "#[derive(Debug, thiserror::Error)]\npub enum ParseError {\n    #[non_exhaustive]\n    Bad,\n}\n".to_string();
        let findings = check(&[file(&src)]);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].variant_name, "Bad");
    }
