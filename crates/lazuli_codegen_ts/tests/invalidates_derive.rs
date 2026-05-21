//! Cell A5 — TS codegen derives `defineCommand` invalidates from the
//! feature's queries while preserving author-declared invalidation
//! targets.
//!
//! Closes the cache-correctness bug where every emitted SDK command
//! shipped `invalidates: []`, forcing pilots to set `staleTime: 0` as
//! a TanStack Query workaround. Algorithm: for any mutating effect
//! (`CreatesFromInput` / `UpdatesByID` / `DeletesByID`), the affected
//! resource is the feature's single resource — so the invalidation
//! target set is every query declared on the same feature, qualified
//! as `<feature>.<short_name>`. Cell B1 (codegen-correctness-cycle-
//! 2026-05-21) dropped the historical `.query.` infix because the
//! `/q/` HTTP prefix already disambiguates kind.
//!
//! Coverage matrix:
//!  * 0 matching queries → empty array (no spurious entries).
//!  * 1 query → one element.
//!  * multiple queries → all elements, in declared order.
//!  * author-declared invalidates are normalized to `<feature>.<query>`.
//!  * author-declared invalidates are emitted before derived entries.
//!  * overlap between author-declared and derived entries is deduped.

use lazuli_codegen_spec::{
    FieldKind, QueryKind, RuntimeArg, RuntimeCommand, RuntimeEffect, RuntimeFeature, RuntimeField,
    RuntimeInput, RuntimeQuery, RuntimeResource, Tenancy,
};
use lazuli_codegen_ts::emit_feature_ts;

fn resource(name: &str) -> RuntimeResource {
    RuntimeResource {
        name: name.to_owned(),
        tenancy: Tenancy::Org,
        soft_delete: false,
        retention: None,
        fields: vec![RuntimeField {
            name: "label".to_owned(),
            kind: FieldKind::Text,
        }],
    }
}

fn mutating_command(short_name: &str, effect: RuntimeEffect) -> RuntimeCommand {
    let inputs = match effect {
        RuntimeEffect::CreatesFromInput => vec![RuntimeInput {
            field_name: "Label".to_owned(),
            kind: FieldKind::Text,
        }],
        RuntimeEffect::UpdatesByID | RuntimeEffect::DeletesByID => vec![RuntimeInput {
            field_name: "ID".to_owned(),
            kind: FieldKind::Integer,
        }],
    };
    RuntimeCommand {
        short_name: short_name.to_owned(),
        policy_name: String::new(),
        policy_atoms: Vec::new(),
        rate_limit: String::new(),
        validators: Vec::new(),
        effect,
        inputs,
        emits: Vec::new(),
        invalidates: Vec::new(),
        deprecated: None,
    }
}

fn lookup_query(short_name: &str) -> RuntimeQuery {
    RuntimeQuery {
        short_name: short_name.to_owned(),
        kind: QueryKind::Lookup,
        policy_name: String::new(),
        policy_atoms: Vec::new(),
        args: vec![RuntimeArg {
            field_name: "ID".to_owned(),
            kind: FieldKind::Integer,
            optional: false,
        }],
        cache: None,
        paginate: 0,
        filters: Vec::new(),
        search: None,
        lookup_by: Vec::new(),
    }
}

fn list_query(short_name: &str) -> RuntimeQuery {
    RuntimeQuery {
        short_name: short_name.to_owned(),
        kind: QueryKind::List,
        policy_name: String::new(),
        policy_atoms: Vec::new(),
        args: Vec::new(),
        cache: None,
        paginate: 0,
        filters: Vec::new(),
        search: None,
        lookup_by: Vec::new(),
    }
}

/// Edge case — no queries on the feature → emitted `invalidates: []`.
/// Locks the contract that derivation never invents targets.
#[test]
fn derives_empty_array_when_feature_has_no_queries() {
    let feature = RuntimeFeature {
        name: "host".to_owned(),
        source_path: "features/host/host.lzi".to_owned(),
        resources: vec![resource("host")],
        commands: vec![mutating_command("save", RuntimeEffect::UpdatesByID)],
        queries: Vec::new(),
    };

    let out = emit_feature_ts(&feature);
    assert!(
        out.contains("invalidates: [],"),
        "expected empty invalidates array; got:\n{out}"
    );
}

