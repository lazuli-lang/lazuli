//! Regression test for API registry keys: API `Name` must be
//! feature-qualified so two features can both declare `api list`
//! without colliding in the Go runtime registry. The URL `Path` stays
//! author-declared and unqualified.

use lazuli_codegen_go::{GeneratedFile, GoEmitOptions, generate_v1};
use lazuli_ir::Module;

const DUPLICATE_LIST_APIS: &str = r#"feature feat_a
  policies
    public: @scope.public

  api list
    method GET
    path "/a/list"
    output Text
    policy @policy.public
    handler @fn.list

feature feat_b
  policies
    public: @scope.public

  api list
    method GET
    path "/b/list"
    output Text
    policy @policy.public
    handler @fn.list
"#;

fn parsed_module(source: &str) -> Module {
    let features = lazuli_syntax::parse_feature_skeletons(source)
        .expect("feature source should parse")
        .into_iter()
        .map(|feature| {
            lazuli_analyzer::lower_feature_skeleton(&feature).expect("feature source should lower")
        })
        .collect();
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

fn file<'a>(files: &'a [GeneratedFile], path: &str) -> &'a str {
    files
        .iter()
        .find(|file| file.path == path)
        .unwrap_or_else(|| panic!("expected generated file {path}"))
        .contents
        .as_str()
}

#[test]
fn duplicate_api_short_names_emit_distinct_feature_qualified_names() {
    let module = parsed_module(DUPLICATE_LIST_APIS);
    assert_eq!(module.features.len(), 2);
    assert!(
        module
            .features
            .iter()
            .all(|feature| { feature.apis.len() == 1 && feature.apis[0].name == "list" })
    );

    let files = generate_v1(&module, &GoEmitOptions::default());
    let feat_a = file(&files, "feat_a/api.gen.go");
    let feat_b = file(&files, "feat_b/api.gen.go");

    assert!(
        feat_a.contains("Name:    \"feat_a.list\","),
        "feat_a API Name must be feature-qualified:\n{feat_a}"
    );
    assert!(
        feat_b.contains("Name:    \"feat_b.list\","),
        "feat_b API Name must be feature-qualified:\n{feat_b}"
    );
    assert!(
        !feat_a.contains("Name:    \"list\",") && !feat_b.contains("Name:    \"list\","),
        "bare API registry keys must not be emitted"
    );
    assert!(
        feat_a.contains("Path:    \"/a/list\",") && feat_b.contains("Path:    \"/b/list\","),
        "API Path must remain the authored URL path"
    );
}
