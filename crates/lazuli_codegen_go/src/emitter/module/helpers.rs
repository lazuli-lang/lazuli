//! Small helpers used by the top-level emitter walker — the empty
//! per-feature stub written before any kind-emitter has run (cell E1
//! contract), and the lower-snake / lower-kebab caser shared with the
//! CLI's module-name derivation.
//!
//! Wave R7-3 extract: lifted out of `module/mod.rs`.

use super::super::imports::ImportSet;
use super::super::printer::GoPrinter;

pub(super) fn emit_feature_stub(source: &str, feature_name: &str) -> String {
    let mut p = GoPrinter::new();
    p.banner(source, &super::super::casing::gen_package_name(feature_name));
    // E1 stub: imports are recorded but unused because no kinds emit
    // yet. We deliberately do not produce an `import (...)` block
    // until the first kind walker (cell E2) introduces a real use —
    // a leading empty block would fail `gofmt`.
    let _placeholder = ImportSet::new();
    p.finish()
}

/// Lower-snake / lower-kebab caser shared with the CLI helper. Mirrors
/// `lazuli_codegen_go::to_kebab_case` (legacy demo) and the CLI's
/// `to_kebab_case` so the derived module name matches across surfaces.
pub(super) fn to_kebab(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut prev_lower = false;
    for ch in value.chars() {
        if ch == '_' || ch == ' ' {
            out.push('-');
            prev_lower = false;
            continue;
        }
        if ch.is_ascii_uppercase() {
            if prev_lower && !out.is_empty() {
                out.push('-');
            }
            for low in ch.to_lowercase() {
                out.push(low);
            }
            prev_lower = false;
            continue;
        }
        out.push(ch);
        prev_lower = ch.is_ascii_lowercase() || ch.is_ascii_digit();
    }
    out
}
