//! Cell G3a (Wave 3.5) — `Translation` kind emission.
//!
//! Walks the optional `Feature.translation` block and emits both:
//!
//!   1. One JSON file per locale at `<feature>/i18n/<feature>.<locale>.json`
//!      carrying the flat `{"<bare_key>": "<localized_text>", ...}` map. The
//!      runtime loader (`lazuli.RegisterFeatureTranslationCatalog`) reads
//!      every locale file at boot and merges the qualified
//!      `<feature>.<bare_key>` entries into the default resolver's
//!      `Catalogs` map.
//!
//!   2. A single `<feature>/translation.gen.go` carrying an `init()` block
//!      that calls `lazuli.RegisterFeatureTranslationCatalog("<feature>",
//!      translationFS, "i18n")`. The Go side does the parse + merge so the
//!      .lzi `translation` block reaches the L1/L2 resolver layers
//!      end-to-end (proposal §2.E, §5.1).
//!
//! Wave 3.5 closes the codegen ↔ runtime sync gap noted in hostpoint
//! `docs/error-vocab-adoption.md` (the "Known follow-up" paragraph): the
//! 124 per-feature translation keys hostpoint authored in c96343d now
//! reach the wire. Before Wave 3.5 codegen emitted a `_placeholder.json`
//! + a `Catalog` literal that was never read by the runtime; after Wave
//! 3.5 the resolver L1/L2 hits authored text.
//!
//! Determinism: keys + locales walked in BTreeMap order so the emitted
//! JSON, the Go source, and the file listing are byte-equivalent across
//! runs.

use std::collections::BTreeMap;

use lazuli_ir::{Feature, Translation};

use super::cross_feature::CrossFeatureIndex;
use super::imports::ImportSet;
use super::patterns::{PATTERN_TRANSLATION_CATALOG, emit_pattern_header};
use super::printer::GoPrinter;
use crate::GeneratedFile;

/// Emit translation generated files for a feature. Returns an empty
/// vector when the feature declares no `translation` block or has no
/// translation keys (no `_placeholder.json` — adapters layer their own
/// files into the directory if they need it).
///
/// When keys exist, this emits:
///   - `<feature>/translation.gen.go` — the `init()` registration call.
///   - One `<feature>/i18n/<feature>.<locale>.json` per locale that any
///     authored key carries a variant for.
pub fn emit_translation_files(
    source_label: &str,
    feature: &Feature,
    module_name: &str,
    cross_index: &CrossFeatureIndex<'_>,
) -> Vec<GeneratedFile> {
    let Some(translation) = feature.translation.as_ref() else {
        return Vec::new();
    };
    if translation.keys.is_empty() {
        return Vec::new();
    }

    let Some(contents) = emit_translation_file(source_label, feature, module_name, cross_index)
    else {
        return Vec::new();
    };

    let mut files = vec![GeneratedFile {
        path: format!("{name}/translation.gen.go", name = feature.name),
        contents,
    }];

    // One JSON catalog per locale. Locales are discovered from the
    // authored variants — codegen does not invent locales the author did
    // not author. The doctor's translation-key-coverage diagnostics catch
    // missing locales separately; emission here is just a faithful
    // lowering of what's in the IR.
    for (locale, catalog) in build_locale_catalogs(translation) {
        files.push(GeneratedFile {
            path: format!(
                "{name}/i18n/{name}.{locale}.json",
                name = feature.name,
                locale = locale,
            ),
            contents: render_locale_json(&catalog),
        });
    }

    files
}

/// Emit `<feature>/translation.gen.go` contents. Returns `None` when
/// the feature declares no `translation` block or has no keys — the
/// orchestrator skips the file entirely in that case so packages
/// without a real catalog never carry an empty `//go:embed` directive.
pub fn emit_translation_file(
    source_label: &str,
    feature: &Feature,
    module_name: &str,
    cross_index: &CrossFeatureIndex<'_>,
) -> Option<String> {
    let translation = feature.translation.as_ref()?;
    if translation.keys.is_empty() {
        return None;
    }

    // The translation emitter does not need cross-feature type
    // resolution, but the signature matches the orchestrator surface
    // used by the other per-feature emitters.
    let _ = (module_name, cross_index);

    let mut p = GoPrinter::new();
    let mut imports = ImportSet::new();
    imports.add("embed");
    imports.add("lazuli.dev/runtime/lazuli");

    p.banner(
        source_label,
        &super::casing::gen_package_name(&feature.name),
    );
    imports.emit(&mut p);
    p.blank();

    emit_translation_embed(&mut p);
    p.blank();
    emit_catalog_register(&mut p, feature);

    Some(p.finish())
}

