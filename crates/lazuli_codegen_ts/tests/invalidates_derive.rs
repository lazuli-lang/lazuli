//! Cell A5 — TS codegen derives `defineCommand` invalidates from the
//! feature's queries whenever the runtime spec leaves `invalidates`
//! empty.
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
//!  * explicit `invalidates` on the spec wins (back-compat for the
//!    JSON manifest path that supplies cross-feature targets).

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

/// Back-compat — when the runtime spec carries an explicit
/// `invalidates` list (e.g. cross-feature targets supplied via the
/// JSON manifest path), the emitter honours it verbatim instead of
/// overriding with the derived set.
#[test]
fn explicit_invalidates_wins_over_derivation() {
    let mut command = mutating_command("save_host", RuntimeEffect::UpdatesByID);
    command.invalidates = vec!["other_feature.list_things".to_owned()];

    let feature = RuntimeFeature {
        name: "host".to_owned(),
        source_path: "features/host/host.lzi".to_owned(),
        resources: vec![resource("host")],
        commands: vec![command],
        queries: vec![lookup_query("lookup_my_host")],
    };

    let out = emit_feature_ts(&feature);
    assert!(
        out.contains("invalidates: [\"other_feature.list_things\"],"),
        "explicit invalidates should win; got:\n{out}"
    );
    // The derived target would have produced `invalidates: ["host.lookup_my_host"]`.
    // That literal must NOT appear; the explicit list above replaces it.
    assert!(
        !out.contains("invalidates: [\"host.lookup_my_host\"]"),
        "derived set should not leak when explicit list is present; got:\n{out}"
    );
}
