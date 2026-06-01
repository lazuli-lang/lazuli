//! Pins the `lazuli_way` teaching surface to reality.
//!
//! `docs/lazuli_way.md` is the index every authoring agent reads to learn the
//! idioms. If it links a slug whose file is missing, the DoD doc disappears,
//! the scaffold stops teaching idioms, or the `@fn` SQL-in-Go loophole reopens,
//! the teaching surface has silently rotted — these tests fail in that case.
//! See `.specs/changes/0001-teaching-spine/`.

use std::path::PathBuf;

/// Repo root: `crates/lazuli_cli/` -> two levels up from the manifest dir.
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn read(rel: &str) -> String {
    let path = repo_root().join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// Pull every `lazuli_way/<slug>.md` target out of a markdown link in the index.
fn linked_slug_files(index: &str) -> Vec<String> {
    let mut out = Vec::new();
    for (i, _) in index.match_indices("(lazuli_way/") {
        let rest = &index[i + 1..]; // skip the '('
        if let Some(end) = rest.find(')') {
            out.push(rest[..end].to_string());
        }
    }
    out
}

#[test]
fn index_links_resolve() {
    let index = read("docs/lazuli_way.md");
    let links = linked_slug_files(&index);
    assert!(
        !links.is_empty(),
        "lazuli_way.md must link at least one idiom file under lazuli_way/"
    );
    for link in &links {
        let path = repo_root().join("docs").join(link);
        assert!(
            path.exists(),
            "lazuli_way.md links `{link}` but {} does not exist",
            path.display()
        );
    }

    // The frozen link set — downstream specs fill files, never add rows.
    let frozen = [
        "crud-by-convention",
        "escape-hatch-decision-tree",
        "feature-defaults",
        "field-policy",
        "one-feature-one-capability",
        "referential-guards",
        "soft-delete",
        "money",
        "state-machines",
        "comment-hygiene",
        "delegate-to-runtime",
    ];
    for slug in frozen {
        let want = format!("lazuli_way/{slug}.md");
        assert!(
            links.contains(&want),
            "lazuli_way.md index must link the frozen slug `{want}`"
        );
    }
}

#[test]
fn dod_doc_present() {
    let dod = read("docs/lazuli_way/definition-of-done.md");
    for needle in ["BUILD:", "MIGRATE:", "TEACH:", "ENFORCE:"] {
        assert!(
            dod.contains(needle),
            "definition-of-done.md must contain the gate line `{needle}`"
        );
    }
}

#[test]
fn scaffold_teaches_idioms() {
    for name in ["CLAUDE.md.tmpl", "AGENTS.md.tmpl"] {
        let body = read(&format!("lazurite/templates/default/{name}"));
        assert!(
            body.contains("## Authoring idioms"),
            "{name} must contain an `## Authoring idioms` section"
        );
        assert!(
            body.contains("lazuli_way"),
            "{name} must link the lazuli_way canon"
        );
        assert!(
            body.contains("conventions [crud]"),
            "{name} must teach `conventions [crud]`"
        );
    }
}

/// The runtime-surface index (mechanism ii of the reinvention defense) must stay
/// referenced from the context pack at all three injection sites — quickref + both
/// scaffold templates — so the injection can't be silently dropped.
#[test]
fn quickref_and_scaffold_reference_runtime_surface() {
    const NEEDLE: &str = "docs/lazuli_way/runtime-surface.md";
    for rel in [
        "docs/quickref.md",
        "lazurite/templates/default/CLAUDE.md.tmpl",
        "lazurite/templates/default/AGENTS.md.tmpl",
    ] {
        let body = read(rel);
        assert!(
            body.contains(NEEDLE),
            "{rel} must reference the runtime-surface index `{NEEDLE}`"
        );
    }
}

/// The keystone teaching doc ties all three mechanisms together; this asserts it
/// is filled (not a stub): the fixed shape, the canonical hash_password case, the
/// runtime-surface link, and the doctor rule that backs it.
#[test]
fn delegate_to_runtime_doc_filled() {
    let body = read("docs/lazuli_way/delegate-to-runtime.md");
    for needle in [
        "## Reach for this",
        "auth.HashPassword",
        "runtime-surface.md",
        "VOCAB-RUNTIME-REINVENTED-001",
    ] {
        assert!(
            body.contains(needle),
            "delegate-to-runtime.md must contain `{needle}` (keystone doc, not a stub)"
        );
    }
}

#[test]
fn scaffold_templates_stay_identical() {
    let claude = read("lazurite/templates/default/CLAUDE.md.tmpl");
    let agents = read("lazurite/templates/default/AGENTS.md.tmpl");
    assert_eq!(
        claude, agents,
        "CLAUDE.md.tmpl and AGENTS.md.tmpl must stay byte-identical"
    );
}

#[test]
fn escape_hatch_clause_closed() {
    let body = read("lazurite/templates/default/CLAUDE.md.tmpl");
    // The old loophole phrasing licensed arbitrary @fn logic.
    assert!(
        !body.contains("derived field computation, multi-step orchestration"),
        "CLAUDE.md.tmpl still contains the old @fn loophole phrasing"
    );
    // The closed rule: raw SQL must be declared, never inside a @fn.
    assert!(
        body.contains("query.sql") && body.contains("query.compose"),
        "CLAUDE.md.tmpl must point raw SQL at query.sql/query.compose"
    );
    assert!(
        body.contains("NEVER as a SQL string literal inside a `@fn`"),
        "CLAUDE.md.tmpl must forbid raw SQL string literals inside a @fn handler"
    );
}

#[test]
fn seed_feature_demonstrates_idioms() {
    let seed = read("lazurite/templates/default/app/features/note/note.lzi");
    assert!(
        seed.contains("conventions [crud]"),
        "seed note feature must use `conventions [crud]`"
    );
    assert!(
        seed.contains("defaults"),
        "seed note feature must use a `defaults` block"
    );
    assert!(
        repo_root()
            .join("lazurite/templates/default/app/features/note/note.lzx")
            .exists(),
        "seed note feature must ship a note.lzx surface"
    );
}
