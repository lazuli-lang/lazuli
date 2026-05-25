//! Encryption helpers — emit `Encrypt<Pascal>` / `Decrypt<Pascal>` and
//! the matching `Resource[T]` value fields for resources carrying at
//! least one `@cap.Encrypted` / `@cap.E2ee` field.
//!
//! Proposal: `docs/proposals/encryption-vocab.md` §Codegen.
//!
//! The Encrypt helper covers every encrypted field; Decrypt skips
//! `@cap.E2ee` per rule 3 (the server stores ciphertext but cannot
//! decrypt — the client holds the only key). Required fields use a
//! `len(row.X) > 0` guard; optional fields guard the nil pointer first
//! per rule 4 so unset values never round-trip through the cipher.
//!
//! Boundary: this module owns the cipher-related Go shape (call sites
//! into `encryption.ForCtx`). It does NOT know any crypto primitive —
//! the runtime resolves the cipher per request from the `@key.<scope>`
//! reference threaded through as a string literal.

use lazuli_ir::{CapabilityRef, Field, Resource, TypeRef};

use crate::emitter::casing::pascal_case;
use crate::emitter::printer::GoPrinter;

use super::write_section_banner;

/// Field-encryption metadata extracted from `@cap.Encrypted` /
/// `@cap.E2ee` field decorators. The codegen threads `key` (the
/// `@key.<scope>` reference) through to `encryption.ForCtx(...)` at
/// the SQL boundary; `e2ee` flags fields the server stores but cannot
/// later decrypt — those get an Encrypt call site but are skipped on
/// Decrypt per proposal §Codegen rule 3.
pub(super) struct EncryptedFieldRef<'a> {
    pub(super) field: &'a Field,
    pub(super) key: &'a str,
    pub(super) e2ee: bool,
}

/// Iterator over a resource's `@cap.Encrypted` / `@cap.E2ee` fields,
/// preserving the IR `Vec` order so emission is deterministic. Used
/// both for the import-side gate (`imports.add("…/encryption")`) and
/// for the helper-function body.
pub(super) fn encrypted_fields(resource: &Resource) -> impl Iterator<Item = EncryptedFieldRef<'_>> {
    resource
        .fields
        .iter()
        .filter_map(|field| match &field.type_ref {
            TypeRef::Capability(CapabilityRef::Encrypted(cap)) => Some(EncryptedFieldRef {
                field,
                key: cap.key.as_str(),
                e2ee: false,
            }),
            TypeRef::Capability(CapabilityRef::E2ee(cap)) => Some(EncryptedFieldRef {
                field,
                key: cap.key.as_str(),
                e2ee: true,
            }),
            _ => None,
        })
}

