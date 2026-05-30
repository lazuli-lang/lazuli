//! MISSING-POLICY-ON-QUERY-001 — query relies on implicit public policy.
//!
//! ## Rule statement
//!
//! Fires when a `query.list`, `query.lookup`, or `query.sql` has no authored
//! `policy` / structured policy expression and the containing feature has no
//! `Feature.defaults.policy` to inherit. The runtime fallback is public access;
//! that can be legitimate for marketing or catalog reads, but the source should
//! say so with `policy @policy.public` instead of relying on an invisible
//! default.
//!
//! ## Severity profile
//!
//! Severity: `warning` in both strict and production profiles. The rule does
//! not escalate in production because public queries are valid, but implicit
//! public exposure is risky enough to stay visible in all profiles.
//!
//! ## Fixture example
//!
//! ```lzi
//! feature catalog
//!   query.list list
//! ```
//!
//! Canonical fix when public access is intended:
//!
//! ```lzi
//! feature catalog
//!   query.list list
//!     policy @policy.public
//! ```
//!
//! Canonical fix when the feature should default to authenticated reads:
//!
//! ```lzi
//! feature catalog
//!   defaults
//!     policy_for queries: @policy.authenticated
//!   query.list list
//! ```
//!
//! ## Proposal anchor
//!
//! No standalone matching file was found in historical `docs/proposals/`.
//! The rule landed in commit `4dbe32d` (`LAZ-62`) after `QUERY-POLICY-001`
//! added per-query policy lowering, and anchors to query policy semantics in
//! `docs/canonical-semantics.md`.
//!
//! Diagnostic ID / code constant: `MISSING-POLICY-ON-QUERY-001`;
//! `Finding::CODE` is `pub const CODE: &'static str =
//! "MISSING-POLICY-ON-QUERY-001";`.

use std::path::{Path, PathBuf};

use lazuli_ir::{Feature, PolicyExpr, PolicyRef, Query};

/// One MISSING-POLICY-ON-QUERY-001 finding — a query has no authored
/// policy AND the feature has no `defaults.policy_for queries:` fallback,
/// so the query is implicitly public.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file the offending query was authored in.
    pub path: PathBuf,
    /// Feature owning the query.
    pub feature: String,
    /// Query kind tag — `list`, `lookup`, `count`, etc.
    pub query_kind: &'static str,
    /// Query name (matches the bare identifier after `query.<kind>`).
    pub query_name: String,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "MISSING-POLICY-ON-QUERY-001";

    /// Render the "implicit public exposure" message and prompt the
    /// author to declare either `@policy.public` or `@policy.authenticated`.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::correctness::missing_policy_on_query_001::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("f.lzi"),
    ///     feature: "catalog".into(),
    ///     query_kind: "list",
    ///     query_name: "list_customers".into(),
    /// };
    /// assert!(f.message().contains("@scope.public"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "query.{} {} has no authored policy and the feature has no defaults.policy. \
             Implicit @scope.public exposes the query to anonymous callers. Add \
             'policy @policy.public' to make the intent explicit, or \
             'policy @policy.authenticated' to gate it.",
            self.query_kind, self.query_name
        )
    }
}

/// Run MISSING-POLICY-ON-QUERY-001 over one feature.
///
/// Delegates to [`check_queries`] with the feature's own queries slice
/// and `defaults.policy` fallback.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::correctness::missing_policy_on_query_001::check;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower a feature with queries");
/// let _ = check(&feature, Path::new("catalog.lzi"));
/// ```
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    check_queries(
        &feature.name,
        feature.defaults.policy.as_ref(),
        &feature.queries,
        path,
    )
}

/// Run MISSING-POLICY-ON-QUERY-001 against an arbitrary `Query` slice.
///
/// Exposed so tests and downstream tooling can lint queries assembled
/// outside of a feature (post-synth pipelines).
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::correctness::missing_policy_on_query_001::check_queries;
/// use lazuli_ir::Query;
///
/// let queries: Vec<Query> = vec![];
/// let _ = check_queries("catalog", None, &queries, Path::new("catalog.lzi"));
/// ```
pub fn check_queries(
    feature_name: &str,
    defaults_policy: Option<&PolicyRef>,
    queries: &[Query],
    path: &Path,
) -> Vec<Finding> {
    if defaults_policy.is_some_and(|policy| !policy.is_none()) {
        return Vec::new();
    }

    queries
        .iter()
        .filter_map(|query| {
            let (kind, name, policy, policy_expr) = query_policy_facts(query);
            if policy.is_none() && policy_expr.is_none() {
                Some(Finding {
                    path: path.to_path_buf(),
                    feature: feature_name.to_owned(),
                    query_kind: kind,
                    query_name: name.to_owned(),
                })
            } else {
                None
            }
        })
        .collect()
}

fn query_policy_facts(query: &Query) -> (&'static str, &str, &PolicyRef, Option<&PolicyExpr>) {
    match query {
        Query::List(query) => (
            "list",
            query.name.as_str(),
            &query.policy,
            query.policy_expr.as_ref(),
        ),
        Query::Lookup(query) => (
            "lookup",
            query.name.as_str(),
            &query.policy,
            query.policy_expr.as_ref(),
        ),
        Query::Sql(query) => (
            "sql",
            query.name.as_str(),
            &query.policy,
            query.policy_expr.as_ref(),
        ),
        // query.compose: W2/W3 — policy facts are real (same policy fields),
        // so MISSING-POLICY-ON-QUERY-001 correctly guards compose reads from
        // day one (a compose read is an HTTP boundary like any other query).
        Query::Compose(query) => (
            "compose",
            query.name.as_str(),
            &query.policy,
            query.policy_expr.as_ref(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lower(source: &str) -> Feature {
        let features = lazuli_syntax::parse_feature_skeletons(source).expect("parse feature");
        lazuli_analyzer::lower_feature_skeleton(&features[0]).expect("lower feature")
    }

    #[test]
    fn query_without_policy_and_without_default_fires() {
        let feature = lower(
            r#"
feature catalog
  query.list list
"#,
        );

        let findings = check(&feature, Path::new("catalog.lzi"));

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].query_kind, "list");
        assert_eq!(findings[0].query_name, "list");
        assert_eq!(Finding::CODE, "MISSING-POLICY-ON-QUERY-001");
    }

    #[test]
    fn explicit_public_policy_does_not_fire() {
        let feature = lower(
            r#"
feature marketing
  query.list registration_landing_marketing
    policy @policy.public
"#,
        );

        assert!(check(&feature, Path::new("marketing.lzi")).is_empty());
    }

    #[test]
    fn feature_default_policy_does_not_fire() {
        let feature = lower(
            r#"
feature catalog
  defaults
    policy_for queries: @policy.authenticated

  query.lookup by_id by id: ID
"#,
        );

        assert!(check(&feature, Path::new("catalog.lzi")).is_empty());
    }

    #[test]
    fn structured_policy_expr_does_not_fire() {
        let feature = lower(
            r#"
feature catalog
  query.sql search
    returns SearchResult
    sql "./queries/search.sql"
    policy authenticated
"#,
        );

        assert!(check(&feature, Path::new("catalog.lzi")).is_empty());
    }
}
