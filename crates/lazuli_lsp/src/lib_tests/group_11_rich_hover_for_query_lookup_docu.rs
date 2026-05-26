//! Sub-file of the inline LSP test suite — see `mod.rs` for the
//! shared preamble and helpers. Tests are grouped by line-range
//! buckets only; each bucket is ≤ 500 LOC so `clippy` and
//! `rust-analyzer` stay responsive.
#![allow(unused_imports)]
use super::*;

#[test]
fn rich_hover_for_query_lookup_documents_single_key_and_composite_forms() {
    assert_rich_hover_contains(
        "query.lookup",
        &[
            "**`query.lookup`**",
            "by <field>: <Type>",
            "params",
            "key",
            "**Example**",
            "docs/quickref.md",
        ],
    );
}

#[test]
fn rich_hover_for_query_sql_requires_returns_and_sql_path() {
    assert_rich_hover_contains(
        "query.sql",
        &[
            "**`query.sql`**",
            "**Required children**",
            "returns",
            "sql \"./queries",
            "record",
            "**Example**",
            "docs/invariants.md",
        ],
    );
}

#[test]
fn rich_hover_for_query_view_requires_returns_and_file_source() {
    assert_rich_hover_contains(
        "query.view",
        &[
            "**`query.view`**",
            "**Required children**",
            "returns list of <Record>",
            "source @file.",
            "params",
            "**Example**",
            "docs/quickref.md",
        ],
    );
}

#[test]
fn rich_hover_for_api_lists_method_path_output_policy_handler() {
    assert_rich_hover_contains(
        "api",
        &[
            "**`api`**",
            "method <GET|POST|PUT|PATCH|DELETE>",
            "path \"<url>\"",
            "output",
            "policy @policy.",
            "handler",
            "**Example**",
            "docs/quickref.md",
        ],
    );
}

#[test]
fn rich_hover_for_policy_documents_forms_and_predicate_combinators() {
    assert_rich_hover_contains(
        "policy",
        &[
            "**`policy`**",
            "@policy.<name>",
            "@role.",
            "@scope.",
            "@actor.",
            "policies",
            "**Example**",
            "docs/quickref.md",
        ],
    );
}

#[test]
fn rich_hover_for_effect_lists_closed_catalog_of_four_verbs() {
    assert_rich_hover_contains(
        "effect",
        &[
            "**`effect`**",
            "creates",
            "updates",
            "deletes",
            "returns",
            "One mutating effect per command",
            "**Example**",
            "docs/quickref.md",
        ],
    );
}

#[test]
fn rich_hover_for_audit_lists_three_forms() {
    assert_rich_hover_contains(
        "audit",
        &[
            "**`audit`**",
            "`audit`",
            "audit <field>",
            "audit none",
            "emit_to",
            "**Example**",
            "docs/invariants.md",
        ],
    );
}

#[test]
fn rich_hover_for_rate_limit_documents_grammar_and_axes() {
    assert_rich_hover_contains(
        "rate_limit",
        &[
            "**`rate_limit`**",
            "<N> per <window> per <axis>",
            "ip",
            "user",
            "org",
            "tenant",
            "rate_limit none",
            "**Example**",
            "docs/quickref.md",
        ],
    );
}

#[test]
fn rich_hover_returns_none_for_unrelated_keywords() {
    // `domain` is a plain keyword that keeps its brief one-line
    // description; rich hover should not invent Markdown for it.
    assert!(
        rich_keyword_hover("domain").is_none(),
        "rich hover must stay scoped to LSP-extended kinds; `domain` should fall back to keyword_description"
    );
}

