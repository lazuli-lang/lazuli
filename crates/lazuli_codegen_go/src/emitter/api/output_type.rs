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
        _ => types::go_type_for(type_ref, ctx),
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
