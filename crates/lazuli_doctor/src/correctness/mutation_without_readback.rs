//! MUTATION-WITHOUT-READBACK-001 — `command` mutates a resource with no
//! reachable `query.lookup` / `query.list` for the same resource.
//!
//! Codegen-correctness cycle 2026-05-21 cell A6. After A5 lands,
//! `command.invalidates` is auto-populated for any `updates X` command
//! that has a matching `lookup_my_X` (or sibling list) query. But if the
//! feature declares zero read queries against the mutated resource, the
//! inference stays empty AND there is nothing for the front-end cache to
//! invalidate — the post-mutation UI silently keeps stale state.
//!
//! This diagnostic surfaces that gap. The author has three explicit
//! ways out:
//!   * Add `query lookup_my_<r>` / `query.list mine_<r>s` (the canonical
//!     read shape — same form A5 wires invalidates against).
//!   * Add any other `query.lookup` / `query.list` that references the
//!     mutated resource by the convention naming (`lookup_<r>`,
//!     `list_<r>`).
//!   * Author the read query in a different feature that `uses` this
//!     one — cross-feature reads count, per the cycle decision.
//!
//! Diagnostic ID: `@correctness.mutation_without_readback` (catalog
//! code `MUTATION-WITHOUT-READBACK-001`). Severity: warning.

use std::path::{Path, PathBuf};

use lazuli_ir::{Command, CommandEffect, Feature, QualifiedName, Query};

/// Single command without a matching read query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub path: PathBuf,
    pub feature: String,
    pub command: String,
    pub effect_kind: &'static str,
    /// Snake-cased resource name (matches the naming convention used by
    /// `lookup_my_<r>` / `list_<r>s` auto-synth) — included in the
    /// hint to give the author copy-paste-ready boilerplate.
    pub resource_snake: String,
    /// Original casing (e.g. `Customer`) — included for the human-
    /// facing message so the diagnostic reads like the source.
    pub resource_display: String,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "MUTATION-WITHOUT-READBACK-001";
    /// Author-facing rule ID used in `# doctor:allow` and prose docs.
    pub const ID: &'static str = "@correctness.mutation_without_readback";

    /// Render the "no query reads it back" message and offer the
    /// canonical readback shapes (`lookup_my_<r>` / `mine_<r>s`).
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::correctness::mutation_without_readback::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("f.lzi"),
    ///     feature: "customer".into(),
    ///     command: "update_customer".into(),
    ///     effect_kind: "updates",
    ///     resource_snake: "customer".into(),
    ///     resource_display: "Customer".into(),
    /// };
    /// assert!(f.message().contains("no Query reads it back"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "command '{}' mutates resource '{}' but no Query reads it back. \
             Front-end consumers will have no cached state to invalidate; \
             either add 'query lookup_my_{}' / 'query.list mine_{}s' or \
             document why no readback is needed.",
            self.command, self.resource_display, self.resource_snake, self.resource_snake
        )
    }
}

/// File-local single-feature check. Looks only at the feature's own
/// queries — used by the LSP for live squiggles where neighbor features
/// are not loaded.
///
/// Prefer [`check_with_neighbors`] from the CLI dispatch path, which
/// also counts cross-feature read queries (per cycle decision).
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::correctness::mutation_without_readback::check;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower a feature with commands + queries");
/// let _ = check(&feature, Path::new("customer.lzi"));
/// ```
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    let no_neighbors: [&Feature; 0] = [];
    check_with_neighbors(feature, path, no_neighbors.iter().copied())
}

/// Multi-feature check. `neighbors` is the iterator of every other
/// feature in the doctor package. A read query found in any neighbor
/// (including this feature itself, which `neighbors` may legitimately
/// include — duplicates are deduplicated by identity) counts.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::correctness::mutation_without_readback::check_with_neighbors;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower a feature");
/// let neighbors: Vec<&Feature> = vec![];
/// let _ = check_with_neighbors(&feature, Path::new("customer.lzi"), neighbors);
/// ```
pub fn check_with_neighbors<'a, I>(feature: &Feature, path: &Path, neighbors: I) -> Vec<Finding>
where
    I: IntoIterator<Item = &'a Feature>,
{
    let mut read_index = ReadIndex::new();
    read_index.push_feature(feature);
    for n in neighbors {
        if std::ptr::eq(n, feature) {
            continue;
        }
        read_index.push_feature(n);
    }

    feature
        .commands
        .iter()
        .filter_map(|cmd| build_finding(&feature.name, &cmd.name, &cmd.effect, &read_index, path))
        .collect()
}