#[test]
fn completion_inside_command_offers_effect_verbs_and_children() {
    let source = "feature customer\n  command create\n    policy @policy.create\n    \n";
    // Line 3 (0-indexed) is the indented blank line; cursor at
    // character 4 sits inside the indent.
    let items = completions_at(source, 3, 4);
    let labels = labels(&items);
    for child in [
        "creates",
        "updates",
        "deletes",
        "returns",
        "policy",
        "rate_limit",
        "audit",
        "emits",
        "invalidates",
        "input",
    ] {
        assert!(
            labels.contains(&child),
            "command completion must offer `{child}`; got {labels:?}"
        );
    }
    // Effect verbs lead the list inside `command`.
    assert_eq!(labels[..4], ["creates", "deletes", "returns", "updates"]);
}

#[test]
fn completion_inside_query_list_offers_closed_catalog_children() {
    let source = "feature customer\n  query.list list\n    \n";
    let items = completions_at(source, 2, 4);
    let labels = labels(&items);
    for child in [
        "params", "filters", "search", "order", "paginate", "cache", "policy", "modifier", "scope",
    ] {
        assert!(
            labels.contains(&child),
            "query.list completion must offer `{child}`; got {labels:?}"
        );
    }
}

#[test]
fn completion_inside_query_lookup_offers_params_and_key() {
    let source = "feature customer\n  query.lookup by_id\n    \n";
    let items = completions_at(source, 2, 4);
    let labels = labels(&items);
    for child in ["params", "key", "policy", "cache", "scope"] {
        assert!(
            labels.contains(&child),
            "query.lookup completion must offer `{child}`; got {labels:?}"
        );
    }
}

#[test]
fn completion_inside_query_sql_offers_returns_sql_params() {
    let source = "feature customer\n  query.sql lifetime_value\n    \n";
    let items = completions_at(source, 2, 4);
    let labels = labels(&items);
    for child in ["returns", "sql", "params", "scope", "policy"] {
        assert!(
            labels.contains(&child),
            "query.sql completion must offer `{child}`; got {labels:?}"
        );
    }
}

#[test]
fn completion_inside_query_view_offers_returns_source_params() {
    let source = "feature customer\n  query.view host_home_view\n    \n";
    let items = completions_at(source, 2, 4);
    let labels = labels(&items);
    for child in ["policy", "returns", "source", "params", "scope"] {
        assert!(
            labels.contains(&child),
            "query.view completion must offer `{child}`; got {labels:?}"
        );
    }
}

#[test]
fn completion_inside_api_offers_method_path_output_policy_handler() {
    let source = "feature hello\n  api greet\n    \n";
    let items = completions_at(source, 2, 4);
    let labels = labels(&items);
    for child in [
        "method",
        "path",
        "output",
        "policy",
        "handler",
        "rate_limit",
        "input",
        "audit",
        "route",
    ] {
        assert!(
            labels.contains(&child),
            "api completion must offer `{child}`; got {labels:?}"
        );
    }
}

#[test]
fn completion_inside_tenant_migration_offers_closed_body() {
    let source = "feature customer\n  tenant_migration backfill\n    \n";
    let items = completions_at(source, 2, 4);
    let labels = labels(&items);
    for child in [
        "target",
        "axis",
        "idempotency",
        "timeout",
        "retry",
        "handler",
    ] {
        assert!(
            labels.contains(&child),
            "tenant_migration completion must offer `{child}`; got {labels:?}"
        );
    }
}

#[test]
fn completion_after_policy_namespace_offers_declared_categories() {
    let source = "feature customer\n  policies\n    create: @role.admin\n    read: @scope.same_org\n    update: @role.admin\n\n  command create\n    policy @policy.\n";
    // Cursor sits immediately after `@policy.` on line 7
    // (0-indexed). Compute the character position.
    let line = "    policy @policy.";
    let items = completions_at(source, 7, line.len() as u32);
    let mut labels = labels(&items);
    labels.sort();
    assert_eq!(labels, vec!["create", "read", "update"]);
}

