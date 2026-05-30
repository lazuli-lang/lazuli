
    use super::*;

    #[test]
    fn tier_parses_case_insensitively() {
        assert_eq!(Tier::parse("gold"), Some(Tier::Gold));
        assert_eq!(Tier::parse("  GOLD "), Some(Tier::Gold));
        assert_eq!(Tier::parse("Draft"), Some(Tier::Draft));
        assert_eq!(Tier::parse("deprecated"), Some(Tier::Deprecated));
        assert_eq!(Tier::parse("approved"), Some(Tier::Approved));
        assert_eq!(Tier::parse("bogus"), None);
    }

    #[test]
    fn frontmatter_scalar_keys_parse() {
        let md = "---\ntier: gold\nrevalidate_by: 2025-01-01\n---\nbody\n";
        let doc = parse_doc(Path::new("0001-charge.md"), "0001-charge", md);
        assert_eq!(doc.tier, Some(Tier::Gold));
        assert_eq!(doc.revalidate_by.as_deref(), Some("2025-01-01"));
        assert_eq!(doc.topic_slug, "charge");
        assert!(doc.is_gold());
    }

    #[test]
    fn cites_inline_list_parses() {
        let md = "---\ntier: gold\ncites: [billing.charge, billing.Invoice]\n---\n";
        let doc = parse_doc(Path::new("0001-x.md"), "0001-x", md);
        assert_eq!(doc.cites, vec!["billing.charge", "billing.Invoice"]);
    }

    #[test]
    fn cites_block_list_parses() {
        let md = "---\ntier: gold\ncites:\n  - billing.charge\n  - billing.Invoice\ntags: [a]\n---\n";
        let doc = parse_doc(Path::new("0001-x.md"), "0001-x", md);
        assert_eq!(doc.cites, vec!["billing.charge", "billing.Invoice"]);
    }

    #[test]
    fn supersession_relation_keys_detected() {
        for key in ["supersedes", "replaces", "deprecated_by", "deprecated"] {
            let md = format!("---\ntier: gold\n{key}: 0001-old\n---\n");
            let doc = parse_doc(Path::new("0002-new.md"), "0002-new", &md);
            assert!(doc.has_supersession, "key `{key}` should set supersession");
        }
    }

    #[test]
    fn empty_supersession_value_is_not_a_relation() {
        let md = "---\ntier: gold\nsupersedes:\n---\n";
        let doc = parse_doc(Path::new("0001-x.md"), "0001-x", md);
        assert!(!doc.has_supersession);
    }

    #[test]
    fn no_frontmatter_yields_empty_doc() {
        let doc = parse_doc(Path::new("0001-x.md"), "0001-x", "just body, no fence\n");
        assert_eq!(doc.tier, None);
        assert!(doc.cites.is_empty());
        assert!(doc.revalidate_by.is_none());
    }

    #[test]
    fn slug_strips_numeric_ordinal() {
        assert_eq!(slug_from_stem("0007-charge-flow"), "charge-flow");
        assert_eq!(slug_from_stem("12-topic"), "topic");
        assert_eq!(slug_from_stem("nonumber"), "nonumber");
        assert_eq!(slug_from_stem("ABC-Mixed"), "abc-mixed"); // non-numeric head kept
    }

    #[test]
    fn scan_sector_reads_md_only_and_sorts() {
        let dir = tempfile::tempdir().expect("tmp");
        let root = dir.path();
        let sector = sector_dir(root, "billing");
        std::fs::create_dir_all(&sector).expect("mkdir");
        std::fs::write(sector.join("0002-b.md"), "---\ntier: draft\n---\n").unwrap();
        std::fs::write(sector.join("0001-a.md"), "---\ntier: gold\n---\n").unwrap();
        std::fs::write(sector.join("notes.txt"), "ignore me").unwrap();
        let docs = scan_sector(root, "billing");
        assert_eq!(docs.len(), 2, "only .md files counted");
        // sorted by path => 0001 first
        assert!(docs[0].path.ends_with("0001-a.md"));
        assert_eq!(docs[0].tier, Some(Tier::Gold));
        assert_eq!(docs[1].tier, Some(Tier::Draft));
    }

    #[test]
    fn scan_missing_sector_is_empty_not_error() {
        let dir = tempfile::tempdir().expect("tmp");
        assert!(scan_sector(dir.path(), "absent").is_empty());
    }

    #[test]
    fn sector_exists_predicate() {
        let dir = tempfile::tempdir().expect("tmp");
        assert!(!sector_exists(dir.path(), "billing"));
        std::fs::create_dir_all(sector_dir(dir.path(), "billing")).unwrap();
        assert!(sector_exists(dir.path(), "billing"));
    }

    #[test]
    fn git_probe_returns_none_outside_repo() {
        // A bare temp dir is not a git repo: probes must degrade to None
        // (skip) rather than panic or fire.
        let dir = tempfile::tempdir().expect("tmp");
        let f = dir.path().join("knowledge/billing/0001-x.md");
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(&f, "---\ntier: gold\n---\n").unwrap();
        assert_eq!(git_commit_count(dir.path(), &f), None);
        assert_eq!(passed_through_draft(dir.path(), &f), None);
    }
