//! Bracketed-annotation formatters for the inspect features summary.
//!
//! Three families share this module:
//!
//! - The resource-header `(conventions: <a>, <b>[, owner-scope])` suffix.
//! - The per-row `[conv:<bundle>[, owner-scope]]` / `[author override;
//!   convention skipped]` bracketed tag.
//! - The width-padded `<indent><name><pad><annotation>` row layout.
//!
//! Each one is independently testable and used by the top-level
//! orchestrator in `mod.rs`. The closed `convention_name` match is the
//! single source of truth for bundle-name spelling — adding a variant
//! to `lazuli_ir::ConventionRef` is a load-bearing compile-time
//! failure here.

use lazuli_ir::{ConventionOrigin, ConventionRef};

/// Render one `<indent><name><pad>[<annotation>]` row, omitting the
/// trailing space + bracket when the annotation is empty (pure
/// author-written entry without a convention origin).
pub(super) fn render_name_row(name: &str, width: usize, annotation: &str) -> String {
    if annotation.is_empty() {
        format!("    {name}\n")
    } else {
        let pad = width.saturating_sub(name.len());
        let spaces = " ".repeat(pad);
        format!("    {name}{spaces}    {annotation}\n")
    }
}

/// `(conventions: <a>, <b>)` annotation for a resource. Appends a
/// trailing `, owner-scope` segment when at least one of the resource's
/// fields carries `@owner_axis(through: ...)` — see owner-scope proposal
/// §11.2. Empty string when the slot is empty (no annotation rendered at
/// all); the owner-scope suffix is suppressed in that case because the
/// inspect view scopes annotations to opted-in resources.
pub(super) fn format_resource_conventions(
    conventions: &[ConventionRef],
    owner_scope: bool,
) -> String {
    if conventions.is_empty() {
        return String::new();
    }
    let mut names: Vec<String> = conventions
        .iter()
        .map(|c| convention_name(c).to_owned())
        .collect();
    if owner_scope {
        names.push("owner-scope".to_owned());
    }
    format!(" (conventions: {})", names.join(", "))
}

/// Bracketed origin annotation for a command/query name. Empty when
/// the entry carries no `synth_origins` record (a pure author-written
/// command with no convention overlap). Synth-origin entries carry the
/// trailing `, owner-scope` segment when the originating resource has
/// any field with `@owner_axis` — surfaces the owner-scope mode at
/// per-command granularity per owner-scope proposal §11.2.
pub(super) fn format_origin_annotation(
    origin: Option<&ConventionOrigin>,
    owner_scope: bool,
) -> String {
    match origin {
        None => String::new(),
        Some(ConventionOrigin::Synthesized(c)) => {
            if owner_scope {
                format!("[conv:{}, owner-scope]", convention_name(c))
            } else {
                format!("[conv:{}]", convention_name(c))
            }
        }
        Some(ConventionOrigin::AuthorOverride(_)) => {
            "[author override; convention skipped]".to_owned()
        }
    }
}

/// `crud`, `me`, etc. — single source of truth so the LSP catalog
/// list, the doctor diagnostic suggestion, and this rendering all
/// stay aligned. Adding a variant in `lazuli_ir::ConventionRef`
/// requires extending this match — the closed `match` makes that
/// failure mode load-bearing at compile time.
pub(super) fn convention_name(c: &ConventionRef) -> &'static str {
    match c {
        ConventionRef::Crud => "crud",
        ConventionRef::Me => "me",
    }
}
