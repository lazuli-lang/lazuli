//! Spec 0018 — `crud` overlay codegen byte-identity golden.
//!
//! The acceptance bar mirrors the analyzer IR-equivalence oracle one
//! layer down: emitting `conventions [crud]` + a `crud` overlay produces
//! byte-identical Go to the equivalent fully hand-rolled commands. This
//! proves the overlay composes onto the EXISTING emitters (RULE-VOCAB-03,
//! zero new lowering) — the synth+overlay path and the hand-rolled path
//! converge on the same handler/SQL output.
//!
//! The hand-rolled side spells out everything the synth attaches by
//! convention (rate_limit / audit / invalidates / `from input`) so the
//! comparison is exact; that is precisely the form spec 0003 migrates
//! FROM and the overlay reproduces.

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
    let module = parsed_module(source);
    generate_v1(&module, &GoEmitOptions::default())
}

fn files_as_map(files: &[GeneratedFile]) -> std::collections::BTreeMap<String, String> {
    files
        .iter()
        .map(|f| (f.path.clone(), f.contents.clone()))
        .collect()
}

/// `conventions [crud]` + a `crud` overlay on the `Note` resource.
const OVERLAY: &str = r#"feature notes
  policies
    author: @role.ADMIN
    authenticated: @scope.authenticated

  domain
    resource Note
      org: Org required
      title: Text required
      body: Text optional
      pinned: Boolean = false
      created_at: DateTime required
      updated_at: DateTime required
      conventions [crud]
      crud
        create
          policy @policy.edit
          input excludes pinned
          assign pinned = false
          emits note_created
        update
          policy @policy.edit
          emits note_updated
        delete
          policy @policy.remove
          emits note_deleted
"#;

/// The hand-rolled equivalent — the resource still opts into
/// `conventions [crud]` (so the lookup/list queries auto-synth identically
/// on both sides), but the three write commands are authored explicitly.
/// The synth's per-name override (§6) skips synthesizing commands the
/// author already wrote, so only the create/update/delete bodies differ —
/// and they must emit byte-identical Go. The hand-rolled bodies spell out
/// everything the synth attaches by convention (rate_limit / audit /
/// invalidates / `from input`).
const HANDROLLED: &str = r#"feature notes
  policies
    author: @role.ADMIN
    authenticated: @scope.authenticated

  domain
    resource Note
      org: Org required
      title: Text required
      body: Text optional
      pinned: Boolean = false
      created_at: DateTime required
      updated_at: DateTime required
      conventions [crud]

  command create_note
    rate_limit "100 per 10 minutes per ip"
    audit default
    input
      title: Text required
      body: Text optional
    policy @policy.edit
    creates Note from input
      title = input.title
      body = input.body
      pinned = false
    emits note_created
    invalidates
      query.lookup_note
      query.list_notes

  command update_note
    rate_limit "100 per 10 minutes per ip"
    audit default
    route id: ID
    input
      title: Text optional
      body: Text optional
      pinned: Boolean optional
    policy @policy.edit
    updates Note
      title = input.title
      body = input.body
      pinned = input.pinned
    emits note_updated
    invalidates
      query.lookup_note
      query.list_notes

  command delete_note
    rate_limit "100 per 10 minutes per ip"
    audit default
    route id: ID
    policy @policy.remove
    deletes Note
    emits note_deleted
    invalidates
      query.lookup_note
      query.list_notes
"#;

/// Filter the emitted file set to the per-command handler/effect files we
/// care about for command equivalence (skip migrations / schema / SDK
/// which carry resource-shape concerns common to both sides anyway).
fn command_files(files: &[GeneratedFile]) -> std::collections::BTreeMap<String, String> {
    files_as_map(files)
        .into_iter()
        .filter(|(path, _)| {
            let p = path.to_lowercase();
            (p.contains("note") || p.contains("command") || p.contains("handler"))
                && p.ends_with(".go")
        })
        .collect()
}

#[test]
fn codegen_overlay_matches_handrolled() {
    let overlay = emit(OVERLAY);
    let hand = emit(HANDROLLED);

    let overlay_cmds = command_files(&overlay);
    let hand_cmds = command_files(&hand);

    // The two feature sources must emit the same per-command Go for the
    // create/update/delete trio. Compare file-by-file so a divergence
    // names the offending file.
    for (path, hand_src) in &hand_cmds {
        match overlay_cmds.get(path) {
            Some(overlay_src) => assert_eq!(
                overlay_src, hand_src,
                "Go diverged for `{path}` between synth+overlay and hand-rolled"
            ),
            None => panic!("synth+overlay missing expected Go file `{path}`"),
        }
    }
    assert_eq!(
        overlay_cmds.keys().collect::<Vec<_>>(),
        hand_cmds.keys().collect::<Vec<_>>(),
        "synth+overlay and hand-rolled emitted a different set of command Go files"
    );
}
