//! HOOK-TARGET-001 — `hook <name>: Hook[X]` where X is not declared in the
//! feature or its `uses` imports.
//!
//! Fires when `hook <name>: Hook[<TargetName>]` is declared but `<TargetName>`
//! cannot be resolved against the feature's commands, jobs, or records, nor
//! against any symbol exported by a `uses <feature>` import.
//!
//! Severity: `error` strict-profile AND `error` production-profile.
//! This is a correctness gate — dangling target = unresolvable codegen.
//! Reference: docs/extension-points.md:33, docs/canonical-semantics.md:2432

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use lazuli_ir::{ExtensionContract, Feature, TypeRef};

// ── output ────────────────────────────────────────────────────────────────────

/// One HOOK-TARGET-001 finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub path: PathBuf,
    pub feature: String,
    pub hook_name: String,
    pub missing_target: String,
    /// Levenshtein-≤3 suggestions from all declared command/job/record names
    /// in scope (current feature + used features).
    pub suggestions: Vec<String>,
}

impl Finding {
    pub const CODE: &'static str = "HOOK-TARGET-001";

    pub fn message(&self) -> String {
        let base = format!(
            "hook `{}` in feature `{}` references `Hook[{}]` but `{}` is not declared \
             as a command, job, or record in this feature or its `uses` imports.",
            self.hook_name, self.feature, self.missing_target, self.missing_target,
        );
        if self.suggestions.is_empty() {
            base
        } else {
            format!("{} Did you mean: {}?", base, self.suggestions.join(", "))
        }
    }
}

// ── detection ─────────────────────────────────────────────────────────────────

/// Run HOOK-TARGET-001 for all hooks in one feature.
///
/// `path` is the source `.lzi` file — used to anchor findings; no I/O is
/// performed here.
///
/// `all_feature_symbols` maps each known feature name to the set of
/// command/job/record names it exports. The caller (doctor walker) builds
/// this from the full loaded set of features so that cross-feature
/// resolution via `uses <feature>` works correctly.
pub fn check(
    feature: &Feature,
    path: &Path,
    all_feature_symbols: &BTreeMap<String, BTreeSet<String>>,
) -> Vec<Finding> {
    // Build the local symbol set from commands, jobs, and records.
    let mut local_symbols: BTreeSet<String> = BTreeSet::new();
    for cmd in &feature.commands {
        local_symbols.insert(cmd.name.clone());
    }
    for job in &feature.jobs {
        local_symbols.insert(job.name.clone());
    }
    for rec in &feature.records {
        local_symbols.insert(rec.name.clone());
    }

    // Collect all in-scope symbols (local + used features).
    let mut all_in_scope: BTreeSet<String> = local_symbols.clone();
    for used_feature in &feature.uses {
        if let Some(symbols) = all_feature_symbols.get(used_feature.as_str()) {
            all_in_scope.extend(symbols.iter().cloned());
        }
    }

    let mut findings = Vec::new();

    for ext in &feature.extensions {
        let type_arg = match &ext.contract {
            ExtensionContract::Hook { type_arg } => type_arg,
            _ => continue,
        };

        let target_name = match type_arg {
            TypeRef::UserDefined(qn) => &qn.name,
            TypeRef::Unresolved(s) => s,
            _ => continue,
        };

        if all_in_scope.contains(target_name.as_str()) {
            continue;
        }

        let suggestions = nearest_matches(target_name, &all_in_scope);

        findings.push(Finding {
            path: path.to_path_buf(),
            feature: feature.name.clone(),
            hook_name: ext.name.clone(),
            missing_target: target_name.clone(),
            suggestions,
        });
    }

    findings
}

// ── internals ─────────────────────────────────────────────────────────────────

/// Returns all names in `candidates` within Levenshtein distance ≤ 3,
/// sorted lexicographically.
fn nearest_matches(target: &str, candidates: &BTreeSet<String>) -> Vec<String> {
    candidates
        .iter()
        .filter(|c| levenshtein(target, c) <= 3)
        .cloned()
        .collect()
}

