//! Row 30 — Storage bucket cycle. Integration coverage for the
//! file-local LSP hooks that drive `@cap.File(...)` ergonomics:
//!   - `file_capability_contract_diagnostics` accepts the four
//!     canonical arguments (`max_size`, `accept`, `visibility`,
//!     `signed_ttl`) without emitting the unknown-argument warning.
//!   - Authoring an empty `@cap.File()` still warns (shape rule).
//!
//! Hover / completion behaviour lives in the inline test module
//! (`crates/lazuli_lsp/src/lib.rs` `mod tests`) because the helpers
//! it exercises are not part of the public crate surface.

use lazuli_lsp::diagnostics_for_source;

#[test]
fn cap_file_canonical_arguments_do_not_warn_unknown() {
    let source = "feature x\n  domain\n    resource Export\n      file: @cap.File(max_size:25mb,accept:text/csv,visibility:private,signed_ttl:1h) required\n";

    let diagnostics = diagnostics_for_source(source);
    // None of the `capability-arguments` warnings should fire — every
    // argument is in the canonical closed set after Row 30.
    for diagnostic in &diagnostics {
        let msg = diagnostic.message.as_str();
        assert!(
            !msg.starts_with("@cap.File only accepts canonical arguments"),
            "canonical args should not trigger unknown-argument warning; got `{}`",
            msg
        );
    }
}

#[test]
fn cap_file_missing_max_size_still_warns() {
    let source = "feature x\n  domain\n    resource Export\n      file: @cap.File(accept:text/csv,visibility:private) required\n";

    let diagnostics = diagnostics_for_source(source);
    assert!(
        diagnostics
            .iter()
            .any(|d| d.message.contains("max_size")),
        "missing `max_size` should still warn; got {:?}",
        diagnostics
            .iter()
            .map(|d| d.message.clone())
            .collect::<Vec<_>>()
    );
}
