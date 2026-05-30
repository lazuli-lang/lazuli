//! Sub-file of the inline LSP test suite — see `mod.rs` for the
//! shared preamble and helpers. Tests are grouped by line-range
//! buckets only; each bucket is ≤ 500 LOC so `clippy` and
//! `rust-analyzer` stay responsive.
#![allow(unused_imports)]
use super::*;

#[test]
fn rich_keyword_hover_describes_conventions_slot() {
    let rich =
        super::rich_keyword_hover("conventions").expect("conventions rich_keyword_hover present");
    assert!(
        rich.contains("Resource-level conventions opt-in"),
        "rich hover should preserve the §4.4 phrasing; got: {rich}"
    );
    assert!(
        rich.contains("`crud`"),
        "rich hover should mention the `crud` bundle; got: {rich}"
    );
    assert!(
        rich.contains("Closed catalog") || rich.contains("**Closed catalog**"),
        "rich hover should label its closed-catalog section; got: {rich}"
    );
}

/// M3 — the rich hover must list both bundles in its closed-catalog
/// section, anchor both proposal paths, and use a composition
/// example (`conventions [crud, me]`) so the surface communicates
/// inter-bundle composition (§6.1) at the editor surface.
#[test]
fn rich_keyword_hover_mentions_both_bundles() {
    let rich =
        super::rich_keyword_hover("conventions").expect("conventions rich_keyword_hover present");
    assert!(
        rich.contains("`crud`"),
        "rich hover should mention the `crud` bundle; got:\n{rich}"
    );
    assert!(
        rich.contains("`me`"),
        "rich hover should mention the `me` bundle; got:\n{rich}"
    );
    assert!(
        rich.contains("ir-resource-conventions-crud"),
        "rich hover should anchor the crud proposal path; got:\n{rich}"
    );
    assert!(
        rich.contains("ir-resource-conventions-me"),
        "rich hover should anchor the me proposal path; got:\n{rich}"
    );
}

#[test]
fn conventions_bundle_hover_on_crud_token_lists_synthesized_entries() {
    let source = r#"
feature customer
  resource Customer
    org: Org required
    name: Text required
    conventions [crud]
"#;
    let offset = source.find("crud").expect("crud token") + 1;
    let hover =
        super::convention_bundle_hover(source, super::position_for_offset(source, offset), "crud")
            .expect("crud bundle hover");

    assert!(
        hover.contains("`conventions [crud]` synthesizes:"),
        "hover should name the bundle; got:\n{hover}"
    );
    assert!(
        hover.contains("`query.list list_<resource_snake>s`"),
        "hover should list the CRUD list query; got:\n{hover}"
    );
    assert!(
        hover.contains("`query.lookup lookup_<resource_snake>`"),
        "hover should list the CRUD lookup query; got:\n{hover}"
    );
    assert!(
        hover.contains("`command create_<resource_snake>`"),
        "hover should list create command; got:\n{hover}"
    );
    assert!(
        hover.contains("author wins"),
        "hover should explain author override behavior; got:\n{hover}"
    );
}

#[test]
fn conventions_bundle_hover_on_me_token_lists_lookup_my() {
    let source = r#"
feature customer
  resource Customer
    org: Org required
    conventions [crud, me]
"#;
    let offset = source.find("me]").expect("me token") + 1;
    let hover =
        super::convention_bundle_hover(source, super::position_for_offset(source, offset), "me")
            .expect("me bundle hover");

    assert!(
        hover.contains("`conventions [me]` synthesizes:"),
        "hover should name the bundle; got:\n{hover}"
    );
    assert!(
        hover.contains("`query.lookup lookup_my_<resource_snake>`"),
        "hover should list lookup_my query; got:\n{hover}"
    );
    assert!(
        hover.contains("author wins"),
        "hover should explain author override behavior; got:\n{hover}"
    );
}

