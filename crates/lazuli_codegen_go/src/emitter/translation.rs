//! Cell G3a - `Translation` kind emission. Walks the optional
//! `Feature.translation` block and emits the Go-side embedded catalog
//! handle into `<feature>/translation.gen.go`.
//!
//! Proposal references:
//! - §3.10 - `//go:embed i18n/*.json`, `embed.FS`, and the future
//!   `i18n.Catalog` value shape.
//! - §4.5 - Lazuli Go lib gap: `runtime/go/lazuli/i18n` currently has
//!   `LocaleContract` and `Fallback`, but no `Catalog` type.
//! - §11 - catalog JSON files remain user territory; codegen owns only
//!   the embed directive and generated Go wrapper.
//!
//! Determinism: there is at most one translation block per feature.
//! The generated source does not walk `Translation.keys`, because the
//! strings ship in sibling JSON catalogs and must not be duplicated
//! into Go literals.

use lazuli_ir::{Feature, Translation};

use super::casing::lower_camel;
use super::cross_feature::CrossFeatureIndex;
use super::imports::ImportSet;
use super::printer::GoPrinter;

/// Emit `<feature>/translation.gen.go` for a feature, or `None` when
/// the feature declares no `translation` block.
pub fn emit_translation_file(
    source_label: &str,
    feature: &Feature,
    module_name: &str,
    cross_index: &CrossFeatureIndex<'_>,
) -> Option<String> {
    let translation = feature.translation.as_ref()?;

    // The translation emitter does not need cross-feature type
    // resolution, but the signature matches the orchestrator surface
    // used by the other per-feature emitters.
    let _ = (module_name, cross_index);

    let mut p = GoPrinter::new();
    let mut imports = ImportSet::new();
    imports.add("embed");
    if i18n_catalog_available() {
        imports.add("lazuli.dev/runtime/lazuli/i18n");
    }

    p.banner(source_label, &feature.name);
    imports.emit(&mut p);
    p.blank();

    emit_translation_embed(&mut p);
    p.blank();

    if i18n_catalog_available() {
        emit_catalog_value(&mut p, feature, translation);
    } else {
        p.line("// TODO(runtime): i18n.Catalog not yet in lib (§4.5)");
    }

    Some(p.finish())
}

fn emit_translation_embed(p: &mut GoPrinter) {
    p.line("//go:embed i18n/*.json");
    p.line("var translationFS embed.FS");
}

fn emit_catalog_value(p: &mut GoPrinter, feature: &Feature, _translation: &Translation) {
    let var_name = translation_var_name(&feature.name);
    let catalog_name = catalog_name(feature);
    p.line(&format!(
        "var {var_name} = i18n.Catalog{{Name: {:?}, FS: translationFS, BasePath: \"i18n\"}}",
        catalog_name
    ));
}

fn translation_var_name(feature_name: &str) -> String {
    format!("{}Translations", lower_camel(feature_name))
}

fn catalog_name(feature: &Feature) -> String {
    format!("{}.messages", feature.name)
}

