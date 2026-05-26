    use super::*;
    use std::collections::HashMap;

    fn al(pairs: &[(&str, &[&str])]) -> Allowlist {
        let mut buckets = HashMap::new();
        for (k, vs) in pairs {
            buckets.insert((*k).to_string(), vs.iter().map(|s| s.to_string()).collect());
        }
        Allowlist { buckets }
    }

    #[test]
    fn trigger_undeclared_color() {
        let lines = vec![r#"<div className="bg-purple-500" />"#];
        let allowlist = al(&[("bg", &["primary", "success"])]);
        let f = check_file(Path::new("x.tsx"), &lines, &allowlist);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].prefix, "bg");
        assert_eq!(f[0].suffix, "purple-500");
    }

    #[test]
    fn allow_declared_token() {
        let lines = vec![r#"<div className="bg-primary text-primary-foreground" />"#];
        let allowlist = al(&[
            ("bg", &["primary"]),
            ("text", &["primary-foreground"]),
        ]);
        let f = check_file(Path::new("x.tsx"), &lines, &allowlist);
        assert!(f.is_empty(), "found unexpected: {:?}", f);
    }

    #[test]
    fn escape_comment_suppresses() {
        let lines = vec![
            "// lazuli-allow: design-token-undefined — third-party widget",
            r#"<div className="bg-purple-500" />"#,
        ];
        let allowlist = al(&[("bg", &["primary"])]);
        let f = check_file(Path::new("x.tsx"), &lines, &allowlist);
        assert!(f.is_empty(), "found: {:?}", f);
    }

    #[test]
    fn unknown_prefix_not_checked() {
        // `flex` / `items-center` are not design-token bound — Doctor
        // doesn't own them. No allowlist bucket = no finding.
        let lines = vec![r#"<div className="flex items-center justify-between" />"#];
        let allowlist = al(&[("bg", &["primary"])]);
        let f = check_file(Path::new("x.tsx"), &lines, &allowlist);
        assert!(f.is_empty());
    }

    #[test]
    fn variant_prefix_stripped_before_lookup() {
        let lines = vec![r#"<div className="hover:bg-primary md:dark:bg-success" />"#];
        let allowlist = al(&[("bg", &["primary", "success"])]);
        let f = check_file(Path::new("x.tsx"), &lines, &allowlist);
        assert!(f.is_empty(), "variants must strip cleanly; found: {:?}", f);
    }

    #[test]
    fn arbitrary_value_not_flagged_here() {
        // `bg-[#fff]` is the hex-leak rule's concern, not this one.
        let lines = vec![r#"<div className="bg-[#fff]" />"#];
        let allowlist = al(&[("bg", &["primary"])]);
        let f = check_file(Path::new("x.tsx"), &lines, &allowlist);
        assert!(f.is_empty());
    }

    #[test]
    fn bare_default_token_resolves() {
        let lines = vec![r#"<div className="rounded" />"#];
        let allowlist = al(&[("rounded", &["DEFAULT", "md", "lg"])]);
        let f = check_file(Path::new("x.tsx"), &lines, &allowlist);
        assert!(f.is_empty(), "rounded → DEFAULT slot lookup; found: {:?}", f);
    }

    #[test]
    fn bare_default_token_missing_fires() {
        let lines = vec![r#"<div className="rounded" />"#];
        let allowlist = al(&[("rounded", &["md", "lg"])]); // no DEFAULT
        let f = check_file(Path::new("x.tsx"), &lines, &allowlist);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].suffix, "DEFAULT");
    }

    #[test]
    fn important_modifier_stripped() {
        let lines = vec![r#"<div className="!bg-primary" />"#];
        let allowlist = al(&[("bg", &["primary"])]);
        let f = check_file(Path::new("x.tsx"), &lines, &allowlist);
        assert!(f.is_empty());
    }

    // ── BB.4 — `text-` prefix ambiguity (color vs typography scale) ───────────
    //
    // The `text-` prefix has dual meaning in Tailwind: `text-<color>` (color
    // bucket) and `text-<size>` (typography scale bucket). The doctor probes
    // BOTH the `text` bucket (color) and the `text-size` bucket (scale); the
    // diagnostic fires only when NEITHER contains the suffix.

    #[test]
    fn text_size_class_resolves_via_scale_bucket() {
        // `text-xs` with the scale bucket containing `xs` → no diagnostic,
        // even when the color bucket has no `xs` entry.
        let lines = vec![r#"<div className="text-xs" />"#];
        let allowlist = al(&[
            ("text", &["primary", "foreground"]),
            ("text-size", &["xs", "sm", "base", "lg", "2xl"]),
        ]);
        let f = check_file(Path::new("x.tsx"), &lines, &allowlist);
        assert!(f.is_empty(), "text-xs should resolve via text-size; found: {:?}", f);
    }

    #[test]
    fn text_color_class_resolves_via_color_bucket() {
        // `text-primary` with the color bucket containing `primary` → no
        // diagnostic, even when the scale bucket has no `primary` entry.
        let lines = vec![r#"<div className="text-primary" />"#];
        let allowlist = al(&[
            ("text", &["primary", "foreground"]),
            ("text-size", &["xs", "base"]),
        ]);
        let f = check_file(Path::new("x.tsx"), &lines, &allowlist);
        assert!(f.is_empty(), "text-primary should resolve via text; found: {:?}", f);
    }

    #[test]
    fn text_class_fires_when_in_neither_bucket() {
        // `text-asdf` not in either bucket → diagnostic fires.
        let lines = vec![r#"<div className="text-asdf" />"#];
        let allowlist = al(&[
            ("text", &["primary"]),
            ("text-size", &["xs", "base"]),
        ]);
        let f = check_file(Path::new("x.tsx"), &lines, &allowlist);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].prefix, "text");
        assert_eq!(f[0].suffix, "asdf");
    }

    #[test]
    fn text_class_resolves_when_in_only_color_bucket() {
        // Scale bucket missing entirely → fall back to color bucket only.
        let lines = vec![r#"<div className="text-primary" />"#];
        let allowlist = al(&[("text", &["primary"])]);
        let f = check_file(Path::new("x.tsx"), &lines, &allowlist);
        assert!(f.is_empty(), "found: {:?}", f);
    }

    #[test]
    fn text_class_resolves_when_in_only_scale_bucket() {
        // Color bucket missing entirely → fall back to scale bucket only.
        let lines = vec![r#"<div className="text-base" />"#];
        let allowlist = al(&[("text-size", &["xs", "base", "lg"])]);
        let f = check_file(Path::new("x.tsx"), &lines, &allowlist);
        assert!(f.is_empty(), "found: {:?}", f);
    }

    #[test]
    fn regression_existing_color_class_still_works() {
        // Non-`text-` prefix path must not regress under the bucket fan-out.
        let lines = vec![r#"<div className="bg-primary" />"#];
        let allowlist = al(&[("bg", &["primary", "success"])]);
        let f = check_file(Path::new("x.tsx"), &lines, &allowlist);
        assert!(f.is_empty(), "bg-primary regression; found: {:?}", f);
    }

    // ── CC.4 — opacity-slash modifier + ring/ring-offset routing ────────────
    //
    // Tailwind allows `bg-X/N` for alpha; `ring-N` is a width built-in;
    // `ring-offset-N` is the offset-width built-in; `ring-offset-<color>`
    // shares the `ring` color palette. These tests pin the doctor-side
    // routing so the final 5 hostpoint residuals resolve.

    #[test]
    fn opacity_slash_modifier_stripped_for_color_lookup() {
        let lines = vec![r#"<div className="bg-brand/90 bg-brand/5" />"#];
        let allowlist = al(&[("bg", &["brand"])]);
        let f = check_file(Path::new("x.tsx"), &lines, &allowlist);
        assert!(f.is_empty(), "opacity-slash on color must resolve; found: {:?}", f);
    }

    #[test]
    fn ring_width_builtin_not_flagged() {
        // `ring-N` digits AND `ring-inherit` are width built-ins, never
        // a color lookup — even when the `ring` color bucket exists.
        let lines = vec![r#"<div className="ring-2 ring-1 ring-4 ring-8 ring-inherit" />"#];
        let allowlist = al(&[("ring", &["primary", "background"])]);
        let f = check_file(Path::new("x.tsx"), &lines, &allowlist);
        assert!(f.is_empty(), "ring width built-ins must not fire; found: {:?}", f);
    }

    #[test]
    fn ring_offset_width_builtin_not_flagged() {
        let lines = vec![
            r#"<div className="ring-offset-0 ring-offset-2 ring-offset-4 ring-offset-8 ring-offset-inherit" />"#,
        ];
        let allowlist = al(&[("ring", &["primary", "background"])]);
        let f = check_file(Path::new("x.tsx"), &lines, &allowlist);
        assert!(f.is_empty(), "ring-offset width built-ins must not fire; found: {:?}", f);
    }

    #[test]
    fn ring_offset_color_resolves_via_color_bucket() {
        // `ring-offset-background` falls through to the `ring` color
        // bucket (Tailwind's ring-offset-color shares the project palette).
        let lines = vec![r#"<div className="ring-offset-background" />"#];
        let allowlist = al(&[("ring", &["primary", "background", "foreground"])]);
        let f = check_file(Path::new("x.tsx"), &lines, &allowlist);
        assert!(f.is_empty(), "ring-offset-background must resolve via ring bucket; found: {:?}", f);
    }

    #[test]
    fn ring_color_resolves_via_color_bucket() {
        let lines = vec![r#"<div className="ring-primary ring-ring" />"#];
        let allowlist = al(&[("ring", &["primary", "ring"])]);
        let f = check_file(Path::new("x.tsx"), &lines, &allowlist);
        assert!(f.is_empty(), "ring-<color> must resolve; found: {:?}", f);
    }

    #[test]
    fn unknown_ring_color_fires_diagnostic() {
        // Regression guard: the width/offset relaxation must NOT
        // over-allow arbitrary suffixes. Non-digit, non-`inherit`
        // suffixes still hit the color lookup.
        let lines = vec![r#"<div className="ring-undeclared" />"#];
        let allowlist = al(&[("ring", &["primary"])]);
        let f = check_file(Path::new("x.tsx"), &lines, &allowlist);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].prefix, "ring");
        assert_eq!(f[0].suffix, "undeclared");
    }

    #[test]
    fn regression_opacity_slash_on_text_class() {
        // `text-primary/50` must still resolve via the text color bucket.
        let lines = vec![r#"<div className="text-primary/50" />"#];
        let allowlist = al(&[("text", &["primary"]), ("text-size", &["base"])]);
        let f = check_file(Path::new("x.tsx"), &lines, &allowlist);
        assert!(f.is_empty(), "text-primary/50 must resolve; found: {:?}", f);
    }

    #[test]
    fn opacity_slash_explicit_extension_entry_honored() {
        // The allowlist-extension escape hatch (`allowlist.extension.json`)
        // lets projects declare literal `black/45`-style suffixes for
        // colors that aren't part of the design.lzi palette. The doctor
        // probes the full suffix BEFORE stripping, so these explicit
        // entries continue to allow the class even after the opacity
        // strip fallback was added.
        let lines = vec![r#"<div className="bg-black/45" />"#];
        let allowlist = al(&[("bg", &["primary", "black/45"])]);
        let f = check_file(Path::new("x.tsx"), &lines, &allowlist);
        assert!(f.is_empty(), "explicit black/45 entry must allow; found: {:?}", f);
    }

    #[test]
    fn test_file_skipped() {
        // Validated by walk_tsx_files unit tests; here we mainly assert
        // the rule API still works with a `.test.tsx` path passed manually.
        // (Doctor's main entrypoint goes through walk_tsx_files, which
        // already skips `.test.tsx`.)
        let lines = vec![r#"<div className="bg-purple-500" />"#];
        let allowlist = al(&[("bg", &["primary"])]);
        // The rule itself does not re-check filename — it's the walker's job.
        // We assert here that the rule remains pure (it fires when called).
        let f = check_file(Path::new("x.test.tsx"), &lines, &allowlist);
        assert_eq!(f.len(), 1);
    }

