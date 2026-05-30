//! Design-token emission for `lazuli generate ts`.
//!
//! Lazuli's design layer (`design.lzi` → `module.design`) projects to
//! five sibling artifacts under `dist/ts-web/design/`:
//!
//! - `tokens.ts`            — typed token catalog (consumed by app code).
//! - `tokens.css`           — CSS custom properties (the runtime layer
//!                            apps import once in their root stylesheet).
//! - `tailwind.gen.ts`      — Tailwind v3 preset (legacy callsite).
//! - `tailwind.theme.css`   — Tailwind v4 `@theme` block (current).
//! - `allowlist.json`       — allow-list catalog consumed by
//!                            `lazuli doctor` / `lazuli lint` to keep
//!                            ad-hoc Tailwind classes off the canvas.
//!
//! The five emitters live in `lazuli_codegen_ts::design`. This module
//! only wires them into the per-file `GeneratedFile` shape the writer
//! consumes. Per `docs/proposals/design-tokens-l0.md` Cell B; the CLI
//! deliberately does not depend on a yet-to-exist
//! `lazuli_codegen_ts::generate_design` aggregator.

use crate::lazurite_manifest;

/// Stub design emission walker. Wires the 5 design emitters from L0 #2
/// Cell B in `lazuli_codegen_ts::design`.
pub(super) fn emit_design_files(
    design: &lazuli_ir::Design,
    _manifest: &Option<lazurite_manifest::Manifest>,
) -> Vec<lazuli_codegen_ts::GeneratedFile> {
    // Hook point: Cell B's individual emitters live as `pub fn emit_*` in
    // `lazuli_codegen_ts::design::*`. Wire them inline here so the CLI
    // doesn't depend on a yet-to-exist `lazuli_codegen_ts::generate_design`.
    vec![
        lazuli_codegen_ts::GeneratedFile {
            path: "dist/ts-web/design/tokens.ts".to_owned(),
            contents: lazuli_codegen_ts::design::emit_tokens_ts(design),
        },
        lazuli_codegen_ts::GeneratedFile {
            path: "dist/ts-web/design/tokens.css".to_owned(),
            contents: lazuli_codegen_ts::design::emit_tokens_css(design),
        },
        lazuli_codegen_ts::GeneratedFile {
            path: "dist/ts-web/design/tailwind.gen.ts".to_owned(),
            contents: lazuli_codegen_ts::design::emit_tailwind_v3_preset(design),
        },
        lazuli_codegen_ts::GeneratedFile {
            path: "dist/ts-web/design/tailwind.theme.css".to_owned(),
            contents: lazuli_codegen_ts::design::emit_tailwind_v4_theme(design),
        },
        lazuli_codegen_ts::GeneratedFile {
            path: "dist/ts-web/design/allowlist.json".to_owned(),
            contents: lazuli_codegen_ts::design::emit_allowlist_json(design),
        },
    ]
}
