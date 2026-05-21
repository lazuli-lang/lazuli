//! DUPLICATE-QUERY-NAME-001 (`@correctness.duplicate_query_name`) —
//! duplicate query names inside one feature.
//!
//! Severity: error.
//!
//! Fires when a feature's lowered IR contains two or more queries with the
//! same name. Example: an author writes two `query.list list_customers`
//! blocks, or a convention synth regression appends `lookup_customer` even
//! though the author already declared it. The generated runtime registry is
//! keyed by query name and panics on duplicates, so doctor rejects the shape
//! before codegen/runtime.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use lazuli_ir::{Feature, Query};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub path: PathBuf,
    pub feature: String,
    pub query_name: String,
    pub occurrences: usize,
}

impl Finding {
    pub const CODE: &'static str = "DUPLICATE-QUERY-NAME-001";
    pub const ID: &'static str = "@correctness.duplicate_query_name";

    pub fn message(&self) -> String {
        format!(
            "feature '{}' declares query '{}' more than once. The runtime registry panics on duplicate names. Either rename one or remove the redundant declaration.",
            self.feature, self.query_name
        )
    }
}

pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    check_queries(&feature.name, &feature.queries, path)
}

pub fn check_queries(feature_name: &str, queries: &[Query], path: &Path) -> Vec<Finding> {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for query in queries {
        *counts.entry(query.name()).or_default() += 1;
    }

    counts
        .into_iter()
        .filter_map(|(query_name, occurrences)| {
            (occurrences > 1).then(|| Finding {
                path: path.to_path_buf(),
                feature: feature_name.to_owned(),
                query_name: query_name.to_owned(),
                occurrences,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lower(source: &str) -> Feature {
        let features = lazuli_syntax::parse_feature_skeletons(source).expect("parse feature");
        lazuli_analyzer::lower_feature_skeleton(&features[0]).expect("lower feature")
    }

    #[test]
    fn two_author_declared_queries_with_same_name_fire() {
        let feature = lower(
            r#"
feature catalog
  query.list list_customers
  query.list list_customers
"#,
        );

        let findings = check(&feature, Path::new("catalog.lzi"));

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].feature, "catalog");
        assert_eq!(findings[0].query_name, "list_customers");
        assert_eq!(findings[0].occurrences, 2);
        assert_eq!(Finding::CODE, "DUPLICATE-QUERY-NAME-001");
        assert_eq!(Finding::ID, "@correctness.duplicate_query_name");
        assert_eq!(
            findings[0].message(),
            "feature 'catalog' declares query 'list_customers' more than once. The runtime registry panics on duplicate names. Either rename one or remove the redundant declaration."
        );
    }

    #[test]
    fn author_plus_synth_shaped_duplicate_fires() {
        let mut feature = lower(
            r#"
feature customer
  policies
    authenticated: @scope.authenticated

  resource Customer
    org: Org required
    name: Text required
    conventions [crud]

  query.lookup lookup_customer by id: ID
"#,
        );

        let author_query = feature
            .queries
            .iter()
            .find(|query| query.name() == "lookup_customer")
            .expect("author override lookup present")
            .clone();
        // Simulate a buggy synth pass appending the canonical CRUD lookup
        // even though the author-owned `lookup_customer` should have won.
        feature.queries.push(author_query);

        let findings = check(&feature, Path::new("customer.lzi"));

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].feature, "customer");
        assert_eq!(findings[0].query_name, "lookup_customer");
        assert_eq!(findings[0].occurrences, 2);
    }

    #[test]
    fn distinct_query_names_do_not_fire() {
        let feature = lower(
            r#"
feature catalog
  query.list list_customers
  query.lookup by_id by id: ID
"#,
        );

        assert!(check(&feature, Path::new("catalog.lzi")).is_empty());
    }
}
