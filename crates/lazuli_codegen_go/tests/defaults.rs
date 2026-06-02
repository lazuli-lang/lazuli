//! 0004 defaults-hoist — codegen byte-identity golden.
//!
//! The migration's safety anchor: for any feature, emitting
//! `{defaults rate_limit X; defaults audit default}` + N commands must
//! produce byte-identical Go to the fully-explicit form where every
//! command spells `rate_limit X` + `audit default`. This proves the
//! hoist is a pure refactor — pilots can delete ~445 duplicated lines
//! with zero behavioural drift.
//!
//! Also pins: a per-command `rate_limit` / `audit` override WINS over the
//! feature default, and `audit none` on a command opts that command out
//! of the inherited audit default.

use lazuli_codegen_go::{GeneratedFile, GoEmitOptions, generate_v1};
use lazuli_ir::Module;

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

fn emit(source: &str) -> Vec<GeneratedFile> {
    let module = parsed_module(source);
    generate_v1(&module, &GoEmitOptions::default())
}

fn files_as_map(files: &[GeneratedFile]) -> std::collections::BTreeMap<&str, &str> {
    files
        .iter()
        .map(|f| (f.path.as_str(), f.contents.as_str()))
        .collect()
}

/// The fully-explicit form: every command spells its own `rate_limit` +
/// `audit default`.
const EXPLICIT: &str = r#"feature billing
  domain
    resource Invoice
      id: ID required
      amount: Integer required

  command create_invoice
    rate_limit "60 per minute per actor"
    audit default
    input
      amount: Integer required
    creates Invoice from input

  command void_invoice
    rate_limit "60 per minute per actor"
    audit default
    input
      id: ID required
    updates Invoice from input

  command delete_invoice
    rate_limit "60 per minute per actor"
    audit default
    input
      id: ID required
    deletes Invoice
"#;

/// The hoisted form: declare the shared `rate_limit` + `audit` once in
/// `defaults`; commands inherit.
const HOISTED: &str = r#"feature billing
  defaults
    rate_limit "60 per minute per actor"
    audit default

  domain
    resource Invoice
      id: ID required
      amount: Integer required

  command create_invoice
    input
      amount: Integer required
    creates Invoice from input

  command void_invoice
    input
      id: ID required
    updates Invoice from input

  command delete_invoice
    input
      id: ID required
    deletes Invoice
"#;

#[test]
fn codegen_byte_identical() {
    let explicit = emit(EXPLICIT);
    let hoisted = emit(HOISTED);

    let explicit_map = files_as_map(&explicit);
    let hoisted_map = files_as_map(&hoisted);

    assert_eq!(
        explicit_map.keys().collect::<Vec<_>>(),
        hoisted_map.keys().collect::<Vec<_>>(),
        "hoisted + explicit forms must emit the same file set"
    );
    for (path, explicit_contents) in &explicit_map {
        let hoisted_contents = hoisted_map.get(path).unwrap();
        assert_eq!(
            explicit_contents, hoisted_contents,
            "byte-identity violated for emitted file `{path}`"
        );
    }

    // Sanity — the inherited rate_limit + audit actually reached codegen
    // (otherwise an empty/empty comparison would pass vacuously).
    let create = hoisted_map
        .get("billing/create_invoice.gen.go")
        .or_else(|| {
            hoisted_map
                .iter()
                .find(|(p, _)| p.contains("create_invoice"))
                .map(|(_, c)| c)
        });
    let any = create
        .copied()
        .unwrap_or_else(|| hoisted.iter().map(|f| f.contents.as_str()).next().unwrap());
    let _ = any;
    assert!(
        hoisted
            .iter()
            .any(|f| f.contents.contains("60 per minute per actor")),
        "hoisted emission must carry the inherited rate_limit string"
    );
}

/// A per-command `rate_limit` / `audit` value overrides the feature
/// default. The override-carrying command must emit its OWN value, not
/// the hoisted default.
#[test]
fn per_command_override_wins() {
    const SRC: &str = r#"feature billing
  defaults
    rate_limit "60 per minute per actor"
    audit default

  domain
    resource Invoice
      id: ID required
      amount: Integer required

  command create_invoice
    input
      amount: Integer required
    creates Invoice from input

  command sensitive_write
    rate_limit "5 per hour per actor"
    input
      amount: Integer required
    creates Invoice from input
"#;
    let files = emit(SRC);
    let blob: String = files.iter().map(|f| f.contents.clone()).collect();
    // The override command keeps its own stricter limit.
    assert!(
        blob.contains("5 per hour per actor"),
        "per-command rate_limit override must win:\n{blob}"
    );
    // The non-override command inherited the feature default.
    assert!(
        blob.contains("60 per minute per actor"),
        "non-override command must inherit the default rate_limit:\n{blob}"
    );
}

/// `audit none` on a command opts it out of the inherited `defaults audit
/// default`. The emitted Go for that command must match what it would emit
/// if `defaults audit` were absent and the command authored `audit none`
/// explicitly — i.e. it does NOT pick up the feature default.
#[test]
fn audit_off_opts_out() {
    const HOISTED_WITH_OPTOUT: &str = r#"feature billing
  defaults
    audit default

  domain
    resource Invoice
      id: ID required
      amount: Integer required

  command create_invoice
    input
      amount: Integer required
    creates Invoice from input

  command silent_write
    audit none
    input
      amount: Integer required
    creates Invoice from input
"#;
    const EXPLICIT_WITH_OPTOUT: &str = r#"feature billing
  domain
    resource Invoice
      id: ID required
      amount: Integer required

  command create_invoice
    audit default
    input
      amount: Integer required
    creates Invoice from input

  command silent_write
    audit none
    input
      amount: Integer required
    creates Invoice from input
"#;
    let hoisted = files_as_map(&emit(HOISTED_WITH_OPTOUT))
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect::<std::collections::BTreeMap<_, _>>();
    let explicit = files_as_map(&emit(EXPLICIT_WITH_OPTOUT))
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(
        hoisted, explicit,
        "`audit none` opt-out under a feature default must emit byte-identically \
         to the fully-explicit `audit default` / `audit none` form"
    );
    // The opt-out command must NOT appear in the audit.gen.go metadata
    // (which only lists `audit default` commands).
    if let Some(audit_meta) = hoisted.get("billing/audit.gen.go") {
        assert!(
            !audit_meta.contains("silentWriteAuditEntry"),
            "the `audit none` command must be absent from audit metadata:\n{audit_meta}"
        );
    }
}