fn emit_translation_embed(p: &mut GoPrinter) {
    p.line("//go:embed i18n/*.json");
    p.line("var translationFS embed.FS");
}

fn emit_catalog_register(p: &mut GoPrinter, feature: &Feature) {
    p.line(&format!(
        "// Register the authored `translation` block for `{}` with the",
        feature.name
    ));
    p.line("// process-global resolver. The Go-side loader walks");
    p.line("// `i18n/<feature>.<locale>.json` files in translationFS, qualifies");
    p.line("// each bare key as `<feature>.<key>`, and merges the entries into");
    p.line("// `i18n.DefaultResolver.Catalogs[<locale>]`. The resolver's L1");
    p.line("// (per-command MessageKey) and L2 (FeatureErrors.Messages) layers");
    p.line("// then hit authored text instead of falling through to the L3");
    p.line("// built-in floor (proposal §2.E, §5.1).");
    emit_pattern_header(p, PATTERN_TRANSLATION_CATALOG);
    p.line("func init() {");
    p.indent();
    p.line(&format!(
        "if err := lazuli.RegisterFeatureTranslationCatalog({:?}, translationFS, \"i18n\"); err != nil {{",
        feature.name
    ));
    p.indent();
    p.line(&format!(
        "panic(\"lazuli: register translation catalog for feature {}: \" + err.Error())",
        feature.name
    ));
    p.dedent();
    p.line("}");
    p.dedent();
    p.line("}");
}

/// Build the per-locale `bare_key -> text` maps from the IR translation
/// block. Both the outer map (locale → catalog) and each inner catalog
/// are `BTreeMap`s so JSON emission is deterministic.
///
/// Per-locale variants on `TranslationKey.variants` win over plural
/// arms; v1 codegen emits only the singular text per locale (plural
/// rendering stays on the i18n bucket cycle's render-time path, not
/// here). When a locale appears only in `plurals[].variants[]`, the
/// `other` arm's text is folded in as the singular fallback so the
/// resolver has something to render before the plural-aware adapter
/// catches up.
fn build_locale_catalogs(translation: &Translation) -> BTreeMap<String, BTreeMap<String, String>> {
    let mut by_locale: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    for key in &translation.keys {
        for variant in &key.variants {
            by_locale
                .entry(variant.locale.clone())
                .or_default()
                .insert(key.name.clone(), variant.text.clone());
        }
        // Plural-only keys: fold the `other` arm into the flat catalog
        // so a missing-singular case still renders. Adapters will swap
        // this for CLDR-aware rendering once the i18n plural cycle
        // closes.
        if key.variants.is_empty() {
            for arm in &key.plurals {
                if arm.arm != "other" {
                    continue;
                }
                for variant in &arm.variants {
                    by_locale
                        .entry(variant.locale.clone())
                        .or_default()
                        .insert(key.name.clone(), variant.text.clone());
                }
            }
        }
    }
    by_locale
}

/// Render a single locale catalog as a flat JSON object. Wire-thin:
/// `serde_json` is already a workspace dep but we hand-roll the small
/// shape so the file contents stay byte-equivalent across runs and
/// match the human-readable layout the runtime loader expects
/// (one key per line, two-space indent, no trailing comma).
fn render_locale_json(catalog: &BTreeMap<String, String>) -> String {
    if catalog.is_empty() {
        return "{}\n".to_owned();
    }
    let mut out = String::from("{\n");
    let last = catalog.len() - 1;
    for (idx, (key, text)) in catalog.iter().enumerate() {
        out.push_str("  ");
        out.push_str(&escape_json_string(key));
        out.push_str(": ");
        out.push_str(&escape_json_string(text));
        if idx != last {
            out.push(',');
        }
        out.push('\n');
    }
    out.push_str("}\n");
    out
}

/// Escape a Rust string as a double-quoted JSON string literal.
/// Handles the closed set of JSON escapes (`"`, `\`, `\b`, `\f`, `\n`,
/// `\r`, `\t`, control chars via `\u00XX`).
fn escape_json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{0008}' => out.push_str("\\b"),
            '\u{000c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}


#[cfg(test)]
mod tests;
