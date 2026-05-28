//! Cross-feature contract lints per
//! `docs/proposals/cross-feature-contracts.md`.
//!
//! Each rule is gated on `architecture mode microservices` — capsules
//! under `monolith` / `modular_monolith` see no findings from these rules.
//! Under `microservices` the rules enforce the language-level contract
//! invariant that every cross-feature symbol reference must be expressible
//! as a `public contract` declaration in the origin feature.
//!
//! v0.1 catalog (3 rules):
//!
//! - `CROSS-FEATURE-CONTRACT-MISSING-001` (error) — cross-feature
//!   reference resolves to a symbol without `public contract`.
//! - `CROSS-FEATURE-CONTRACT-VERSION-DRIFT-001` (error) — consumer pinned
//!   via `uses <feature> version v<N>` references a symbol whose origin
//!   publishes a different `public contract <Symbol> as v<M>` version.
//! - `CROSS-FEATURE-WORKFLOW-SPAN-001` (warning) — workflow transitions
//!   touch resources owned by multiple features.

pub mod contract_missing_001;
pub mod polymorphic_target_001;
pub mod ref_unknown_001;
pub mod version_drift_001;
pub mod workflow_span_001;