#[test]
fn conventions_bundle_hover_does_not_fire_for_crud_outside_conventions_list() {
    let source = "feature crud\n";
    let offset = source.find("crud").expect("crud word") + 1;

    assert!(
        super::convention_bundle_hover(source, super::position_for_offset(source, offset), "crud",)
            .is_none(),
        "crud should only hover as a convention bundle inside `conventions [...]`"
    );
}

#[test]
fn keywords_list_contains_conventions() {
    assert!(
        KEYWORDS.contains(&"conventions"),
        "`KEYWORDS` should list `conventions` so completions surface it"
    );
}

#[test]
fn conventions_list_completion_inside_brackets_offers_crud_and_me() {
    // Cursor sits inside an open `conventions [` bracket list with
    // no closing `]` on the line. M3 extends the catalog to two
    // bundles; the completer surfaces both.
    let items = super::conventions_list_completions("    conventions [")
        .expect("completion should fire inside `conventions [` bracket list");
    let labels: Vec<&str> = items.iter().map(|item| item.label.as_str()).collect();
    assert_eq!(
        labels,
        vec!["crud", "me"],
        "closed catalog should be `crud, me` (in declaration order)"
    );
}

#[test]
fn conventions_list_completion_after_partial_token_still_offers_crud() {
    // Authoring `conventions [cr<cursor>` is the canonical typo
    // recovery path; the completer should still show `crud`.
    let items = super::conventions_list_completions("    conventions [cr")
        .expect("completion should fire inside `conventions [` with partial token");
    let labels: Vec<&str> = items.iter().map(|item| item.label.as_str()).collect();
    assert!(
        labels.contains(&"crud"),
        "closed catalog must still surface `crud`; got: {labels:?}"
    );
}

#[test]
fn conventions_list_completion_outside_brackets_returns_none() {
    // The cursor is on the keyword itself, not inside `[..]`.
    assert!(
        super::conventions_list_completions("    conventions ").is_none(),
        "completion must not fire before the `[` opens the bracket list"
    );
    // The cursor is past a closed bracket list (parser would have
    // accepted it already); no further completions to offer.
    assert!(
        super::conventions_list_completions("    conventions [crud] ").is_none(),
        "completion must not fire after the closing `]`"
    );
}

#[test]
fn hover_describes_rate_limit_env_qualifier() {
    // The keyword_description one-liner is the LSP hover seed for
    // the `rate_limit` keyword. Per the cell brief, the description
    // must mention the `in <env>` qualifier shape AND list the
    // closed env catalog so an LLM author hovering on the keyword
    // sees the full surface in one tooltip.
    let description =
        super::keyword_description("rate_limit").expect("`rate_limit` keyword_description present");
    assert!(
        description.contains("in <env>"),
        "hover must mention `in <env>` qualifier shape; got: {description}"
    );
    assert!(
        description.contains("production"),
        "hover must list `production` in the closed catalog; got: {description}"
    );
    assert!(
        description.contains("staging"),
        "hover must list `staging` in the closed catalog; got: {description}"
    );
    assert!(
        description.contains("test"),
        "hover must list `test` in the closed catalog; got: {description}"
    );
    assert!(
        description.contains("dev"),
        "hover must list `dev` in the closed catalog; got: {description}"
    );
    assert!(
        description.contains("local"),
        "hover must list `local` in the closed catalog; got: {description}"
    );
    assert!(
        description.contains("default"),
        "hover must describe the default-line semantics; got: {description}"
    );
}