/// Single matching query — `command save_host effect updates host`
/// pairs with `query lookup_my_host` and the emitted SDK lists that
/// one query as the invalidation target.
#[test]
fn derives_single_query_target() {
    let feature = RuntimeFeature {
        name: "host".to_owned(),
        source_path: "features/host/host.lzi".to_owned(),
        resources: vec![resource("host")],
        commands: vec![mutating_command("save_host", RuntimeEffect::UpdatesByID)],
        queries: vec![lookup_query("lookup_my_host")],
    };

    let out = emit_feature_ts(&feature);
    assert!(
        out.contains("invalidates: [\"host.lookup_my_host\"],"),
        "expected single-element invalidates array; got:\n{out}"
    );
}

/// Multiple queries — every query on the feature appears in the
/// emitted array, in declared order, for any mutating effect.
#[test]
fn derives_all_queries_for_mutating_command() {
    let feature = RuntimeFeature {
        name: "host".to_owned(),
        source_path: "features/host/host.lzi".to_owned(),
        resources: vec![resource("host")],
        commands: vec![
            mutating_command("create_host", RuntimeEffect::CreatesFromInput),
            mutating_command("save_host", RuntimeEffect::UpdatesByID),
            mutating_command("archive_host", RuntimeEffect::DeletesByID),
        ],
        queries: vec![lookup_query("lookup_my_host"), list_query("list_hosts")],
    };

    let out = emit_feature_ts(&feature);
    // Each mutating command picks up both queries, in declared order.
    let expected = "invalidates: [\"host.lookup_my_host\", \"host.list_hosts\"],";
    let occurrences = out.matches(expected).count();
    assert_eq!(
        occurrences, 3,
        "expected derived invalidates on all three mutating commands; got:\n{out}"
    );
}

/// Author-declared same-feature shorthand — `invalidates query.foo`
/// normalizes to the post-B1 SDK wire key `<feature>.foo`. With no
/// feature queries, the auto-derived side contributes nothing.
#[test]
fn author_declared_same_feature_invalidates_emit_without_derivation() {
    let mut command = mutating_command("save_host", RuntimeEffect::UpdatesByID);
    command.invalidates = vec!["query.foo".to_owned()];

    let feature = RuntimeFeature {
        name: "host".to_owned(),
        source_path: "features/host/host.lzi".to_owned(),
        resources: vec![resource("host")],
        commands: vec![command],
        queries: Vec::new(),
    };

    let out = emit_feature_ts(&feature);
    assert!(
        out.contains("invalidates: [\"host.foo\"],"),
        "author-declared same-feature target should normalize; got:\n{out}"
    );
}

/// Author-declared + derived overlap — author order wins, but the
/// overlapping derived target is not emitted a second time.
#[test]
fn author_declared_and_derived_overlap_dedups_by_qualified_name() {
    let mut command = mutating_command("save_host", RuntimeEffect::UpdatesByID);
    command.invalidates = vec!["query.lookup_my_host".to_owned()];

    let feature = RuntimeFeature {
        name: "host".to_owned(),
        source_path: "features/host/host.lzi".to_owned(),
        resources: vec![resource("host")],
        commands: vec![command],
        queries: vec![lookup_query("lookup_my_host"), list_query("list_hosts")],
    };

    let out = emit_feature_ts(&feature);
    assert!(
        out.contains("invalidates: [\"host.lookup_my_host\", \"host.list_hosts\"],"),
        "overlap should dedupe while preserving author-first order; got:\n{out}"
    );
    assert!(
        !out.contains("\"host.lookup_my_host\", \"host.lookup_my_host\""),
        "overlapping target should not be duplicated; got:\n{out}"
    );
}

/// Cross-feature author-declared + same-feature derived — the legacy
/// `bar.query.baz` marker loses the `.query.` infix, then derived
/// same-feature entries are appended.
#[test]
fn cross_feature_author_declared_invalidates_merge_before_same_feature_derived() {
    let mut command = mutating_command("save_host", RuntimeEffect::UpdatesByID);
    command.invalidates = vec!["bar.query.baz".to_owned()];

    let feature = RuntimeFeature {
        name: "host".to_owned(),
        source_path: "features/host/host.lzi".to_owned(),
        resources: vec![resource("host")],
        commands: vec![command],
        queries: vec![lookup_query("lookup_my_host")],
    };

    let out = emit_feature_ts(&feature);
    assert!(
        out.contains("invalidates: [\"bar.baz\", \"host.lookup_my_host\"],"),
        "cross-feature author target should emit before derived target; got:\n{out}"
    );
}
