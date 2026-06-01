//! End-to-end golden — `unique <fields> error <CODE>` registration glue.
//!
//! Proves the full parser→IR→ddl→handler-codegen path: a coded unique emits
//! BOTH (a) a deterministically-NAMED `CONSTRAINT <name> UNIQUE (...)` in the
//! migration and (b) a `<feature>/unique_codes.gen.go` registering that EXACT
//! name → the domain code. The two names are produced by one shared helper, so
//! the runtime 23505 remap (`pgErr.ConstraintName` lookup) cannot silently
//! miss. This is the unique-constraint twin of the spec-0014
//! `restrict on_delete ... error <CODE>` seam.

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
        features,
    }
}

fn emit(source: &str) -> Vec<GeneratedFile> {
    generate_v1(&parsed_module(source), &GoEmitOptions::default())
}

fn file<'a>(files: &'a [GeneratedFile], path: &str) -> &'a str {
    files
        .iter()
        .find(|f| f.path == path)
        .unwrap_or_else(|| panic!("missing generated file {path}; got: {:?}",
            files.iter().map(|f| &f.path).collect::<Vec<_>>()))
        .contents
        .as_str()
}

const JOB_MEMBER: &str = r#"feature job
  domain
    resource JobMember
      job_id: ID required
      user_id: ID required
      unique (job_id, user_id) error MEMBER_ALREADY_IN_JOB
"#;

#[test]
fn coded_unique_emits_named_constraint_and_matching_registration() {
    let files = emit(JOB_MEMBER);

    // (a) The migration carries the DETERMINISTICALLY-named UNIQUE constraint.
    let migration = file(&files, "migrations/001_job_job_member.sql");
    assert!(
        migration.contains("CONSTRAINT job_member_job_id_user_id_key UNIQUE (job_id, user_id)"),
        "named unique constraint missing from DDL:\n{migration}"
    );

    // (b) The registration glue binds that EXACT name → the domain code.
    let glue = file(&files, "job/unique_codes.gen.go");
    assert!(
        glue.contains(
            "lazuli.RegisterUniqueViolationCode(\"job_member_job_id_user_id_key\", \"MEMBER_ALREADY_IN_JOB\")"
        ),
        "registration glue missing/misnamed:\n{glue}"
    );
    assert!(glue.contains("func init()"), "registration must run in init():\n{glue}");

    // (c) The two names are byte-identical — the make-or-break correctness
    // property. Extract both and assert equality so a future rename of either
    // side fails loudly here.
    let ddl_name = "job_member_job_id_user_id_key";
    assert!(
        migration.contains(&format!("CONSTRAINT {ddl_name} UNIQUE")),
        "DDL constraint name drifted:\n{migration}"
    );
    assert!(
        glue.contains(&format!("RegisterUniqueViolationCode(\"{ddl_name}\"")),
        "registration constraint name drifted from DDL name {ddl_name}:\n{glue}"
    );
}

#[test]
fn uncoded_unique_emits_no_registration_file() {
    // A plain `unique (...)` (no `error <CODE>`) stays anonymous and registers
    // nothing — back-compat for every constraint that did not opt in.
    let files = emit(
        r#"feature job
  domain
    resource JobTag
      job_id: ID required
      label: Text required
      unique (job_id, label)
"#,
    );
    assert!(
        !files.iter().any(|f| f.path == "job/unique_codes.gen.go"),
        "uncoded unique must not emit a registration file"
    );
}