#[test]
fn completion_inside_in_offers_env_catalog() {
    // Cursor sits at `rate_limit "5 per 10 minutes per ip" in <|>`.
    // The completer surfaces the 5-entry closed env catalog so an
    // author can pick `production` / `staging` / `test` / `dev` /
    // `local` without typing it from memory.
    let items = super::rate_limit_env_completions("  rate_limit \"5 per 10 minutes per ip\" in ")
        .expect("completion should fire inside `in <env>` slot");
    let labels: Vec<&str> = items.iter().map(|item| item.label.as_str()).collect();
    assert_eq!(
        labels,
        vec!["production", "staging", "test", "dev", "local"],
        "closed env catalog should match `production, staging, test, dev, local`"
    );
    // Sanity: every item is a closed-catalog ENUM_MEMBER (so
    // editors render them distinctly from arbitrary keywords).
    assert!(
        items
            .iter()
            .all(|i| i.kind == Some(super::CompletionItemKind::ENUM_MEMBER)),
        "all env completions should carry `ENUM_MEMBER` kind; got: {items:?}"
    );

    // After committing one env, the completer filters it out so the
    // author doesn't see duplicate offers. Cursor sits at
    // `rate_limit "..." in dev, <|>`.
    let items =
        super::rate_limit_env_completions("  rate_limit \"5 per 10 minutes per ip\" in dev, ")
            .expect("completion should fire after the comma");
    let labels: Vec<&str> = items.iter().map(|item| item.label.as_str()).collect();
    assert!(
        !labels.contains(&"dev"),
        "already-committed `dev` should be filtered; got: {labels:?}"
    );
    assert!(
        labels.contains(&"staging"),
        "remaining catalog entries should still be offered; got: {labels:?}"
    );

    // Negative case: cursor outside the `in <env>` slot — e.g.
    // still mid-spec — must not fire (axis completion owns that).
    assert!(
        super::rate_limit_env_completions("  rate_limit \"5 per 10 minutes per ip\"").is_none(),
        "completer must not fire when the `in` keyword is absent"
    );
    // Negative case: not a rate_limit line at all.
    assert!(
        super::rate_limit_env_completions("  audit default in ").is_none(),
        "completer must only fire on `rate_limit` lines"
    );
}

#[test]
fn hover_describes_owner_axis_annotation() {
    // The verbatim one-liner from §11.3 is surfaced through the
    // `keyword_description` fallback (matches the `@cap.File` /
    // `cap.File` precedent — both `@owner_axis` and `owner_axis`
    // arms must resolve to the same description).
    let with_at = super::keyword_description("@owner_axis")
        .expect("`@owner_axis` keyword_description present");
    let without_at =
        super::keyword_description("owner_axis").expect("`owner_axis` keyword_description present");
    assert_eq!(
        with_at, without_at,
        "both `@owner_axis` and `owner_axis` must resolve to the same one-liner"
    );
    assert!(
        with_at.contains("Field-level annotation: `@owner_axis(through: <column>)`"),
        "hover should open with the §11.3 verbatim sentence; got: {with_at}"
    );
    assert!(
        with_at.contains("ownership chain"),
        "hover should mention the ownership chain semantics; got: {with_at}"
    );
    assert!(
        with_at.contains("`ctx.User.ID`"),
        "hover should anchor the resolved actor key `ctx.User.ID`; got: {with_at}"
    );
    assert!(
        with_at.contains("ir-resource-conventions-owner-scope.md"),
        "hover should anchor the proposal path; got: {with_at}"
    );

    // The rich Markdown hover gates on the same key; ensure it
    // surfaces the worked example and the doctor codes for the
    // authoring rules (mirroring the `conventions` rich-hover
    // pattern). Cell brief §11.3.
    let rich =
        super::rich_keyword_hover("@owner_axis").expect("`@owner_axis` rich_keyword_hover present");
    assert!(
        rich.contains("**`@owner_axis`**"),
        "rich hover should bold the annotation name; got:\n{rich}"
    );
    assert!(
        rich.contains("host: Host required @owner_axis(through: user)"),
        "rich hover should include the §11.2 worked Property example; got:\n{rich}"
    );
    assert!(
        rich.contains("owner_axis_on_non_fk"),
        "rich hover should reference the parser-level doctor code; got:\n{rich}"
    );
}

