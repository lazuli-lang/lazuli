//! Sub-file of the inline LSP test suite — see `mod.rs` for the
//! shared preamble and helpers. Tests are grouped by line-range
//! buckets only; each bucket is ≤ 500 LOC so `clippy` and
//! `rust-analyzer` stay responsive.
#![allow(unused_imports)]
use super::*;

#[test]
fn canonical_order_reports_late_uses() {
    let source = r#"
feature customer
  purpose "Customers"

  domain
    resource Customer

  uses org
"#;

    let diagnostics = diagnostics_for(source);

    assert_eq!(diagnostics.len(), 1);
    assert!(
        diagnostics[0]
            .message
            .contains("`uses` appears after `domain`")
    );
}

#[test]
fn canonical_order_reports_late_webhook_after_surface() {
    let source = r#"
registry
  env
    server STRIPE_WEBHOOK_SECRET: Secret required

feature billing
  purpose "Billing"

  domain
    resource Invoice

  surface web admin
    view list Table

  webhook stripe_invoice_paid
    path "/webhooks/stripe/invoice-paid"
    verify hmac sha256
      secret env.STRIPE_WEBHOOK_SECRET
      header "Stripe-Signature"
    idempotency by payload.provider_event_id
"#;

    let diagnostics = diagnostics_for(source);

    assert_eq!(diagnostics.len(), 1);
    assert!(
        diagnostics[0]
            .message
            .contains("`webhook` appears after `surface`")
    );
}

#[test]
fn canonical_formatter_reorders_feature_blocks() {
    let source = r#"
registry
  env
    server INBOUND_WEBHOOK_SECRET: Secret required

feature customer
  purpose "Customers"

  surface web admin
    view list Table

  uses org

  domain
    resource Customer

  webhook inbound
    path "/webhooks/inbound"
    verify hmac sha256
      secret env.INBOUND_WEBHOOK_SECRET
      header "X-Signature"
    idempotency by payload.id
"#;

    let formatted = format_canonical_source(source).expect("canonical source");

    assert!(
        formatted.find("  uses org").unwrap() < formatted.find("  domain").unwrap(),
        "uses should move before domain:\n{formatted}"
    );
    assert!(
        formatted.find("  webhook inbound").unwrap()
            < formatted.find("  surface web admin").unwrap(),
        "webhook should move before surface:\n{formatted}"
    );
    assert!(
        diagnostics_for(&formatted).is_empty(),
        "formatter should produce canonical order"
    );
}

#[test]
fn design_lzi_completion_surfaces_token_groups() {
    let uri = Url::parse("file:///workspace/design.lzi").unwrap();
    let items = completion_items_for_uri(&uri);
    let labels: Vec<_> = items.iter().map(|item| item.label.as_str()).collect();

    for group in [
        "color",
        "typography",
        "space",
        "radius",
        "shadow",
        "motion",
        "breakpoint",
        "z",
    ] {
        assert!(
            labels.contains(&group),
            "`design.lzi` completions should include `{group}`"
        );
    }
}

#[test]
fn feature_lzi_does_not_surface_design_keywords() {
    let uri = Url::parse("file:///workspace/features/customer/customer.lzi").unwrap();
    let items = completion_items_for_uri(&uri);
    let labels: Vec<_> = items.iter().map(|item| item.label.as_str()).collect();

    for design_only in [
        "color",
        "typography",
        "space",
        "radius",
        "shadow",
        "motion",
        "breakpoint",
        "z",
    ] {
        assert!(
            !labels.contains(&design_only),
            "feature `.lzi` completions should not include design keyword `{design_only}`"
        );
    }
}

#[test]
fn design_keyword_hovers_link_to_proposal() {
    for kw in DESIGN_KEYWORDS {
        let description =
            design_keyword_description(kw).unwrap_or_else(|| panic!("hover for `{kw}` missing"));
        assert!(description.contains("docs/proposals/design-tokens.md"));
    }
}

#[test]
fn keyword_hover_describes_cap_file_arguments() {
    for kw in ["max_size", "accept", "visibility", "signed_ttl"] {
        let description =
            keyword_description(kw).unwrap_or_else(|| panic!("hover for `{kw}` missing"));
        assert!(
            !description.is_empty(),
            "hover for `{kw}` must be non-empty"
        );
    }
}

