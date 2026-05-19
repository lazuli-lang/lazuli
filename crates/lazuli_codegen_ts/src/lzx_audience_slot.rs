//! L0 #3 `.lzx` integration codegen — Cells C.1 + C.2.
//!
//! - **Cell C.1** (`audience_sdk`): audience-scoped SDK projection per
//!   `docs/proposals/lzx-integration-codegen.md` §7. Computes which
//!   commands/queries are reachable from a given set of audience
//!   declarations and emits a filtered `<feat>.gen.ts`. The filter is
//!   compile-time: a `public` bundle simply does not export
//!   `deleteSlug` because the policy intersection misses, so any
//!   import attempt is a TypeScript error.
//!
//! - **Cell C.2** (`slot_interface`): slot props interface emitter per
//!   §8. For each `cells <field> @client.<slot>` binding, the
//!   generated `*Props` interface declares `value: Resource["field"]`
//!   and `row: Resource`, deriving the TS type by index access. For
//!   section slots (no field axis), `value: void` is emitted.
//!
//! This module owns a **local `.lzx` IR stub** matching the shape in
//! L0 #3 §6. The dedicated parser cell publishes types of identical
//! shape into `lazuli_ir`; once that lands, the stub is deleted and
//! consumers re-point to `lazuli_ir::Surface` / `Audience` / `View*`.
//! Until then this stub keeps Cells C.1 + C.2 testable end-to-end
//! under `cargo test -p lazuli_codegen_ts`.

#[path = "audience_sdk.rs"]
pub mod audience_sdk;

#[path = "slot_interface.rs"]
pub mod slot_interface;

pub use audience_sdk::{
    AudienceProjection, LifecycleGateIntegration, LifecycleGateTarget, RouteGuardTarget,
    compute_audience_projection, emit_feature_sdk_filtered, emit_lifecycle_gate_artifacts,
    emit_lifecycle_gate_artifacts_from_json, emit_route_guard_artifacts,
};
pub use slot_interface::emit_slot_interface;

// ---------------------------------------------------------------------------
// Local `.lzx` IR stub — replaced by `lazuli_ir::Surface` once the parser
// cell lands. Shapes are taken verbatim from L0 #3 §6.
// ---------------------------------------------------------------------------

pub mod ir {
    //! Canonical .lzx IR types — re-exported from lazuli_ir.
    //! (Original stubs replaced after Cell A.1+A.2+A.3 landed in 235d7a7.)
    pub use lazuli_ir::{
        Surface, SurfaceTarget, Audience, View, ViewList, ViewDetail, ViewCreate,
        QueryRef, QueryKind, CommandRef, CellBinding, RouteParam, PolicyAtom,
    };
}

pub use ir::*;

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Convert a snake_case / kebab-case / mixed identifier to PascalCase.
/// Acronyms (`id`, `url`, `api`, ...) are preserved as fully-uppercase
/// segments to match the runtime emitter's casing conventions.
pub(crate) fn pascal_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for word in s.split(|c: char| c == '_' || c == '-' || c == ' ') {
        if word.is_empty() {
            continue;
        }
        if is_acronym(word) {
            out.push_str(&word.to_ascii_uppercase());
            continue;
        }
        let mut chars = word.chars();
        if let Some(first) = chars.next() {
            for u in first.to_uppercase() {
                out.push(u);
            }
        }
        out.push_str(&chars.as_str().to_ascii_lowercase());
    }
    out
}

fn is_acronym(word: &str) -> bool {
    matches!(
        word.to_ascii_lowercase().as_str(),
        "id" | "url" | "uri" | "api" | "html" | "json" | "sql" | "ttl"
    )
}

#[cfg(test)]
mod helper_tests {
    use super::pascal_case;

    #[test]
    fn pascal_case_snake_to_pascal() {
        assert_eq!(pascal_case("type_badge"), "TypeBadge");
    }

    #[test]
    fn pascal_case_kebab_to_pascal() {
        assert_eq!(pascal_case("workspace-admin"), "WorkspaceAdmin");
    }

    #[test]
    fn pascal_case_acronym_uppercase() {
        assert_eq!(pascal_case("api_key"), "APIKey");
    }
}