/// Classic Wagner-Fischer Levenshtein distance (O(m·n) time, O(min(m,n)) space).
fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let (m, n) = (a.len(), b.len());
    if m == 0 {
        return n;
    }
    if n == 0 {
        return m;
    }
    // Keep only two rows to save memory.
    let mut prev: Vec<usize> = (0..=n).collect();
    let mut curr = vec![0usize; n + 1];
    for i in 1..=m {
        curr[0] = i;
        for j in 1..=n {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            curr[j] = (prev[j] + 1)
                .min(curr[j - 1] + 1)
                .min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[n]
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        BuiltinType, Command, CommandEffect, CommandInput, CommandKind, Defaults, Extension,
        ExtensionContract, Feature, Job, JobBody, JobHandler, JobTrigger, PathRef, PathSource,
        Policies, PolicyRef, QualifiedName, Record, ReturnsEffect, TypeRef,
    };

    fn empty_feature(name: &str) -> Feature {
        Feature {
            name: name.to_owned(),
            purpose: None,
            non_goals: vec![],
            context_path: None,
            defaults: Defaults::default(),
            uses: vec![],
            requirements: vec![],
            enums: vec![],
            resources: vec![],
            events: vec![],
            rules: vec![],
            policies: Policies::default(),
            commands: vec![],
            apis: vec![],
            records: vec![],
            queries: vec![],
            workflows: vec![],
            jobs: vec![],
            webhooks: vec![],
            notifications: vec![],
            event_groups: vec![],
            tenant_migrations: vec![],
            translation: None,
            auth: None,
            surfaces: vec![],
            extensions: vec![],
            escape_routes: vec![],
            agents: vec![],
            previous_names: vec![],
            span_ref: None,
        }
    }

    fn hook_ext(name: &str, target: &str) -> Extension {
        Extension {
            name: name.to_owned(),
            contract: ExtensionContract::Hook {
                type_arg: TypeRef::UserDefined(QualifiedName {
                    feature: None,
                    name: target.to_owned(),
                }),
            },
            resolved_path: PathRef {
                path: format!("./handlers/{name}.go"),
                source: PathSource::Convention,
            },
            previous_names: vec![],
            span_ref: None,
        }
    }

    fn mk_command(name: &str) -> Command {
        Command {
            name: name.to_owned(),
            kind: CommandKind::Returns,
            route: vec![],
            input: CommandInput::Empty,
            target: None,
            lets: vec![],
            effect: CommandEffect::Returns(ReturnsEffect {
                return_type: TypeRef::Builtin(BuiltinType::Boolean),
            }),
            policy: PolicyRef::None,
            emits: vec![],
            rate_limit: None,
            audit: None,
            approval: None,
            invalidates: vec![],
            external_calls: vec![],
            timeout: None,
            retry: None,
            idempotency: None,
            deprecated: None,
            tests: None,
            previous_names: vec![],
            span_ref: None,
        }
    }

    fn mk_job(name: &str) -> Job {
        Job {
            name: name.to_owned(),
            trigger: JobTrigger::Schedule { cron: "0 * * * *".to_owned() },
            queue: None,
            idempotency: None,
            retry: None,
            policy: None,
            tenant_from: None,
            fanout: None,
            timeout: None,
            external_calls: vec![],
            body: JobBody::Handler(JobHandler {
                path: PathRef {
                    path: "./handlers/job.go".to_owned(),
                    source: PathSource::Convention,
                },
                returns: None,
            }),
            emits: vec![],
            previous_names: vec![],
            span_ref: None,
        }
    }

    fn mk_record(name: &str) -> Record {
        Record {
            name: name.to_owned(),
            fields: vec![],
            discriminator_field: None,
            span_ref: None,
        }
    }

    fn no_cross_features() -> BTreeMap<String, BTreeSet<String>> {
        BTreeMap::new()
    }

    // ── positive: hook with missing target fires ───────────────────────────────

    #[test]
    fn positive_dangling_hook_fires() {
        let mut feature = empty_feature("customer_metrics");
        feature.extensions.push(hook_ext("refresh_ltv", "RefreshCustomerLtv"));

        let findings = check(&feature, Path::new("features/customer_metrics.lzi"), &no_cross_features());
        assert_eq!(findings.len(), 1, "expected one finding for dangling hook target");
        let f = &findings[0];
        assert_eq!(f.hook_name, "refresh_ltv");
        assert_eq!(f.missing_target, "RefreshCustomerLtv");
        assert_eq!(f.feature, "customer_metrics");
        assert_eq!(Finding::CODE, "HOOK-TARGET-001");
        assert!(
            f.message().contains("RefreshCustomerLtv"),
            "message should name the missing target"
        );
    }

    // ── negative (i): same-feature command target does not fire ───────────────

    #[test]
    fn negative_i_same_feature_command_does_not_fire() {
        let mut feature = empty_feature("customer_metrics");
        feature.commands.push(mk_command("RefreshCustomerLtv"));
        feature.extensions.push(hook_ext("refresh_ltv", "RefreshCustomerLtv"));

        let findings = check(&feature, Path::new("f.lzi"), &no_cross_features());
        assert!(
            findings.is_empty(),
            "hook targeting a same-feature command must not fire HOOK-TARGET-001"
        );
    }

    // ── negative (ii): cross-feature target via `uses` does not fire ──────────

    #[test]
    fn negative_ii_cross_feature_uses_does_not_fire() {
        let mut feature = empty_feature("customer_metrics");
        feature.uses.push("customer".to_owned());
        feature.extensions.push(hook_ext("refresh_ltv", "RefreshCustomerLtv"));

        // Simulate the used feature exporting `RefreshCustomerLtv`.
        let mut cross: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        cross.insert("customer".to_owned(), {
            let mut s = BTreeSet::new();
            s.insert("RefreshCustomerLtv".to_owned());
            s
        });

        let findings = check(&feature, Path::new("f.lzi"), &cross);
        assert!(
            findings.is_empty(),
            "hook targeting a cross-feature symbol via `uses` must not fire HOOK-TARGET-001"
        );
    }

    // ── suggestion: near-miss name appears in suggestions ─────────────────────

    #[test]
    fn suggestions_include_near_miss() {
        let mut feature = empty_feature("billing");
        // "RefreshLtv" is Levenshtein distance 7 from "RefreshCustomerLtv" —
        // use a closer name so it surfaces in suggestions.
        feature.commands.push(mk_command("RefreshCustomrLtv")); // typo, distance 1
        feature.extensions.push(hook_ext("refresh_ltv", "RefreshCustomerLtv"));

        let findings = check(&feature, Path::new("f.lzi"), &no_cross_features());
        assert_eq!(findings.len(), 1);
        assert!(
            findings[0].suggestions.contains(&"RefreshCustomrLtv".to_owned()),
            "near-miss command should appear in suggestions"
        );
        assert!(
            findings[0].message().contains("Did you mean"),
            "message should include suggestion prompt"
        );
    }

    // ── levenshtein unit tests ─────────────────────────────────────────────────

    #[test]
    fn levenshtein_identity_is_zero() {
        assert_eq!(levenshtein("foo", "foo"), 0);
    }

    #[test]
    fn levenshtein_empty_strings() {
        assert_eq!(levenshtein("", "abc"), 3);
        assert_eq!(levenshtein("abc", ""), 3);
        assert_eq!(levenshtein("", ""), 0);
    }

    #[test]
    fn levenshtein_one_insertion() {
        assert_eq!(levenshtein("kitten", "sitten"), 1);
    }

    // ── job and record targets also satisfy the rule ───────────────────────────

    #[test]
    fn negative_same_feature_job_does_not_fire() {
        let mut feature = empty_feature("billing");
        feature.jobs.push(mk_job("ProcessInvoice"));
        feature.extensions.push(hook_ext("process_hook", "ProcessInvoice"));
        assert!(
            check(&feature, Path::new("f.lzi"), &no_cross_features()).is_empty(),
            "hook targeting a same-feature job must not fire"
        );
    }

    #[test]
    fn negative_same_feature_record_does_not_fire() {
        let mut feature = empty_feature("billing");
        feature.records.push(mk_record("InvoiceRecord"));
        feature.extensions.push(hook_ext("invoice_hook", "InvoiceRecord"));
        assert!(
            check(&feature, Path::new("f.lzi"), &no_cross_features()).is_empty(),
            "hook targeting a same-feature record must not fire"
        );
    }
}