/// Doctor-side dispatch entry point. Takes pre-extracted `(feature_name,
/// commands, queries)` triples — matches the `Tier3FeatureFacts` shape so
/// the CLI doctor can call this without re-materializing whole `Feature`
/// values.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::correctness::mutation_without_readback::check_from_facts;
/// use lazuli_ir::{Command, Query};
///
/// let cmds: Vec<Command> = vec![];
/// let qs: Vec<Query> = vec![];
/// let _ = check_from_facts("customer", &cmds, &qs, &[], Path::new("customer.lzi"));
/// ```
pub fn check_from_facts(
    feature_name: &str,
    commands: &[Command],
    self_queries: &[Query],
    neighbor_queries: &[(String, &[Query])],
    path: &Path,
) -> Vec<Finding> {
    let mut read_index = ReadIndex::new();
    read_index.push_queries(feature_name, self_queries);
    for (feat, qs) in neighbor_queries {
        if feat == feature_name {
            continue;
        }
        read_index.push_queries(feat, qs);
    }

    commands
        .iter()
        .filter_map(|cmd| build_finding(feature_name, &cmd.name, &cmd.effect, &read_index, path))
        .collect()
}

#[derive(Default)]
struct ReadIndex {
    /// (query_feature, query_name) pairs for Lookup / List queries.
    entries: Vec<(String, String)>,
}

impl ReadIndex {
    fn new() -> Self {
        Self::default()
    }

    fn push_feature(&mut self, feat: &Feature) {
        self.push_queries(&feat.name, &feat.queries);
    }

    fn push_queries(&mut self, feature_name: &str, queries: &[Query]) {
        for query in queries {
            match query {
                Query::Lookup(q) => self.entries.push((feature_name.to_owned(), q.name.clone())),
                Query::List(q) => self.entries.push((feature_name.to_owned(), q.name.clone())),
                Query::Sql(_) => {}
            }
        }
    }

    fn covers(&self, resource_feature: &str, resource_snake: &str) -> bool {
        self.entries.iter().any(|(qfeat, qname)| {
            query_reads_resource(
                Some(qfeat.as_str()),
                qname,
                resource_feature,
                resource_snake,
            )
        })
    }
}

fn build_finding(
    feature_name: &str,
    command_name: &str,
    effect: &CommandEffect,
    read_index: &ReadIndex,
    path: &Path,
) -> Option<Finding> {
    let (effect_kind, resource_qn) = effect_resource(effect)?;
    let resource_display = resource_qn.name.clone();
    let resource_snake = pascal_to_snake(&resource_qn.name);
    let resource_feature = resource_qn
        .feature
        .clone()
        .unwrap_or_else(|| feature_name.to_owned());

    if read_index.covers(&resource_feature, &resource_snake) {
        return None;
    }

    Some(Finding {
        path: path.to_path_buf(),
        feature: feature_name.to_owned(),
        command: command_name.to_owned(),
        effect_kind,
        resource_snake,
        resource_display,
    })
}

fn effect_resource(effect: &CommandEffect) -> Option<(&'static str, &QualifiedName)> {
    match effect {
        CommandEffect::Creates(e) => Some(("creates", &e.resource)),
        CommandEffect::Updates(e) => Some(("updates", &e.resource)),
        CommandEffect::Deletes(e) => Some(("deletes", &e.resource)),
        CommandEffect::Returns(_) | CommandEffect::None => None,
    }
}

/// Does a query named `query_name` (declared in `query_feature`) plausibly
/// read the resource `<resource_feature>.<resource_snake>`?
///
/// Conservative inference: the query name must end with the snake form
/// (`lookup_my_<r>`, `lookup_<r>`, `list_<r>s`, `list_my_<r>s`,
/// `mine_<r>s`, `by_<x>_<r>`, ...) OR equal the snake form exactly.
/// We intentionally do not require a specific prefix — `lookup_active_<r>`
/// and similar author-written shapes are valid read queries too.
///
/// Cross-feature gating: when `resource_feature` is set, only queries in
/// the same feature OR in a feature that `uses` it would normally be
/// allowed to read. Doctor does not have the import graph in this rule
/// (the analyzer enforces import discipline separately), so we accept
/// any neighbor here per the cycle decision and let import checks reject
/// orphans on their own track.
fn query_reads_resource(
    _query_feature: Option<&str>,
    query_name: &str,
    _resource_feature: &str,
    resource_snake: &str,
) -> bool {
    if resource_snake.is_empty() {
        return false;
    }
    if query_name == resource_snake {
        return true;
    }
    // Plural list shapes: `mine_customers`, `list_customers`, `<resource>s`
    let plural = format!("{}s", resource_snake);
    if query_name == plural {
        return true;
    }
    // Word-boundary suffix match: `_<r>` or `_<r>s`.
    let underscore_singular = format!("_{}", resource_snake);
    let underscore_plural = format!("_{}s", resource_snake);
    query_name.ends_with(&underscore_singular) || query_name.ends_with(&underscore_plural)
}

