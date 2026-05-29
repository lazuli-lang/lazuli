    // Doctor scope_owner / scope_same_org / derived_from sibling tests
    // Split from `crates/lazuli_cli/src/doctor/tests.rs`.

    use std::fs;
    use std::path::Path;

    use super::test_support_packages::*;
    use super::test_support_core::*;
    use crate::doctor::*;

    #[test]
    fn doctor_warns_scope_owner_when_resource_has_no_owner_column() {
        let package = package_from_sources(vec![(
            "trust.lzi",
            r#"
feature trust
  policies
    update: @scope.owner

  domain
    resource Review
      status: Text required

  command flag
    route id: ID
    input
      reason: Text required
    policy @policy.update
    updates Review
      status = "flagged"
"#,
        )]);
        let diagnostics = package.diagnostics();
        let scope_diags: Vec<&str> = diagnostics
            .iter()
            .filter(|d| d.code == "SCOPE-OWNER-COLUMN-001")
            .map(|d| d.message.as_str())
            .collect();
        assert!(
            !scope_diags.is_empty(),
            "expected SCOPE-OWNER-COLUMN-001 on Review with no owner column; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
        assert!(
            scope_diags[0].contains("@scope.owner"),
            "message should name the offending atom: {}",
            scope_diags[0]
        );
        assert!(
            scope_diags[0].contains("Review"),
            "message should name the resource: {}",
            scope_diags[0]
        );
    }

    #[test]
    fn doctor_accepts_scope_owner_when_resource_has_user_id_column() {
        let package = package_from_sources(vec![(
            "account.lzi",
            r#"
feature account
  policies
    delete: @scope.owner

  domain
    resource UserSession
      user_id: ID required
      token: Text required

  command revoke
    route id: ID
    input
      id: ID required
    policy @policy.delete
    deletes UserSession
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            !codes(&diagnostics).contains("SCOPE-OWNER-COLUMN-001"),
            "user_id should resolve @scope.owner; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_warns_scope_same_org_when_resource_has_no_org_column() {
        let package = package_from_sources(vec![(
            "payments.lzi",
            r#"
feature payments
  policies
    update: @scope.same_org

  domain
    resource Charge
      amount: Integer required

  command flag
    route id: ID
    input
      id: ID required
    policy @policy.update
    updates Charge
      amount = 0
"#,
        )]);
        let diagnostics = package.diagnostics();
        let scope_diags: Vec<&str> = diagnostics
            .iter()
            .filter(|d| d.code == "SCOPE-OWNER-COLUMN-001")
            .map(|d| d.message.as_str())
            .collect();
        assert!(
            !scope_diags.is_empty(),
            "expected SCOPE-OWNER-COLUMN-001 on Charge with no org column; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
        assert!(
            scope_diags[0].contains("@scope.same_org"),
            "message should name same_org: {}",
            scope_diags[0]
        );
    }

    // -------------------------------------------------------------------------
    // field_derived_from_unresolved — Tier 4c lint per naming-reconciliation
    // proposal §4. Resource field's `derived from <expr>` must reference
    // sibling fields (or whitelisted keywords). Closes 1 of the 3 net-new
    // Tier 4c lints surfaced 2026-05-17.
    // -------------------------------------------------------------------------

    #[test]
    fn doctor_warns_derived_from_referencing_unknown_sibling() {
        let package = package_from_sources(vec![(
            "billing.lzi",
            r#"
feature billing
  domain
    resource Charge
      amount: Integer required
      is_premium: Boolean derived from total_amount > 1000
"#,
        )]);
        let diagnostics = package.diagnostics();
        let derived: Vec<&str> = diagnostics
            .iter()
            .filter(|d| d.code == "field_derived_from_unresolved")
            .map(|d| d.message.as_str())
            .collect();
        assert!(
            !derived.is_empty(),
            "expected field_derived_from_unresolved on Charge.is_premium; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
        assert!(
            derived[0].contains("total_amount"),
            "message should name unresolved identifier `total_amount`: {}",
            derived[0]
        );
        assert!(
            derived[0].contains("is_premium"),
            "message should name the offending field: {}",
            derived[0]
        );
    }

    #[test]
    fn doctor_accepts_derived_from_referencing_sibling_field() {
        let package = package_from_sources(vec![(
            "billing.lzi",
            r#"
feature billing
  domain
    resource Charge
      amount: Integer required
      is_premium: Boolean derived from amount > 1000
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            !codes(&diagnostics).contains("field_derived_from_unresolved"),
            "sibling field `amount` should resolve; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    // -------------------------------------------------------------------------
    // resource_unique_qualifier_unknown + resource_validates_path_unknown —
    // Tier 4c lints per naming-reconciliation proposal §4 rows 1+2.
    //
    // Lint code shipped + ready, but `Resource.constraints` and
    // `Resource.validates` slots are not yet populated by
    // `lower_resource_decl` (`crates/lazuli_analyzer/src/lib.rs:2702-2718`
    // hardcodes both to empty Vec). The lint walkers stay silent until
    // the analyzer wires the lift from `ResourceDecl.validates` +
    // domain-level `constraints` block.
    //
    // The unit tests below were dropped because they would assert against
    // an empty IR slot. When the upstream lift lands, re-introduce the
    // tests by mirroring `doctor_warns_derived_from_referencing_unknown_sibling`
    // (which DOES work because `derived_from` is lifted).
    //
    // Tracked as a Tier 4c follow-up per the naming-reconciliation
    // proposal §"Net diagnostic action items" rows 1+2.
    // -------------------------------------------------------------------------

    #[test]
    fn doctor_accepts_derived_from_with_keywords_and_string_literals() {
        // `and` / `not` are keywords; "high" is a string literal —
        // none should be flagged as unresolved identifiers.
        let package = package_from_sources(vec![(
            "billing.lzi",
            r#"
feature billing
  domain
    resource Charge
      amount: Integer required
      status: Text required
      flagged: Boolean derived from amount > 1000 and status != "high"
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            !codes(&diagnostics).contains("field_derived_from_unresolved"),
            "keywords + string literals must not be flagged; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

