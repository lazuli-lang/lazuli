//! Encryption bucket cycle — emit `var EncryptionBindings = ...` plus
//! the `init()` that registers each binding with the runtime registry.
//! The runtime side (`runtime/go/lazuli/encryption/registry.go`)
//! supplies `encryption.Binding`, `encryption.Register`, and the closed
//! catalogs (`SourceEnv`/`SourceSecrets`, `AxisTenantID`/`AxisUserID`/
//! `AxisRecordID`, `AlgorithmAES256GCM`, `RotationManual`).
//!
//! Codegen never names AES, GCM, or any concrete crypto primitive —
//! the runtime resolves the cipher behind the `encryption.Binding`
//! catalog token. Per `docs/proposals/encryption-vocab.md` §Codegen,
//! this emitter is ~50 LOC of `import + call`, not a homegrown crypto
//! envelope.

use lazuli_ir::{
    EncryptionAlgorithm, EncryptionBinding, EncryptionRotation, EncryptionSource,
    EncryptionTemplateAxis,
};

use super::super::patterns::{PATTERN_ENCRYPTION_REGISTER, emit_pattern_header};
use super::super::printer::GoPrinter;
use super::helpers::emit_aligned_struct_value_rows;

pub(super) fn emit_encryption_bindings(p: &mut GoPrinter, bindings: &[EncryptionBinding]) {
    p.line("// EncryptionBindings is the lowered `app.encryption` catalog from app.lzi.");
    p.line("// One entry per `@key.<scope>` referenced by any `@cap.Encrypted` /");
    p.line("// `@cap.E2ee` field. The `init()` below registers each binding with");
    p.line("// the runtime registry so `encryption.For(ctx, \"@key.<scope>\")` resolves");
    p.line("// the per-tenant cipher on demand.");
    p.line("var EncryptionBindings = []encryption.Binding{");
    p.indent();
    for binding in bindings {
        emit_encryption_binding_literal(p, binding);
    }
    p.dedent();
    p.line("}");
    p.blank();
    emit_pattern_header(p, PATTERN_ENCRYPTION_REGISTER);
    p.line("func init() {");
    p.indent();
    p.line("for _, b := range EncryptionBindings {");
    p.indent();
    p.line("encryption.Register(b)");
    p.dedent();
    p.line("}");
    p.dedent();
    p.line("}");
}

fn emit_encryption_binding_literal(p: &mut GoPrinter, binding: &EncryptionBinding) {
    p.line("{");
    p.indent();
    let mut rows: Vec<(String, String)> = Vec::new();
    rows.push(("Scope:".to_owned(), format!("{:?},", binding.scope)));
    let (source_const, template) = match &binding.source {
        EncryptionSource::Env(t) => ("encryption.SourceEnv", t),
        EncryptionSource::Secrets(t) => ("encryption.SourceSecrets", t),
    };
    rows.push(("Source:".to_owned(), format!("{},", source_const)));
    rows.push(("Template:".to_owned(), format!("{:?},", template.literal)));
    let axis_consts: Vec<&'static str> = template
        .axes
        .iter()
        .map(|axis| encryption_axis_const(*axis))
        .collect();
    rows.push((
        "Axes:".to_owned(),
        format!("[]encryption.TemplateAxis{{{}}},", axis_consts.join(", ")),
    ));
    rows.push((
        "Algorithm:".to_owned(),
        format!("{},", encryption_algorithm_const(binding.algorithm)),
    ));
    rows.push((
        "Rotation:".to_owned(),
        format!("{},", encryption_rotation_const(binding.rotation)),
    ));
    emit_aligned_struct_value_rows(p, &rows);
    p.dedent();
    p.line("},");
}

fn encryption_axis_const(axis: EncryptionTemplateAxis) -> &'static str {
    match axis {
        EncryptionTemplateAxis::TenantId => "encryption.AxisTenantID",
        EncryptionTemplateAxis::UserId => "encryption.AxisUserID",
        EncryptionTemplateAxis::RecordId => "encryption.AxisRecordID",
    }
}

fn encryption_algorithm_const(algorithm: EncryptionAlgorithm) -> &'static str {
    match algorithm {
        EncryptionAlgorithm::Aes256Gcm => "encryption.AlgorithmAES256GCM",
    }
}

fn encryption_rotation_const(rotation: EncryptionRotation) -> &'static str {
    match rotation {
        EncryptionRotation::Manual => "encryption.RotationManual",
    }
}