/// Emit the `EncryptedColumns` + `Decrypt` fields on a `Resource[T]`
/// value literal. Both fields are typed against the runtime's
/// `Resource[T]` shape; the runtime uses them to wire the call sites
/// in `applyCreates`/`applyUpdates`/`applyDeletes`, `RunList`, and
/// `RunLookup`. Decrypt is a typed wrapper around the generated
/// `Decrypt<Pascal>` helper so the callback's `any` parameter type
/// matches the runtime's `func(*lazuli.Ctx, any) error` field while
/// the body stays typed against `*<Pascal>`.
pub(super) fn emit_resource_value_encryption_fields(
    p: &mut GoPrinter,
    pascal: &str,
    encrypted: &[EncryptedFieldRef<'_>],
) {
    p.line("EncryptedColumns: map[string]string{");
    p.indent();
    // Walk in declared order — `encrypted_fields` already preserves
    // the IR `Vec` ordering so the emitted map literal is deterministic.
    for entry in encrypted {
        let col = &entry.field.name;
        let key = &entry.key;
        p.line(&format!("\"{col}\": \"{key}\","));
    }
    p.dedent();
    p.line("},");
    p.line(&format!(
        "Decrypt: func(ctx *lazuli.Ctx, row any) error {{ return Decrypt{pascal}(ctx, row.(*{pascal})) }},"
    ));
}

/// Emit `Encrypt<Pascal>` and `Decrypt<Pascal>` helpers for one
/// resource. The helpers operate in-place on `*<Pascal>` (the struct
/// emitted above) so callers can wrap them around their SQL
/// boundary without copying rows.
///
/// Wire-thin: the body of each helper is, per field,
/// `encryption.ForCtx(ctx, "@key.<scope>", "")` then `cipher.Encrypt(...)`
/// or `cipher.Decrypt(...)`. Zero crypto knowledge in codegen.
///
/// Decrypt skips `@cap.E2ee` fields (proposal §Codegen rule 3 — the
/// server stores ciphertext and the user holds the only key). The
/// `_ = ctx` no-op at the top of `Decrypt<Pascal>` keeps the
/// function signature uniform when every field is E2ee and the
/// body is otherwise empty.
pub(super) fn emit_encryption_helpers(p: &mut GoPrinter, resource: &Resource) {
    let pascal = pascal_case(&resource.name);
    let encrypted: Vec<EncryptedFieldRef<'_>> = encrypted_fields(resource).collect();

    write_section_banner(
        p,
        &[
            format!("Encryption helpers: {pascal}"),
            "  one Encrypt/Decrypt call per @cap.Encrypted / @cap.E2ee field".to_owned(),
        ],
    );

    // Encrypt — covers every `@cap.Encrypted` AND `@cap.E2ee` field.
    p.line(&format!(
        "// Encrypt{pascal} ciphers each `@cap.Encrypted` / `@cap.E2ee` field on row in"
    ));
    p.line("// place. Call this immediately before INSERT / UPDATE so the row written");
    p.line("// to the database holds opaque AES-256-GCM bytes (BYTEA column, see");
    p.line("// migration_ddl.rs). The cipher is resolved per request via");
    p.line("// `encryption.ForCtx(ctx, scope, \"\")`; the runtime registry caches the");
    p.line("// derived key under (scope, resolved-template).");
    p.line("//lazuli:pattern resource_encrypt v1");
    p.line(&format!(
        "func Encrypt{pascal}(ctx *lazuli.Ctx, row *{pascal}) error {{"
    ));
    p.indent();
    for entry in &encrypted {
        emit_field_encrypt(p, entry);
    }
    p.line("return nil");
    p.dedent();
    p.line("}");
    p.blank();

    // Decrypt — skips E2ee per proposal §Codegen rule 3.
    let decryptable: Vec<&EncryptedFieldRef<'_>> = encrypted.iter().filter(|e| !e.e2ee).collect();
    p.line(&format!(
        "// Decrypt{pascal} undoes Encrypt{pascal} for every server-readable encrypted"
    ));
    p.line("// field after a SELECT / RETURNING. `@cap.E2ee` fields are intentionally");
    p.line("// skipped (the server stores ciphertext but cannot decrypt — the client");
    p.line("// holds the only key). Callers must check the returned error.");
    p.line("//lazuli:pattern resource_decrypt v1");
    p.line(&format!(
        "func Decrypt{pascal}(ctx *lazuli.Ctx, row *{pascal}) error {{"
    ));
    p.indent();
    if decryptable.is_empty() {
        p.line("// Every encrypted field on this resource is @cap.E2ee; nothing to decrypt.");
        p.line("_ = ctx");
        p.line("_ = row");
    } else {
        for entry in &decryptable {
            emit_field_decrypt(p, entry);
        }
    }
    p.line("return nil");
    p.dedent();
    p.line("}");
}

/// Emit one Encrypt call site. Optional fields are guarded by a
/// `if row.<F> != nil && len(*row.<F>) > 0` check so empty / unset
/// values never round-trip through the cipher (proposal §Codegen
/// rule 4). Required fields use `len(row.<F>) > 0` for the same
/// reason — an unset required field still has the zero `[]byte`
/// header.
fn emit_field_encrypt(p: &mut GoPrinter, entry: &EncryptedFieldRef<'_>) {
    emit_field_cipher_call(p, entry, "Encrypt", "ct");
}

/// Mirror of `emit_field_encrypt` for the read path.
fn emit_field_decrypt(p: &mut GoPrinter, entry: &EncryptedFieldRef<'_>) {
    emit_field_cipher_call(p, entry, "Decrypt", "pt");
}

fn emit_field_cipher_call(
    p: &mut GoPrinter,
    entry: &EncryptedFieldRef<'_>,
    method: &str,
    out_var: &str,
) {
    let pascal_field = pascal_case(&entry.field.name);
    let key_literal = go_str_literal(entry.key);
    let optional = !entry.field.required;
    let (guard, access) = if optional {
        (
            format!("row.{pascal_field} != nil && len(*row.{pascal_field}) > 0"),
            format!("(*row.{pascal_field})"),
        )
    } else {
        (
            format!("len(row.{pascal_field}) > 0"),
            format!("row.{pascal_field}"),
        )
    };
    p.line(&format!("if {guard} {{"));
    p.indent();
    p.line(&format!(
        "cipher, err := encryption.ForCtx(ctx, {key_literal}, \"\")"
    ));
    p.line("if err != nil {");
    p.indent();
    p.line("return err");
    p.dedent();
    p.line("}");
    p.line(&format!("{out_var}, err := cipher.{method}({access})"));
    p.line("if err != nil {");
    p.indent();
    p.line("return err");
    p.dedent();
    p.line("}");
    p.line(&format!("{access} = {out_var}"));
    p.dedent();
    p.line("}");
}

/// Minimal Go string-literal renderer for the `@key.<scope>` key
/// reference. The scope alphabet is `[a-z._@]` so we never hit
/// quote-escaping cases, but we keep the helper conservative.
fn go_str_literal(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            _ => out.push(ch),
        }
    }
    out.push('"');
    out
}
