//! Meta-lint coverage for doctor rule module headers.
//!
//! New correctness and vocab rule modules should explain the rule, its
//! severity, and at least one concrete trigger/example cue before code.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

const RULE_DIRS: &[&str] = &["src/correctness", "src/vocab"];
const SKIP_FILES: &[&str] = &["mod.rs", "tests.rs"];
const TRIGGER_TERMS: &[&str] = &["fixture", "example", "fires when", "warns when"];

// These modules already have module-level `//!` docs, but their header wording
// still needs DOC1 follow-up to include the severity/trigger contract.
const DOC1_FOLLOW_UP_NEEDED: &[&str] = &[
    "src/correctness/channel_payload_unresolved_001.rs",
    "src/correctness/command_input_shadows_field_001.rs",
    "src/correctness/event_group_variant_type_001.rs",
    "src/correctness/full_text_type_001.rs",
    "src/correctness/missing_policy_on_query_001.rs",
    "src/correctness/mutation_without_readback.rs",
    "src/correctness/record_column_storage.rs",
    "src/correctness/resource_lock_contract_001.rs",
    "src/correctness/route_id_effect_consistency.rs",
    "src/correctness/schema_migration_present.rs",
    "src/correctness/updates_missing_updated_at.rs",
    "src/correctness/webhook_emit_predicate_field_001.rs",
    "src/vocab/conventions.rs",
    "src/vocab/money_arithmetic_001.rs",
    "src/vocab/money_compare_001.rs",
    "src/vocab/owner_axis.rs",
    "src/vocab/rate_limit.rs",
    "src/vocab/universal_columns.rs",
    "src/vocab/vocab_audit_002.rs",
    "src/vocab/vocab_event_orphan_001.rs",
    "src/vocab/vocab_event_producer_001.rs",
    "src/vocab/vocab_grammar_form_001.rs",
    "src/vocab/vocab_json_typed_001.rs",
    "src/vocab/vocab_money_multi_currency_001.rs",
    "src/vocab/vocab_union_001.rs",
    "src/vocab/vocab_union_002.rs",
];

#[test]
fn correctness_and_vocab_modules_have_header_docs() {
    let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let follow_up_needed: BTreeSet<&'static str> = DOC1_FOLLOW_UP_NEEDED.iter().copied().collect();
    let mut seen_follow_up = BTreeSet::new();
    let mut modules_walked = 0usize;

    for rule_dir in RULE_DIRS {
        let dir = crate_dir.join(rule_dir);
        walk_rs_files(&dir, &mut |path| {
            if should_skip(path) {
                return;
            }

            modules_walked += 1;
            let rel_path = relative_path(&crate_dir, path);
            let contents = fs::read_to_string(path)
                .unwrap_or_else(|err| panic!("failed to read {rel_path}: {err}"));

            match validate_header(&contents) {
                Ok(()) => {
                    if follow_up_needed.contains(rel_path.as_str()) {
                        seen_follow_up.insert(rel_path);
                    }
                }
                Err(_reason) if follow_up_needed.contains(rel_path.as_str()) => {
                    seen_follow_up.insert(rel_path.clone());
                    assert!(
                        has_module_header(&contents),
                        "{rel_path}: DOC1 follow-up may not grandfather missing module docs"
                    );
                }
                Err(reason) => panic!("{rel_path}: {reason}"),
            }
        });
    }

    assert!(modules_walked > 0, "expected to walk doctor rule modules");

    let missing_follow_up_files: Vec<_> = follow_up_needed
        .iter()
        .filter(|path| !seen_follow_up.contains(**path))
        .copied()
        .collect();
    assert!(
        missing_follow_up_files.is_empty(),
        "DOC1 follow-up allowlist contains files that were not walked: {missing_follow_up_files:?}"
    );
}

fn walk_rs_files(dir: &Path, visit: &mut impl FnMut(&Path)) {
    for entry in fs::read_dir(dir).unwrap_or_else(|err| panic!("failed to read {dir:?}: {err}")) {
        let entry = entry.unwrap_or_else(|err| panic!("failed to read entry in {dir:?}: {err}"));
        let path = entry.path();

        if path.is_dir() {
            walk_rs_files(&path, visit);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            visit(&path);
        }
    }
}

fn should_skip(path: &Path) -> bool {
    path.file_name()
        .and_then(|file_name| file_name.to_str())
        .is_some_and(|file_name| {
            SKIP_FILES.contains(&file_name)
                || file_name.ends_with("_tests.rs")
                // SPEC-19 `include!`-chunk fragments (`<base>_p1.rs`, `_p2.rs`, …)
                // are split-out sections of their parent module, not standalone
                // modules — the parent carries the `//!` header.
                || is_split_chunk(file_name)
        })
}

/// True for a SPEC-19 chunk-fragment filename like `foo_p1.rs` / `foo_p12.rs`.
fn is_split_chunk(file_name: &str) -> bool {
    let Some(stem) = file_name.strip_suffix(".rs") else {
        return false;
    };
    let Some((_, tail)) = stem.rsplit_once("_p") else {
        return false;
    };
    !tail.is_empty() && tail.bytes().all(|b| b.is_ascii_digit())
}

fn validate_header(contents: &str) -> Result<(), &'static str> {
    let header = header_doc_lines(contents);

    if header.is_empty() {
        return Err("first non-empty lines must be `//!` module doc comments");
    }

    let joined = header.join("\n").to_lowercase();
    if !joined.contains("severity") {
        return Err("module header must describe diagnostic severity");
    }

    if !TRIGGER_TERMS.iter().any(|term| joined.contains(term)) {
        return Err(
            "module header must include at least one trigger cue: fixture, example, fires when, warns when",
        );
    }

    Ok(())
}

fn has_module_header(contents: &str) -> bool {
    !header_doc_lines(contents).is_empty()
}

fn header_doc_lines(contents: &str) -> Vec<&str> {
    let mut header = Vec::new();

    for line in contents.lines() {
        let trimmed = line.trim_start();

        if header.is_empty() && trimmed.is_empty() {
            continue;
        }

        if trimmed.starts_with("//!") {
            header.push(trimmed);
            continue;
        }

        break;
    }

    header
}

fn relative_path(crate_dir: &Path, path: &Path) -> String {
    path.strip_prefix(crate_dir)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}
