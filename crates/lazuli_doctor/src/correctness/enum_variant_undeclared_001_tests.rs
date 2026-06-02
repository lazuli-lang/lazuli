// ENUM-VARIANT-UNDECLARED-001 tests — exercise the predicate-RHS variant
// path and the field-default variant path against real lowered IR, plus the
// no-false-positive guards (correct variant, free-text field, dynamic RHS,
// qualified-literal rename alias).

use super::*;
use lazuli_ir::{CompareOp, Expr, ListQuery, Predicate, Query};

fn lower(source: &str) -> Feature {
    let features = lazuli_syntax::parse_feature_skeletons(source).expect("parse feature");
    lazuli_analyzer::lower_feature_skeleton(&features[0]).expect("lower feature")
}

// (a) predicate-RHS variant typo → ERRORS, naming the bad variant + the
//     declared set (audit 05-DX F4 + 03).

const POSTS_TYPO_SRC: &str = r#"
feature blog
  domain
    enum Status
      draft
      published
      archived

    resource Post
      title: Text required
      status: Status = draft

    query.list list_published
      filters
        status == publishedd
"#;

#[test]
fn predicate_rhs_variant_typo_fires() {
    let feature = lower(POSTS_TYPO_SRC);
    let findings = check(&feature, Path::new("blog.lzi"));

    assert_eq!(findings.len(), 1, "exactly one undeclared-variant finding");
    let f = &findings[0];
    assert_eq!(Finding::CODE, "ENUM-VARIANT-UNDECLARED-001");
    assert_eq!(f.enum_name, "Status");
    assert_eq!(f.variant, "publishedd");
    assert_eq!(f.declared, vec!["draft", "published", "archived"]);
    assert!(f.site.contains("list_published"));
    // Message names the bad variant AND the declared set.
    let msg = f.message();
    assert!(msg.contains("publishedd"), "names bad variant: {msg}");
    assert!(msg.contains("`published`"), "names declared variant: {msg}");
    assert!(msg.contains("Status"), "names the enum: {msg}");
}

// (b) correct variant → no false-positive.

const POSTS_CLEAN_SRC: &str = r#"
feature blog
  domain
    enum Status
      draft
      published
      archived

    resource Post
      title: Text required
      status: Status = draft

    query.list list_published
      filters
        status == published
"#;

#[test]
fn correct_variant_does_not_fire() {
    let feature = lower(POSTS_CLEAN_SRC);
    assert!(
        check(&feature, Path::new("blog.lzi")).is_empty(),
        "a correctly-spelled variant must not fire"
    );
}

// (c) free-text Text field compared to a quoted string → no false-positive.
//     `title == "anything"` is `Expr::String` on a non-enum field.

const TEXT_FIELD_SRC: &str = r#"
feature blog
  domain
    enum Status
      draft
      published

    resource Post
      title: Text required
      status: Status = draft

    query.list search_title
      filters
        title == "anything"
"#;

#[test]
fn text_field_string_compare_does_not_fire() {
    let feature = lower(TEXT_FIELD_SRC);
    let findings = check(&feature, Path::new("blog.lzi"));
    assert!(
        findings.is_empty(),
        "a Text field == quoted string must not fire: {findings:?}"
    );
}

// (d) dynamic / bound RHS (`status == params.kind`) is a runtime Path, never
//     an enum literal → no false-positive.

const DYNAMIC_RHS_SRC: &str = r#"
feature blog
  domain
    enum Status
      draft
      published

    resource Post
      title: Text required
      status: Status = draft

    query.list list_by_status
      params
        kind: Status optional

      filters
        status == params.kind
"#;

#[test]
fn dynamic_param_rhs_does_not_fire() {
    let feature = lower(DYNAMIC_RHS_SRC);
    assert!(
        check(&feature, Path::new("blog.lzi")).is_empty(),
        "a param-bound RHS must not fire"
    );
}

// (e) field-default variant typo → ERRORS (bare-variant case, audit 05-DX F4).

const FIELD_DEFAULT_TYPO_SRC: &str = r#"
feature blog
  domain
    enum Status
      draft
      published

    resource Post
      title: Text required
      status: Status = publishd
"#;

#[test]
fn field_default_variant_typo_fires() {
    let feature = lower(FIELD_DEFAULT_TYPO_SRC);
    let findings = check(&feature, Path::new("blog.lzi"));

    assert_eq!(findings.len(), 1, "field default typo fires once");
    let f = &findings[0];
    assert_eq!(f.enum_name, "Status");
    assert_eq!(f.variant, "publishd");
    assert!(f.site.contains("Post.status"), "names field site: {}", f.site);
}

// (f) correct field default → no false-positive.

const FIELD_DEFAULT_CLEAN_SRC: &str = r#"
feature blog
  domain
    enum Status
      draft
      published

    resource Post
      title: Text required
      status: Status = published
"#;

#[test]
fn field_default_correct_variant_does_not_fire() {
    let feature = lower(FIELD_DEFAULT_CLEAN_SRC);
    assert!(
        check(&feature, Path::new("blog.lzi")).is_empty(),
        "a correct field default must not fire"
    );
}

// (g) UNIT — a `previous_names` rename alias must not false-positive. Built
//     directly because the surface lacks a variant-rename authoring form
//     today; the rule still honours the alias so a downstream rename pass
//     does not trip it.

#[test]
fn variant_rename_alias_does_not_fire() {
    let mut feature = lower(POSTS_CLEAN_SRC);
    // Rename `published` -> `live`, recording `published` as a previous name.
    for e in &mut feature.enums {
        if e.name == "Status" {
            for v in &mut e.variants {
                if v.name == "published" {
                    v.name = "live".into();
                    v.previous_names = vec!["published".into()];
                }
            }
        }
    }
    // The query still filters `status == published` (the old name) — an alias,
    // not a typo.
    assert!(
        check(&feature, Path::new("blog.lzi")).is_empty(),
        "a recorded rename alias must not fire"
    );
}

// (h) UNIT — conservative skip when no resource binds the column (no enum to
//     check against → no false positive).

#[test]
fn unbound_unqualified_literal_is_skipped() {
    // The query's resource (`Post` from `feature_stub`) has no `status`
    // field, so the unqualified literal cannot bind to an enum-typed column
    // → conservative skip, no false positive.
    let feature = Feature {
        queries: vec![Query::List(ListQuery {
            name: "orphan".into(),
            filters: vec![lazuli_ir::Filter {
                predicate: Predicate::Comparison {
                    left: Expr::Path(lazuli_ir::Path::from_segments(["status"])),
                    op: CompareOp::Eq,
                    right: Expr::Enum(EnumLiteral {
                        type_name: None,
                        variant: "whatever".into(),
                    }),
                },
                when: None,
            }],
            ..list_query_stub()
        })],
        ..feature_stub()
    };
    assert!(check(&feature, Path::new("x.lzi")).is_empty());
}

fn feature_stub() -> Feature {
    // Reuse the lowered shape of a minimal feature to get a complete default,
    // then callers override the slices they care about via `..`.
    lower(
        r#"
feature blog
  domain
    resource Post
      title: Text required
"#,
    )
}

fn list_query_stub() -> ListQuery {
    ListQuery {
        name: "stub".into(),
        public_contract: None,
        params: vec![],
        scope: vec![],
        scope_override: false,
        filters: vec![],
        order: vec![],
        paginate: None,
        modifier: None,
        cache: None,
        policy: lazuli_ir::PolicyRef::None,
        policy_expr: None,
        policy_when_denied: None,
        previous_names: vec![],
        span_ref: None,
        owner_scope_sql: None,
    }
}