#[test]
fn keyword_hover_describes_encryption_block() {
    let description = keyword_description("encryption").expect("encryption hover present");
    assert!(description.contains("@key."));
    assert!(description.contains("@cap.Encrypted"));
}

#[test]
fn keyword_hover_describes_rotation_strategy() {
    let description = keyword_description("rotation").expect("rotation hover present");
    assert!(description.contains("manual"));
}

#[test]
fn keyword_hover_describes_cap_file_decorator() {
    for kw in ["@cap.File", "cap.File"] {
        assert!(
            keyword_description(kw).is_some(),
            "hover for `{kw}` must be available"
        );
    }
}

#[test]
fn keyword_hover_visibility_lists_closed_catalog() {
    let description = keyword_description("visibility").unwrap();
    assert!(description.contains("public"));
    assert!(description.contains("private"));
    assert!(description.contains("signed"));
}

#[test]
fn keyword_hover_describes_tenant_migration_children() {
    let description = keyword_description("tenant_migration").unwrap();
    assert!(description.contains("target query."));
    assert!(description.contains("axis <tenant_axis>"));
    assert!(description.contains("idempotency <path>"));
    assert!(
        keyword_description("axis")
            .unwrap()
            .contains("defaults.tenancy")
    );
}

#[test]
fn keywords_list_contains_storage_arguments() {
    for kw in ["max_size", "accept", "visibility", "signed_ttl"] {
        assert!(
            KEYWORDS.contains(&kw),
            "`KEYWORDS` should list `{kw}` so completions surface it"
        );
    }
}

#[test]
fn cap_file_value_completion_for_visibility_offers_closed_catalog() {
    let source = "    output @cap.File(max_size:10mb,accept:text/csv,visibility:";
    let position = Position {
        line: 0,
        character: source.len() as u32,
    };
    let items = cap_file_value_completions(source, position).expect("visibility offers");
    let labels: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();
    assert_eq!(labels, vec!["public", "private", "signed"]);
}

#[test]
fn cap_file_value_completion_for_max_size_offers_units() {
    let source = "    file: @cap.File(max_size:25";
    let position = Position {
        line: 0,
        character: source.len() as u32,
    };
    let items = cap_file_value_completions(source, position).expect("max_size offers");
    let labels: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();
    assert_eq!(labels, vec!["kb", "mb", "gb"]);
}

#[test]
fn cap_file_value_completion_for_signed_ttl_offers_units() {
    let source =
        "    output @cap.File(max_size:10mb,accept:text/csv,visibility:signed,signed_ttl:1";
    let position = Position {
        line: 0,
        character: source.len() as u32,
    };
    let items = cap_file_value_completions(source, position).expect("signed_ttl offers");
    let labels: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();
    assert_eq!(labels, vec!["s", "m", "h", "d"]);
}

#[test]
fn cap_file_value_completion_for_accept_offers_mime_families() {
    let source = "    output @cap.File(max_size:10mb,accept:";
    let position = Position {
        line: 0,
        character: source.len() as u32,
    };
    let items = cap_file_value_completions(source, position).expect("accept offers");
    let labels: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();
    assert_eq!(
        labels,
        vec![
            "text",
            "image",
            "application",
            "audio",
            "video",
            "font",
            "*"
        ]
    );
}

#[test]
fn cap_file_value_completion_returns_none_outside_capability() {
    let source = "    file: Text";
    let position = Position {
        line: 0,
        character: source.len() as u32,
    };
    assert!(cap_file_value_completions(source, position).is_none());
}

#[test]
fn error_page_hover_and_status_completion_are_available() {
    let hover = rich_keyword_hover("error_page").expect("error_page hover");
    assert!(hover.contains("Closed catalog") || hover.contains("closed catalog"));

    let source = "  error_page 4";
    let position = Position {
        line: 0,
        character: source.len() as u32,
    };
    let items = context_aware_completions(source, position).expect("status completions");
    let labels: Vec<_> = items.iter().map(|item| item.label.as_str()).collect();
    assert!(labels.contains(&"404"));
    assert!(labels.contains(&"503"));
}

#[test]
fn error_page_child_completion_offers_template_and_audience() {
    let source = "app Acme\n  error_page 404\n    ";
    let position = Position {
        line: 2,
        character: 4,
    };
    let items = context_aware_completions(source, position).expect("child completions");
    let labels: Vec<_> = items.iter().map(|item| item.label.as_str()).collect();
    assert_eq!(labels, vec!["template", "audience"]);
}

