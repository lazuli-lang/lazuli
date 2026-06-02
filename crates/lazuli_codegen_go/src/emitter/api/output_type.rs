//! API output Go-type resolution.
//!
//! Wraps `types::go_type_for` with the @cap.File → storage.FileRef
//! rewrite + import registration. Lifted out so the orchestrator in
//! `mod.rs` only sees `go_type_for_api_output` and the import-side
//! helper.

use lazuli_ir::TypeRef;

use super::super::imports::ImportSet;
use super::super::types::{self, TypeCtx};

pub(super) fn go_type_for_api_output(
    type_ref: &TypeRef,
    ctx: &TypeCtx<'_>,
) -> (String, Option<String>) {
    match type_ref {
        TypeRef::Capability(_) => types::go_type_for(type_ref, ctx),
        TypeRef::Many(inner) => {
            let (inner_go, import) = go_type_for_api_output(inner, ctx);
            (format!("[]{}", inner_go), import)
        }
        TypeRef::UserDefined(qname) if is_cap_file_literal(&qname.name) => storage_file_ref_type(),
        TypeRef::Unresolved(raw) if is_cap_file_literal(raw) => storage_file_ref_type(),
        // RETURN AXIS — an api `output User` returns the full row, exactly
        // like a `command me returns User`. Use the return-type resolver,
        // NOT `go_type_for`: the latter is the FIELD resolver and collapses
        // a resource reference (`User`) to its FK alias `lazuli.ID` (right
        // for a BIGINT column scan, wrong for the output the handler
        // returns). The collapse silently typed `var meApi =
        // lazuli.Api[MeApiArgs, lazuli.ID]` while the registered `@fn.me`
        // handler returns `accountgen.User`, so the handler bridge's type
        // assertion failed → 500. Mirrors `command/effects.rs`'s
        // `go_return_type_for` on the `Returns` effect.
        _ => types::go_return_type_for(type_ref, ctx),
    }
}

fn storage_file_ref_type() -> (String, Option<String>) {
    (
        "storage.FileRef".to_owned(),
        Some("lazuli.dev/runtime/lazuli/storage".to_owned()),
    )
}

// API outputs may still carry the authored decorator text on older
// analyzer paths; normalize it before the generic identifier sanitizer.
fn is_cap_file_literal(raw: &str) -> bool {
    let raw = raw.trim();
    raw == "@cap.File" || raw.starts_with("@cap.File(")
}

pub(super) fn register_imports_for_api_output(
    type_ref: &TypeRef,
    ctx: &TypeCtx<'_>,
    imports: &mut ImportSet,
) {
    let (_go, import) = go_type_for_api_output(type_ref, ctx);
    if let Some(path) = import {
        imports.add(&path);
    }
    if let TypeRef::Many(inner) = type_ref {
        register_imports_for_api_output(inner, ctx, imports);
    }
}
