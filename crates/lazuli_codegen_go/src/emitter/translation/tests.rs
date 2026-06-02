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
        knowledge: None,
        defaults: Defaults {
            tenancy: None,
            timestamps: false,
            policy: None,
            rate_limit: None,
            audit: None,
        },
        uses: Vec::new(),
        uses_spans: Vec::new(),
        uses_versions: Vec::new(),
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
        errors: None,
        commands: Vec::new(),
        apis: Vec::new(),
        records: Vec::new(),
        queries: Vec::new(),
        resume_routers: Vec::new(),
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
        caches: Vec::new(),
        aggregates: vec![],
        mcp_servers: vec![],
        previous_names: Vec::new(),
        span_ref: None,
        synth_origins: std::collections::BTreeMap::new(),
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
        doctor_allows: Vec::new(),
        features,
    }
}

fn sample_translation() -> Translation {
    Translation {
        catalog: "./i18n/customer.<locale>.json".to_owned(),
        keys: vec![
            TranslationKey {
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
            },
            TranslationKey {
                name: "policy_denied".to_owned(),
                variants: vec![
                    TranslationVariant {
                        locale: "pt-BR".to_owned(),
                        text: "Acesso negado.".to_owned(),
                    },
                    TranslationVariant {
                        locale: "en-US".to_owned(),
                        text: "Access denied.".to_owned(),
                    },
                ],
                plurals: Vec::new(),
            },
        ],
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
fn translation_files_emit_init_plus_per_locale_json() {
    let mut feature = base_feature("customer");
    feature.translation = Some(sample_translation());
    let files = emit_files(&feature);

    let paths: Vec<&str> = files.iter().map(|file| file.path.as_str()).collect();
    // translation.gen.go first, then one JSON per locale in
    // BTreeMap (alphabetical) order: en-US before pt-BR.
    assert_eq!(
        paths,
        vec![
            "customer/translation.gen.go",
            "customer/i18n/customer.en-US.json",
            "customer/i18n/customer.pt-BR.json",
        ]
    );

    // No placeholder JSON anymore — the per-locale files cover the
    // `//go:embed i18n/*.json` directive.
    assert!(
        !paths.iter().any(|p| p.ends_with("_placeholder.json")),
        "Wave 3.5 must not emit _placeholder.json: {paths:?}"
    );
}

#[test]
fn translation_files_skip_emit_when_keys_are_empty() {
    let mut feature = base_feature("customer");
    feature.translation = Some(Translation {
        catalog: "./i18n/customer.<locale>.json".to_owned(),
        keys: Vec::new(),
    });

    assert!(emit_files(&feature).is_empty());
}

#[test]
fn translation_emits_init_register_call_not_catalog_literal() {
    let mut feature = base_feature("customer");
    feature.translation = Some(sample_translation());
    let out = emit(&feature).expect("must emit");

    assert!(out.starts_with("// Code generated by lazuli; DO NOT EDIT.\n"));
    assert!(out.contains("package customergen"));
    assert!(out.contains("//go:embed i18n/*.json"));
    assert!(out.contains("var translationFS embed.FS"));
    assert!(out.contains("//lazuli:pattern translation_catalog v1"));
    assert!(out.contains("func init() {"));
    assert!(out.contains(
        "lazuli.RegisterFeatureTranslationCatalog(\"customer\", translationFS, \"i18n\")"
    ));
    // No more arbitrary `Name` literal pointing at the first key.
    assert!(!out.contains("i18n.Catalog{"));
    assert!(!out.contains("var customerTranslations"));
}

#[test]
fn translation_imports_runtime_package_not_i18n_subpkg() {
    // The init() calls `lazuli.RegisterFeatureTranslationCatalog`,
    // so the import set drops the i18n subpackage in favor of the
    // public re-export.
    let mut feature = base_feature("customer");
    feature.translation = Some(sample_translation());
    let out = emit(&feature).expect("must emit");

    assert!(out.contains("\"lazuli.dev/runtime/lazuli\""));
    assert!(!out.contains("\"lazuli.dev/runtime/lazuli/i18n\""));
}

#[test]
fn per_locale_json_carries_bare_keys_not_qualified() {
    let mut feature = base_feature("customer");
    feature.translation = Some(sample_translation());
    let files = emit_files(&feature);

    let pt_br = files
        .iter()
        .find(|f| f.path == "customer/i18n/customer.pt-BR.json")
        .expect("pt-BR file present");
    // Bare key, not qualified — the runtime loader prepends the
    // feature name at insert time.
    assert!(pt_br.contains_key("welcome_title"));
    assert!(pt_br.contains_text("Boas vindas"));
    assert!(pt_br.contains_key("policy_denied"));
    assert!(pt_br.contains_text("Acesso negado."));
    assert!(
        !pt_br.contents.contains("customer.welcome_title"),
        "JSON must carry bare keys, not qualified ones:\n{}",
        pt_br.contents
    );

    let en_us = files
        .iter()
        .find(|f| f.path == "customer/i18n/customer.en-US.json")
        .expect("en-US file present");
    assert!(en_us.contains_text("Welcome"));
    assert!(en_us.contains_text("Access denied."));
}

#[test]
fn per_locale_json_is_sorted_by_bare_key() {
    let mut feature = base_feature("customer");
    feature.translation = Some(sample_translation());
    let files = emit_files(&feature);
    let pt_br = files
        .iter()
        .find(|f| f.path == "customer/i18n/customer.pt-BR.json")
        .expect("pt-BR file present");
    // BTreeMap walk: "policy_denied" < "welcome_title".
    let p_idx = pt_br.contents.find("\"policy_denied\"").unwrap();
    let w_idx = pt_br.contents.find("\"welcome_title\"").unwrap();
    assert!(
        p_idx < w_idx,
        "JSON keys must be sorted:\n{}",
        pt_br.contents
    );
}

#[test]
fn locales_without_variants_are_not_emitted() {
    // Feature authoring only pt-BR variants must not produce an
    // en-US JSON file — the loader is built to silently ignore
    // missing locales, so emitting empty/partial files would only
    // add noise.
    let mut feature = base_feature("customer");
    feature.translation = Some(Translation {
        catalog: "./i18n/customer.<locale>.json".to_owned(),
        keys: vec![TranslationKey {
            name: "k".to_owned(),
            variants: vec![TranslationVariant {
                locale: "pt-BR".to_owned(),
                text: "olá".to_owned(),
            }],
            plurals: Vec::new(),
        }],
    });
    let files = emit_files(&feature);
    let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(
        paths,
        vec![
            "customer/translation.gen.go",
            "customer/i18n/customer.pt-BR.json",
        ]
    );
}

#[test]
fn plural_other_arm_is_folded_when_no_singular_variant() {
    let mut feature = base_feature("customer");
    feature.translation = Some(Translation {
        catalog: "./i18n/customer.<locale>.json".to_owned(),
        keys: vec![TranslationKey {
            name: "items_count".to_owned(),
            variants: Vec::new(),
            plurals: vec![TranslationPluralArm {
                arm: "other".to_owned(),
                variants: vec![TranslationVariant {
                    locale: "en-US".to_owned(),
                    text: "{count} items".to_owned(),
                }],
            }],
        }],
    });
    let files = emit_files(&feature);
    let en_us = files
        .iter()
        .find(|f| f.path == "customer/i18n/customer.en-US.json")
        .expect("en-US must surface the `other` arm fallback");
    assert!(en_us.contains_key("items_count"));
    assert!(en_us.contents.contains("{count} items"));
}

#[test]
fn json_escaping_handles_quotes_and_backslashes() {
    let mut feature = base_feature("customer");
    feature.translation = Some(Translation {
        catalog: "./i18n/customer.<locale>.json".to_owned(),
        keys: vec![TranslationKey {
            name: "k".to_owned(),
            variants: vec![TranslationVariant {
                locale: "en-US".to_owned(),
                text: "He said \"hi\\there\"\nnewline".to_owned(),
            }],
            plurals: Vec::new(),
        }],
    });
    let files = emit_files(&feature);
    let en_us = files
        .iter()
        .find(|f| f.path == "customer/i18n/customer.en-US.json")
        .expect("en-US present");
    // Quotes, backslashes, and newlines must be escaped.
    assert!(en_us.contents.contains(r#"\"hi\\there\""#));
    assert!(en_us.contents.contains("\\n"));
}

#[test]
fn catalog_uses_feature_name_in_init_call() {
    let mut feature = base_feature("customer_outreach");
    feature.translation = Some(sample_translation());
    let out = emit(&feature).expect("must emit");

    assert!(out.contains("package customer_outreachgen"));
    assert!(out.contains(
        "lazuli.RegisterFeatureTranslationCatalog(\"customer_outreach\", translationFS, \"i18n\")"
    ));
}

#[test]
fn deterministic_across_runs() {
    let mut feature = base_feature("customer");
    feature.translation = Some(sample_translation());

    let a = emit_files(&feature);
    let b = emit_files(&feature);
    assert_eq!(a.len(), b.len());
    for (left, right) in a.iter().zip(b.iter()) {
        assert_eq!(left.path, right.path);
        assert_eq!(left.contents, right.contents);
    }
}

// Tiny helpers used by the JSON-shape assertions above to keep
// test predicates readable.
impl GeneratedFile {
    fn contains_key(&self, key: &str) -> bool {
        self.contents.contains(&format!("\"{key}\""))
    }
    fn contains_text(&self, text: &str) -> bool {
        // The text appears inside a JSON-quoted string, so an
        // unanchored substring match is enough; the surrounding
        // quotes and key prefix are checked separately.
        self.contents.contains(text)
    }
}