#[test]
fn completion_after_validator_namespace_offers_declared_extensions() {
    let source = "feature customer\n  extensions\n    validator verify_totp: Validator[Customer]\n    fn lifetime_value: Fn[Customer]\n    hook before_create: Hook[CreateCustomer]\n\n  command create\n    validate @validator.\n";
    let line = "    validate @validator.";
    let items = completions_at(source, 7, line.len() as u32);
    let labels = labels(&items);
    assert_eq!(labels, vec!["verify_totp"]);
}

#[test]
fn completion_after_fn_namespace_offers_declared_fns() {
    let source = "feature customer\n  extensions\n    validator verify_totp: Validator[Customer]\n    fn lifetime_value: Fn[Customer]\n    hook before_create: Hook[CreateCustomer]\n\n  command create\n    let v = @fn.\n";
    let line = "    let v = @fn.";
    let items = completions_at(source, 7, line.len() as u32);
    let labels = labels(&items);
    assert_eq!(labels, vec!["lifetime_value"]);
}

#[test]
fn completion_for_rate_limit_axis_offers_closed_catalog() {
    let source = "feature customer\n  command create\n    rate_limit \"30 per hour per ";
    // Cursor sits inside the open string after `per `.
    let line_text = "    rate_limit \"30 per hour per ";
    let items = completions_at(source, 2, line_text.len() as u32);
    let mut labels = labels(&items);
    labels.sort();
    let mut expected: Vec<&str> = RATE_LIMIT_AXES.to_vec();
    expected.sort();
    assert_eq!(labels, expected);
    // Each item carries an `ENUM_MEMBER` kind so VS Code and
    // Helix render the closed set as values, not keywords.
    for item in &items {
        assert_eq!(item.kind, Some(CompletionItemKind::ENUM_MEMBER));
    }
}

#[test]
fn completion_falls_back_outside_known_blocks() {
    // Top-level cursor — not inside command/query/api/agent —
    // returns None so the global keyword list still surfaces.
    let source = "feature customer\n  \n";
    let result = context_aware_completions(
        source,
        Position {
            line: 1,
            character: 2,
        },
    );
    assert!(
        result.is_none(),
        "top-level / unknown context must fall back; got {result:?}"
    );
}

#[test]
fn block_kind_detection_handles_nested_indent() {
    // A `command` block at indent 2 with a child line at indent
    // 4 — block_kind_at must walk back to the header.
    let source = "feature customer\n  command create\n    policy @policy.create\n    ";
    let kind = block_kind_at(
        source,
        Position {
            line: 3,
            character: 4,
        },
    );
    assert_eq!(kind, Some("command"));
}

#[test]
fn block_kind_detection_distinguishes_query_kinds() {
    for (block_header, expected) in [
        ("query.list list", "query.list"),
        ("query.lookup by_id by id: ID", "query.lookup"),
        ("query.sql lifetime_value", "query.sql"),
        ("api greet", "api"),
        ("agent summarize", "agent"),
        ("command create", "command"),
    ] {
        let source = format!("feature x\n  {block_header}\n    ");
        let kind = block_kind_at(
            &source,
            Position {
                line: 2,
                character: 4,
            },
        );
        assert_eq!(
            kind,
            Some(expected),
            "header `{block_header}` should resolve to `{expected}` kind"
        );
    }
}

#[test]
fn kind_child_completions_cover_seven_target_kinds() {
    let kinds: Vec<&str> = KIND_CHILD_COMPLETIONS.iter().map(|(k, _)| *k).collect();
    for required in [
        "command",
        "query.list",
        "query.lookup",
        "query.sql",
        "api",
        "tenant_migration",
    ] {
        assert!(
            kinds.contains(&required),
            "kind catalog must include `{required}`; got {kinds:?}"
        );
    }
}

#[test]
fn effect_verbs_catalog_is_the_canonical_four() {
    let mut verbs = EFFECT_VERBS.to_vec();
    verbs.sort();
    assert_eq!(verbs, vec!["creates", "deletes", "returns", "updates"]);
}

// `doctor_diagnostics_with_code` lives in `super` (mod.rs) so all
// sub-modules share it via `use super::*` above.