#[test]
fn error_page_audience_completion_offers_common_values() {
    let source = "app Acme\n  error_page 404\n    audience p";
    let position = Position {
        line: 2,
        character: "    audience p".len() as u32,
    };
    let items = context_aware_completions(source, position).expect("audience completions");
    let labels: Vec<_> = items.iter().map(|item| item.label.as_str()).collect();
    assert!(labels.contains(&"public"));
}

#[test]
fn keyword_hover_describes_notification_digest_children() {
    for kw in [
        "digest",
        "every",
        "group_by",
        "max_size",
        "template_strategy",
    ] {
        assert!(
            keyword_description(kw).is_some(),
            "hover for `{kw}` must be available"
        );
    }
}

#[test]
fn keyword_hover_describes_notification_throttle_children() {
    for kw in [
        "throttle",
        "max_per",
        "per_recipient",
        "per_channel",
        "burst",
    ] {
        assert!(
            keyword_description(kw).is_some(),
            "hover for `{kw}` must be available"
        );
    }
}

#[test]
fn keyword_hover_throttle_distinguishes_from_rate_limit() {
    let throttle = keyword_description("throttle").unwrap();
    assert!(
        throttle.contains("per-recipient") || throttle.contains("Distinct from"),
        "throttle hover must call out the distinction from scalar rate_limit; got `{throttle}`"
    );
}

#[test]
fn keywords_list_contains_notification_subblocks() {
    for kw in [
        "digest",
        "throttle",
        "every",
        "group_by",
        "max_size",
        "template_strategy",
        "max_per",
        "per_recipient",
        "per_channel",
        "burst",
    ] {
        assert!(
            KEYWORDS.contains(&kw),
            "`KEYWORDS` should list `{kw}` so completions surface it"
        );
    }
}

#[test]
fn notification_digest_template_strategy_catalog_has_two_entries() {
    use super::NOTIFICATION_DIGEST_TEMPLATE_STRATEGY_VALUES;
    assert_eq!(NOTIFICATION_DIGEST_TEMPLATE_STRATEGY_VALUES.len(), 2);
    for value in NOTIFICATION_DIGEST_TEMPLATE_STRATEGY_VALUES {
        assert!(
            super::notification_digest_template_strategy_detail(value).is_some(),
            "detail for `{value}` must be available"
        );
    }
}

#[test]
fn keyword_hover_describes_webhook_event_registry_kind() {
    let hover = keyword_description("webhook_event").expect("webhook_event hover");
    assert!(hover.contains("outbound"), "{hover}");
    assert!(hover.contains("Distinct from inbound `webhook`"), "{hover}");
    assert!(keyword_description("previous_version").is_some());
}

#[test]
fn keywords_list_contains_webhook_event_registry_kind() {
    for kw in [
        "webhook_event",
        "payload",
        "version",
        "previous_version",
        "deprecated",
    ] {
        assert!(
            KEYWORDS.contains(&kw),
            "`KEYWORDS` should list `{kw}` so completions surface it"
        );
    }
}

#[test]
fn keyword_hover_describes_conventions_slot() {
    let one_liner =
        keyword_description("conventions").expect("conventions keyword_description present");
    // Verbatim phrasing from the proposal §4.4 — the hover surface,
    // the docstring on `Resource.conventions`, and the doctor
    // diagnostic share this template.
    assert!(
        one_liner.contains("Resource-level conventions opt-in"),
        "conventions one-liner should open with the §4.4 phrasing; got: {one_liner}"
    );
    assert!(
        one_liner.contains("`conventions [<name1>, <name2>, ...]`"),
        "conventions one-liner should show the slot syntax verbatim; got: {one_liner}"
    );
    assert!(
        one_liner.contains("Today's catalog: `crud`, `me`"),
        "conventions one-liner should pin the two-member catalog; got: {one_liner}"
    );
    assert!(
        one_liner.contains("ir-resource-conventions-crud"),
        "conventions one-liner should anchor the crud proposal path; got: {one_liner}"
    );
    assert!(
        one_liner.contains("ir-resource-conventions-me"),
        "conventions one-liner should anchor the me proposal path; got: {one_liner}"
    );
}
