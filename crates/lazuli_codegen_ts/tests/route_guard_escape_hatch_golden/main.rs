//! Snapshot coverage for the 3 new route-guard escape-hatch slots
//! shipped under `docs/proposals/ir-route-guard-escape-hatch-2026-05-28.md`
//! Cell B-1. Each pair (`*_emits_*` happy path + `*_back_compat_*`
//! invariant) proves:
//!
//!  1. **Allow-list lifecycle** — `requires_lifecycle_in <R> [s, ...]`
//!     emits an `Array.includes` membership check; existing
//!     `requires_lifecycle <R> = <state>` fixtures emit byte-identical
//!     equality-check TS.
//!  2. **Field gate** — `requires <feature>.lookup_my.<field> = <lit>
//!     on_unmet redirect <path>` emits a fetchQuery + mismatch redirect;
//!     routes without the slot emit unchanged.
//!  3. **`forbid_when ... only_when lifecycle`** — composes the atom
//!     check with a `lifecycleState === <s>` gate; legacy unconditional
//!     `forbid_when` keeps its previous shape.
//!
//! The fixtures are built via `serde_json::from_value` against the IR
//! shapes (same approach as `lifecycle_gate_golden.rs`) so the test
//! stays decoupled from parser surface evolution.

use lazuli_codegen_ts::GeneratedFile;
use lazuli_codegen_ts::routes::{RoutesTarget, emit_routes_artifacts};
use lazuli_ir::{
    AppRoute, Experience, Feature, LifecycleRouteArm, LifecycleRoutes, Platform, PlatformSurface,
};

include!("main_p1.rs");
include!("main_p2.rs");