/// Local copy of the `Pascal -> snake` converter from the analyzer.
/// We mirror the analyzer's behaviour exactly (acronym handling +
/// digit-after-letter separators) so the inference here matches the
/// `lookup_my_<r>` name that auto-synth emits.
fn pascal_to_snake(s: &str) -> String {
    let mut out = String::new();
    let mut prev: Option<char> = None;
    let mut iter = s.chars().peekable();

    while let Some(ch) = iter.next() {
        if ch.is_ascii_uppercase() {
            let next_is_lower = iter
                .peek()
                .copied()
                .is_some_and(|next| next.is_ascii_lowercase());
            let prev_needs_sep = prev.is_some_and(|p| {
                p.is_ascii_lowercase()
                    || p.is_ascii_digit()
                    || (p.is_ascii_uppercase() && next_is_lower)
            });
            if !out.is_empty() && prev_needs_sep {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
        prev = Some(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lower(source: &str) -> Feature {
        let features = lazuli_syntax::parse_feature_skeletons(source).expect("parse feature");
        lazuli_analyzer::lower_feature_skeleton(&features[0]).expect("lower feature")
    }

    #[test]
    fn mutation_with_matching_lookup_does_not_fire() {
        let feature = lower(
            r#"
feature billing
  domain
    resource Customer
      id: ID required

    query.lookup lookup_my_customer by id: ID

  command update_customer
    route id: ID
    updates Customer
"#,
        );

        let findings = check(&feature, Path::new("billing.lzi"));
        assert!(
            findings.is_empty(),
            "expected no diagnostic when a matching lookup exists, got: {:?}",
            findings
        );
    }

    #[test]
    fn mutation_without_any_read_query_fires_warning() {
        let feature = lower(
            r#"
feature billing
  domain
    resource Customer
      id: ID required

  command update_customer
    route id: ID
    updates Customer
"#,
        );

        let findings = check(&feature, Path::new("billing.lzi"));
        assert_eq!(
            findings.len(),
            1,
            "expected one finding, got: {:?}",
            findings
        );
        let f = &findings[0];
        assert_eq!(f.command, "update_customer");
        assert_eq!(f.effect_kind, "updates");
        assert_eq!(f.resource_snake, "customer");
        assert_eq!(f.resource_display, "Customer");
        assert_eq!(Finding::CODE, "MUTATION-WITHOUT-READBACK-001");
        assert_eq!(Finding::ID, "@correctness.mutation_without_readback");
        let msg = f.message();
        assert!(msg.contains("command 'update_customer'"), "msg: {msg}");
        assert!(msg.contains("resource 'Customer'"), "msg: {msg}");
        assert!(msg.contains("lookup_my_customer"), "msg: {msg}");
        assert!(msg.contains("mine_customers"), "msg: {msg}");
    }

    #[test]
    fn list_query_with_matching_resource_counts_as_readback() {
        let feature = lower(
            r#"
feature billing
  domain
    resource Customer
      id: ID required

    query.list mine_customers

  command create_customer
    creates Customer
"#,
        );

        let findings = check(&feature, Path::new("billing.lzi"));
        assert!(
            findings.is_empty(),
            "list query should satisfy readback: {:?}",
            findings
        );
    }

    #[test]
    fn delete_without_readback_fires() {
        let feature = lower(
            r#"
feature billing
  domain
    resource Customer
      id: ID required

  command delete_customer
    route id: ID
    deletes Customer
"#,
        );

        let findings = check(&feature, Path::new("billing.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].effect_kind, "deletes");
    }

    #[test]
    fn returns_command_without_query_does_not_fire() {
        // `command compute_total returns Money` is a pure read shape; it has
        // no `creates/updates/deletes` effect, so the rule does not apply.
        let feature = lower(
            r#"
feature billing
  domain
    resource Customer
      id: ID required

  command compute_total
    returns Money
"#,
        );

        assert!(check(&feature, Path::new("billing.lzi")).is_empty());
    }

    #[test]
    fn cross_feature_lookup_satisfies_readback() {
        let owner = lower(
            r#"
feature billing
  domain
    resource Customer
      id: ID required

  command update_customer
    route id: ID
    updates Customer
"#,
        );

        let reader = lower(
            r#"
feature crm
  uses billing
  domain
    query.lookup lookup_my_customer by id: ID
"#,
        );

        let findings = check_with_neighbors(&owner, Path::new("billing.lzi"), [&reader]);
        assert!(
            findings.is_empty(),
            "cross-feature lookup_my_customer should satisfy readback: {:?}",
            findings
        );
    }

    #[test]
    fn sql_query_does_not_satisfy_readback() {
        // SQL queries are not cache-reachable for `invalidates` wiring;
        // they intentionally do not count toward the readback test.
        let feature = lower(
            r#"
feature billing
  domain
    resource Customer
      id: ID required

    query.sql search_customers
      returns CustomerSearchHit
      sql "./queries/search.sql"

  command update_customer
    route id: ID
    updates Customer
"#,
        );

        let findings = check(&feature, Path::new("billing.lzi"));
        assert_eq!(
            findings.len(),
            1,
            "sql query must NOT satisfy readback: {:?}",
            findings
        );
    }
}