fn i18n_catalog_available() -> bool {
    // Verified against `runtime/go/lazuli/i18n` in this worktree:
    // `Catalog` is still the §4.5 runtime gap. Keeping this as a
    // helper makes the eventual flip a one-line change without
    // reshaping emission.
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        Defaults, Feature, Module, Policies, Translation, TranslationKey, TranslationPluralArm,
        TranslationVariant,
    };

    fn base_feature(name: &str) -> Feature {
        Feature {
            name: name.to_owned(),
            purpose: None,
            non_goals: Vec::new(),
            context_path: None,
            defaults: Defaults {
                tenancy: None,
                timestamps: false,
                policy: None,
            },
            uses: Vec::new(),
            requirements: Vec::new(),
            enums: Vec::new(),
            resources: Vec::new(),
            events: Vec::new(),
            rules: Vec::new(),
            policies: Policies {
                categories: Vec::new(),
                fields: Vec::new(),
                span_ref: None,
            },
            commands: Vec::new(),
            apis: Vec::new(),
            records: Vec::new(),
            queries: Vec::new(),
            workflows: Vec::new(),
            jobs: Vec::new(),
            webhooks: Vec::new(),
            notifications: Vec::new(),
            event_groups: Vec::new(),
            tenant_migrations: Vec::new(),
            translation: None,
            auth: None,
            surfaces: Vec::new(),
            extensions: Vec::new(),
            escape_routes: Vec::new(),
            agents: Vec::new(),
            previous_names: Vec::new(),
            span_ref: None,
        }
    }

    fn module_with_features(features: Vec<Feature>) -> Module {
        Module {
            workspace: None,
            contracts: Vec::new(),
            app: None,
            registry: None,
            profiles: Vec::new(),
            features,
        }
    }

    fn sample_translation() -> Translation {
        Translation {
            catalog: "./i18n/customer.<locale>.json".to_owned(),
            keys: vec![TranslationKey {
                name: "welcome_title".to_owned(),
                variants: vec![
                    TranslationVariant {
                        locale: "en-US".to_owned(),
                        text: "Welcome".to_owned(),
                    },
                    TranslationVariant {
                        locale: "pt-BR".to_owned(),
                        text: "Boas vindas".to_owned(),
                    },
                ],
                plurals: Vec::new(),
            }],
        }
    }

    fn emit(feature: &Feature) -> Option<String> {
        let module = module_with_features(vec![feature.clone()]);
        let index = CrossFeatureIndex::build(&module);
        emit_translation_file("examples/x.lzi", feature, "lazuli/test", &index)
    }

    #[test]
    fn empty_feature_returns_none() {
        let feature = base_feature("customer");
        assert!(emit(&feature).is_none());
    }

    #[test]
    fn translation_emits_embed_fs_and_catalog_gap_todo() {
        let mut feature = base_feature("customer");
        feature.translation = Some(sample_translation());
        let out = emit(&feature).expect("must emit");

        assert!(out.contains("// Code generated by lazuli; DO NOT EDIT."));
        assert!(out.contains("package customer"));
        assert!(out.contains("import (\n\t\"embed\"\n)"));
        assert!(out.contains("//go:embed i18n/*.json"));
        assert!(out.contains("var translationFS embed.FS"));
        assert!(out.contains("// TODO(runtime): i18n.Catalog not yet in lib (§4.5)"));
    }

    #[test]
    fn translation_does_not_duplicate_catalog_strings_into_go() {
        let mut feature = base_feature("customer");
        feature.translation = Some(sample_translation());
        let out = emit(&feature).expect("must emit");

        assert!(!out.contains("Welcome"));
        assert!(!out.contains("Boas vindas"));
        assert!(!out.contains("./i18n/customer.<locale>.json"));
    }

    #[test]
    fn catalog_gap_skips_active_i18n_import_and_value() {
        let mut feature = base_feature("customer_outreach");
        feature.translation = Some(sample_translation());
        let out = emit(&feature).expect("must emit");

        assert!(out.contains("package customer_outreach"));
        assert!(!out.contains("\"lazuli.dev/runtime/lazuli/i18n\""));
        assert!(!out.contains("var customerOutreachTranslations = i18n.Catalog"));
    }

    #[test]
    fn deterministic_across_runs() {
        let mut feature = base_feature("customer");
        let mut translation = sample_translation();
        translation.keys.push(TranslationKey {
            name: "items_count".to_owned(),
            variants: Vec::new(),
            plurals: vec![TranslationPluralArm {
                arm: "other".to_owned(),
                variants: vec![TranslationVariant {
                    locale: "en-US".to_owned(),
                    text: "{count} items".to_owned(),
                }],
            }],
        });
        feature.translation = Some(translation);

        let a = emit(&feature).expect("must emit");
        let b = emit(&feature).expect("must emit");
        assert_eq!(a, b);
    }
}