#[test]
fn completion_inside_owner_axis_offers_fk_columns() {
    // Authoring shape: cursor sits at the `<|>` position inside
    // `@owner_axis(through: <|>)`. Per §7.5 + the cell brief, the
    // completer offers the FK fields on the current `resource`
    // block — fields whose type is a bare PascalCase identifier
    // (i.e. a reference to another resource), with the builtin
    // closed-catalog skip list (`Text`/`Integer`/`ID`/…) filtered out.
    let source = "\
feature catalog
  resources
    resource Property
      org: Org required
      host: Host required @owner_axis(through: )
      category: ServiceCategory optional
      name: Text required
      conventions [crud]
";
    // The `through:` keyword sits after `: ` — column position is
    // the byte index immediately after `through: ` on line index 4
    // (0-based; the `host:` line).
    let line_idx = 4u32;
    let line = source
        .lines()
        .nth(line_idx as usize)
        .expect("host line present");
    // Cursor right after `through: ` (the trailing space inside the
    // parens, before the closing `)`).
    let cursor = line.find("through: ").expect("through: present") + "through: ".len();
    let pos = super::Position {
        line: line_idx,
        character: cursor as u32,
    };
    let items = super::owner_axis_through_completions(source, pos).expect("completion should fire");
    let labels: Vec<&str> = items.iter().map(|item| item.label.as_str()).collect();
    // The completer offers all FK fields on the resource. `Org`,
    // `Host`, and `ServiceCategory` are PascalCase resource refs
    // (FK). `name: Text` is filtered by the builtin skip list.
    assert!(
        labels.contains(&"org"),
        "FK field `org: Org` should be offered; got: {labels:?}"
    );
    assert!(
        labels.contains(&"host"),
        "FK field `host: Host` should be offered; got: {labels:?}"
    );
    assert!(
        labels.contains(&"category"),
        "FK field `category: ServiceCategory` should be offered; got: {labels:?}"
    );
    assert!(
        !labels.contains(&"name"),
        "builtin-typed field `name: Text` should NOT be offered; got: {labels:?}"
    );
    // Sanity: every item is a FIELD kind (so editors can tag them
    // differently from KEYWORD entries in the popup).
    assert!(
        items
            .iter()
            .all(|i| i.kind == Some(super::CompletionItemKind::FIELD)),
        "all FK completions should carry `FIELD` kind; got: {items:?}"
    );
}

#[test]
fn completion_outside_owner_axis_returns_none() {
    // Sibling negative case: cursor is on a different line entirely
    // (a plain `command` declaration), so `@owner_axis(...)` is not
    // active. The dedicated completer returns `None`, leaving the
    // global keyword list to take over.
    let source = "\
feature catalog
  resources
    resource Property
      host: Host required @owner_axis(through: user)
      conventions [crud]

  command create_property
    policy @policy.create
";
    let pos = super::Position {
        line: 6,
        character: 4,
    };
    assert!(
        super::owner_axis_through_completions(source, pos).is_none(),
        "completer must not fire outside `@owner_axis(...)`"
    );
}

// `assert_rich_hover_contains` lives in `super` (mod.rs) so all
// sub-modules share it via `use super::*` above.

#[test]
fn rich_hover_for_command_describes_required_and_optional_children() {
    assert_rich_hover_contains(
        "command",
        &[
            "**`command`**",
            "**Required children**",
            "policy @policy.",
            "creates",
            "**Optional children**",
            "rate_limit",
            "audit",
            "emits",
            "invalidates",
            "**Example**",
            "```lazuli",
            "docs/quickref.md",
        ],
    );
}

#[test]
fn rich_hover_for_query_list_calls_out_default_order_and_paginate() {
    assert_rich_hover_contains(
        "query.list",
        &[
            "**`query.list`**",
            "order created_at desc",
            "paginate",
            "search",
            "cache",
            "**Example**",
            "docs/quickref.md",
        ],
    );
}
