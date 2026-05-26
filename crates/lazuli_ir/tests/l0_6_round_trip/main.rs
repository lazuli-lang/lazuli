//! L0 #6 — terminal, search, sort, selection, setting, drawer, on-success,
//! auth-rotation, route-guard, and lifecycle-gate serde round-trips.
//!
//! Previously lived inline as `mod l0_6_ir_tests` in `lazuli_ir/src/lib.rs`;
//! promoted to an integration test by Wave R3-F Stage 3 to shrink the lib
//! root toward the ≤ 1000 LOC gold-standard target. These tests only touch
//! the public `lazuli_ir` ABI surface — they belong at the integration
//! layer.
//!
//! Wave R10-C split this single-file crate into per-concern sub-modules to
//! keep every file ≤ 500 LOC.

use serde::Serialize;
use serde::de::DeserializeOwned;

use lazuli_ir::{CommandRef, QueryKind, QueryRef};

mod auth_rotation;
mod route_guards_lifecycle;
mod terminal;

pub(crate) fn round_trip<T>(value: &T)
where
    T: Clone + PartialEq + std::fmt::Debug + Serialize + DeserializeOwned,
{
    let encoded = serde_json::to_string(value).expect("serialize IR value");
    let decoded: T = serde_json::from_str(&encoded).expect("deserialize IR value");
    assert_eq!(*value, decoded);
}

pub(crate) fn query_ref(name: &str) -> QueryRef {
    QueryRef {
        feature: "item".to_string(),
        kind: QueryKind::Lookup,
        name: name.to_string(),
    }
}

pub(crate) fn command_ref(name: &str) -> CommandRef {
    CommandRef {
        feature: "item".to_string(),
        name: name.to_string(),
    }
}
