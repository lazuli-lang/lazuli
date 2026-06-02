    // CTX-PATH-UNRESOLVED-001 end-to-end doctor tests (W2-8).
    //
    // Drives a real `.lzi` source through the full doctor package and
    // asserts the analyze-time diagnostic fires on an author-written
    // `ctx.<tail>` whose tail is not in the SoT catalog
    // (`runtime/go/lazuli/ctx_path_catalog.json`), and does NOT fire on a
    // sibling feature using only catalog ctx paths (no false-positive on
    // the shapes the pilots actually write).

    use super::test_support_packages::*;
    use crate::doctor::*;

    // A command `creates` binding reading `ctx.actor.bogus` — the tail
    // `actor.bogus` is not a catalog entry, so it would lower to
    // `lazuli.FromCtx("actor.bogus")` and 500 at runtime with
    // "unknown ctx path: actor.bogus". The diagnostic must fire at
    // analyze time instead.
    const BOGUS_CTX_SRC: &str = r#"
feature widgets
  uses org, user

  domain
    resource Widget
      tenancy org
      owner: User required
      label: Text required
      timestamps

  policies
    author: @scope.same_org

  command create
    input
      label: Text
    policy @policy.author
    creates Widget
      label = input.label
      owner = ctx.actor.bogus
"#;

    // The same feature shape but binding the real catalog path
    // `ctx.actor.id` — must NOT fire (no false-positive).
    const CLEAN_CTX_SRC: &str = r#"
feature widgets
  uses org, user

  domain
    resource Widget
      tenancy org
      owner: User required
      label: Text required
      timestamps

  policies
    author: @scope.same_org

  command create
    input
      label: Text
    policy @policy.author
    creates Widget
      label = input.label
      owner = ctx.actor.id
      created_at = ctx.now
"#;

    #[test]
    fn doctor_fires_on_unknown_ctx_tail_binding() {
        let package = package_from_sources(vec![("widgets.lzi", BOGUS_CTX_SRC)]);
        let diagnostics = package.diagnostics();

        let hit = diagnostics
            .iter()
            .find(|d| d.code == "CTX-PATH-UNRESOLVED-001")
            .unwrap_or_else(|| {
                panic!(
                    "expected CTX-PATH-UNRESOLVED-001 to fire on `ctx.actor.bogus`, got: {:#?}",
                    diagnostics
                )
            });
        assert_eq!(hit.severity, DoctorSeverity::Error);
        assert!(
            hit.message.contains("actor.bogus"),
            "message should name the offending tail: {}",
            hit.message
        );
        // The message lists the known catalog paths for the author.
        assert!(
            hit.message.contains("ctx.actor.id"),
            "message should list known ctx paths: {}",
            hit.message
        );
    }

    #[test]
    fn doctor_does_not_fire_on_known_ctx_paths() {
        let package = package_from_sources(vec![("widgets.lzi", CLEAN_CTX_SRC)]);
        let diagnostics = package.diagnostics();

        assert!(
            !diagnostics
                .iter()
                .any(|d| d.code == "CTX-PATH-UNRESOLVED-001"),
            "CTX-PATH-UNRESOLVED-001 must not fire on catalog paths \
             (ctx.actor.id / ctx.now), got: {:#?}",
            diagnostics
                .iter()
                .filter(|d| d.code == "CTX-PATH-UNRESOLVED-001")
                .collect::<Vec<_>>()
        );
    }
