//! Cell G3a - `Translation` kind emission. Walks the optional
//! `Feature.translation` block and emits the Go-side embedded
//! `i18n.Catalog` handle into `<feature>/translation.gen.go`, plus
//! `<feature>/i18n/_placeholder.json` so `//go:embed i18n/*.json`
//! always matches at least one generated file.
//!
//! Proposal references:
//! - §3.10 - `//go:embed i18n/*.json`, `embed.FS`, and the
//!   `i18n.Catalog` value shape.
//! - §4.5 - Lazuli Go lib backfill: `runtime/go/lazuli/i18n` now has
//!   a `Catalog` type.
//! - §11 - catalog JSON files remain user territory; codegen owns only
//!   the embed directive and generated Go wrapper.
//!
//! Determinism: there is at most one translation block per feature.
//! The generated source does not duplicate translation variants,
//! because the strings ship in sibling JSON catalogs and must not be
//! copied into Go literals.

use lazuli_ir::{Feature, Translation};

use super::casing::lower_camel;
use super::cross_feature::CrossFeatureIndex;
use super::imports::ImportSet;
use super::printer::GoPrinter;
use crate::GeneratedFile;

const PLACEHOLDER_JSON_PATH: &str = "i18n/_placeholder.json";
const PLACEHOLDER_JSON_CONTENTS: &str = "{}";

/// Emit translation generated files for a feature. Returns an empty
/// vector when the feature declares no `translation` block or has no
/// translation keys.
///
/// When keys exist, this emits both `<feature>/translation.gen.go` and
/// `<feature>/i18n/_placeholder.json`. The placeholder file keeps
/// `//go:embed i18n/*.json` valid even before users add real catalogs.
pub fn emit_translation_files(
    source_label: &str,
    feature: &Feature,
    module_name: &str,
    cross_index: &CrossFeatureIndex<'_>,
) -> Vec<GeneratedFile> {
    let Some(contents) = emit_translation_file(source_label, feature, module_name, cross_index)
    else {
        return Vec::new();
    };

    vec![
        GeneratedFile {
            path: format!("{name}/translation.gen.go", name = feature.name),
            contents,
        },
        GeneratedFile {
            path: format!("{name}/{PLACEHOLDER_JSON_PATH}", name = feature.name),
            contents: PLACEHOLDER_JSON_CONTENTS.to_owned(),
        },
    ]
}

/// Emit `<feature>/translation.gen.go` contents for a feature, or
/// `None` when the feature declares no `translation` block or has no
/// translation keys. The latter keeps `//go:embed i18n/*.json` out of
/// generated packages that do not actually ship catalog files.
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
    imports.add("lazuli.dev/runtime/lazuli/i18n");

    p.banner(source_label, &feature.name);
    imports.emit(&mut p);
    p.blank();

    emit_translation_embed(&mut p);
    p.blank();
    emit_catalog_value(&mut p, feature, translation);

    Some(p.finish())
}

fn emit_translation_embed(p: &mut GoPrinter) {
    p.line("//go:embed i18n/*.json");
    p.line("var translationFS embed.FS");
}

fn emit_catalog_value(p: &mut GoPrinter, feature: &Feature, translation: &Translation) {
    let var_name = translation_var_name(&feature.name);
    let catalog_name = catalog_name(feature, translation);
    p.line(&format!("var {var_name} = i18n.Catalog{{"));
    p.indent();
    p.line(&format!("Name:     {:?},", catalog_name));
    p.line("FS:       translationFS,");
    p.line("BasePath: \"i18n\",");
    p.dedent();
    p.line("}");
}

fn translation_var_name(feature_name: &str) -> String {
    format!("{}Translations", lower_camel(feature_name))
}

fn catalog_name(feature: &Feature, translation: &Translation) -> String {
    let key_name = translation
        .keys
        .first()
        .map(|key| key.name.as_str())
        .unwrap_or("messages");
    format!("{}.{}", feature.name, key_name)
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
            pollers: vec![],
            auth: None,
            surfaces: Vec::new(),
            extensions: Vec::new(),
            escape_routes: Vec::new(),
            agents: Vec::new(),
            reports: Vec::new(),
            channels: Vec::new(),
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
            design: None,
            rbac: None,
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

    fn emit_files(feature: &Feature) -> Vec<GeneratedFile> {
        let module = module_with_features(vec![feature.clone()]);
        let index = CrossFeatureIndex::build(&module);
        emit_translation_files("examples/x.lzi", feature, "lazuli/test", &index)
    }

    #[test]
    fn empty_feature_returns_none() {
        let feature = base_feature("customer");
        assert!(emit(&feature).is_none());
    }

    #[test]
    fn translation_with_empty_keys_returns_none() {
        let mut feature = base_feature("customer");
        feature.translation = Some(Translation {
            catalog: "./i18n/customer.<locale>.json".to_owned(),
            keys: Vec::new(),
        });

        assert!(emit(&feature).is_none());
    }

    #[test]
    fn translation_files_include_placeholder_json() {
        let mut feature = base_feature("customer");
        feature.translation = Some(sample_translation());
        let files = emit_files(&feature);

        assert_eq!(
            files
                .iter()
                .map(|file| file.path.as_str())
                .collect::<Vec<_>>(),
            vec![
                "customer/translation.gen.go",
                "customer/i18n/_placeholder.json"
            ]
        );

        let placeholder = files
            .iter()
            .find(|file| file.path == "customer/i18n/_placeholder.json")
            .expect("must emit placeholder JSON");
        assert_eq!(placeholder.contents, "{}");
    }

    #[test]
    fn translation_files_skip_placeholder_when_keys_are_empty() {
        let mut feature = base_feature("customer");
        feature.translation = Some(Translation {
            catalog: "./i18n/customer.<locale>.json".to_owned(),
            keys: Vec::new(),
        });

        assert!(emit_files(&feature).is_empty());
    }

    #[test]
    fn translation_emits_embed_fs_and_catalog_value() {
        let mut feature = base_feature("customer");
        feature.translation = Some(sample_translation());
        let out = emit(&feature).expect("must emit");

        assert!(out.starts_with("// Code generated by lazuli; DO NOT EDIT.\n"));
        assert!(!out.contains("//go:build lazuli_i18n"));
        assert!(out.contains("package customer"));
        assert!(out.contains("import (\n\t\"embed\"\n\n\t\"lazuli.dev/runtime/lazuli/i18n\"\n)"));
        assert!(out.contains("//go:embed i18n/*.json"));
        assert!(out.contains("var translationFS embed.FS"));
        assert!(out.contains("var customerTranslations = i18n.Catalog{"));
        assert!(out.contains("Name:     \"customer.welcome_title\","));
        assert!(out.contains("FS:       translationFS,"));
        assert!(out.contains("BasePath: \"i18n\","));
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
    fn catalog_uses_lower_camel_feature_var_name() {
        let mut feature = base_feature("customer_outreach");
        feature.translation = Some(sample_translation());
        let out = emit(&feature).expect("must emit");

        assert!(out.contains("package customer_outreach"));
        assert!(out.contains("var customerOutreachTranslations = i18n.Catalog"));
        assert!(out.contains("Name:     \"customer_outreach.welcome_title\","));
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
