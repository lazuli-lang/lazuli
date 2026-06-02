//! HTML-sanitization wiring — emit the `SanitizeColumns` field on a
//! `Resource[T]` value literal for resources carrying at least one
//! `validate sanitize_html(<profile>)` field.
//!
//! The DSL `validate sanitize_html(<profile>)` lowers (analyzer) into
//! `FieldConstraints.sanitize_html: Option<SanitizeHtmlProfile>`. This
//! module reads that constraint and emits a `column -> profile` map the
//! runtime walks at the write boundary (`applyCreates` / `applyUpdates`,
//! see `runtime/go/lazuli/sanitize_wire.go`) to rewrite each bound string
//! value through the matching `bluemonday` policy before INSERT/UPDATE.
//!
//! Wire-thin: codegen knows zero HTML; it only threads the profile name
//! (the IR serde form: `strict` / `basic` / `markdown_safe`) through as a
//! string literal so the runtime can resolve the policy. Mirrors the
//! `EncryptedColumns` emit in `encryption.rs`.

use lazuli_ir::{Field, Resource, SanitizeHtmlProfile};

use crate::emitter::printer::GoPrinter;

/// One field carrying `validate sanitize_html(<profile>)`.
pub(super) struct SanitizedFieldRef<'a> {
    pub(super) field: &'a Field,
    pub(super) profile: SanitizeHtmlProfile,
}

/// Iterator over a resource's `sanitize_html` fields, preserving the IR
/// `Vec` order so emission is deterministic. Used both for the
/// resource-value emit and the test that asserts the wiring is present.
pub(super) fn sanitized_fields(resource: &Resource) -> impl Iterator<Item = SanitizedFieldRef<'_>> {
    resource.fields.iter().filter_map(|field| {
        field
            .constraints
            .sanitize_html
            .map(|profile| SanitizedFieldRef { field, profile })
    })
}

/// Render the IR profile enum to the runtime/serde string form. Kept in
/// one place so the analyzer serde, the runtime `SanitizeHTMLProfile`
/// constants, and this codegen agree on the exact spelling.
fn profile_str(profile: SanitizeHtmlProfile) -> &'static str {
    match profile {
        SanitizeHtmlProfile::Strict => "strict",
        SanitizeHtmlProfile::Basic => "basic",
        SanitizeHtmlProfile::MarkdownSafe => "markdown_safe",
    }
}

/// Emit the `SanitizeColumns` field on a `Resource[T]` value literal. The
/// runtime uses it to wire the sanitizer call sites in
/// `applyCreates` / `applyUpdates`. Walk is in declared order so the
/// emitted map literal is deterministic.
pub(super) fn emit_resource_value_sanitize_fields(
    p: &mut GoPrinter,
    sanitized: &[SanitizedFieldRef<'_>],
) {
    p.line("SanitizeColumns: map[string]string{");
    p.indent();
    for entry in sanitized {
        let col = &entry.field.name;
        let profile = profile_str(entry.profile);
        p.line(&format!("\"{col}\": \"{profile}\","));
    }
    p.dedent();
    p.line("},");
}
