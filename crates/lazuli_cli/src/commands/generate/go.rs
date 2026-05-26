//! `lazuli generate go` — emit Lazuli Go user-code from the typed IR.
//!
//! Walks `lazuli_codegen_go::generate_v1` (with optional source-map
//! context when `--with-source` is set), runs the closed §6.2.1 error
//! catalog as a pre-write gate (errors abort, warnings stream to
//! stderr), then writes the multi-file emitter output:
//!
//! - `<out>/dist/go/**.gen.go` — codegen-owned, fully overwritable.
//! - `<out>/migrations/*.sql` — codegen-owned, nuke-and-regen on every
//!   run (the dir is gitignored canonically; users don't author here).
//! - `app/features/<feature>/<name>.go` — handler stubs, scaffold-once
//!   and never overwrite. Legacy layouts (`dist/go/<f>/<n>.go`,
//!   flat-app) also count as "stub already exists".
//! - `go.work` — preserved-merge: existing `use` directives stay, new
//!   ones added.
//!
//! `--check` short-circuits the write step and only enumerates what
//! would be emitted, returning 1 if the §6.2.1 catalog already
//! aborted. `--out` is required because the emitter produces multiple
//! files — there is no stdout fallback.
//!
//! Cross-refs:
//! - `lazuli_codegen_go::generate_v1` / `generate_v1_with_manifest` —
//!   the actual emitter.
//! - `lazuli_codegen_go::emitter::check` — the §6.2.1 error catalog.
//! - `crate::write_generated_file`,
//!   `crate::write_go_work_preserving_entries` — the file-write
//!   primitives shared with `generate_ts`.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

use crate::{
    build_module_from_path, build_module_with_source_from_path, codegen_lazurite_manifest,
    collect_plan_gate_facts_for_generate, default_go_module_name, lazurite_manifest,
    project_root_for_input, write_generated_file, write_go_work_preserving_entries,
};

/// Handler for `GenerateKind::Go`.
///
/// Walks `generate_v1`/`generate_v1_with_manifest`, runs the §6.2.1
/// closed error catalog as a pre-write gate, and writes the multi-file
/// emitter output under `<out>/dist/go/`, `<out>/migrations/`, and the
/// merged `go.work`. `check` short-circuits to enumerate-only mode.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_cli::commands::generate::go::generate_go;
///
/// // generate_go(Path::new("."), Some(Path::new(".")), None, None, true, false, false)?;
/// ```
#[allow(clippy::too_many_arguments)]
pub fn generate_go(
    input: &Path,
    output: Option<&Path>,
    module: Option<&str>,
    lazuli_go_version: Option<&str>,
    check: bool,
    with_source: bool,
    allow_drops: bool,
) -> Result<()> {
    // Cell A11 — `--allow-drops` gates the ALTER migration emitter's
    // treatment of `SchemaDiff.drops`. Without the flag, drops are
    // emitted as commented-out lines under a WARNING header so authors
    // explicitly opt in to destructive ALTERs. With the flag, the drops
    // become live `DROP COLUMN IF EXISTS` statements.
    //
    // The diff-vs-baseline orchestration that produces a non-empty
    // `SchemaDiff` lives in cell A10 (`schema_diff.rs`). Once A10 lands,
    // wire it here: read `migrations/` from `out_dir`, parse the latest
    // CREATE TABLE per resource, compare to the IR, hand the diff +
    // `AlterEmitOptions { allow_drops }` to
    // `lazuli_codegen_go::emitter::migration_ddl::emit_alter_migration_file`,
    // and append the returned (up, down) pair to `files`. Until A10 is
    // in tree, `--allow-drops` is accepted on the CLI but has no
    // observable effect because no diff is computed.
    let alter_options = lazuli_codegen_go::emitter::migration_ddl::AlterEmitOptions { allow_drops };
    // `_ = alter_options;` suppresses dead_code while A10 is in flight;
    // delete this discard when A10's caller wires `emit_alter_migration_file`.
    let _ = alter_options;
    let project_root = project_root_for_input(input);
    let manifest = lazurite_manifest::load(&project_root).with_context(|| {
        format!(
            "failed to read {}",
            project_root.join("Lazurite.toml").display()
        )
    })?;
    let (module_ir, source_context) = if with_source {
        let (module_ir, source_map, feature_file_ids) = build_module_with_source_from_path(input)?;
        (module_ir, Some((source_map, feature_file_ids)))
    } else {
        (build_module_from_path(input)?, None)
    };
    // Frente 1 — `[generate.go]` defaults apply transparently when the
    // block is absent; pilots no longer need to author the section just
    // to pin the canonical `out = "dist/go"` value.
    let manifest_out = manifest
        .as_ref()
        .map(|m| project_root.join(m.generate_go_or_default().out));
    let out_dir = output.or(manifest_out.as_deref());
    let codegen_manifest = manifest
        .as_ref()
        .map(|manifest| codegen_lazurite_manifest(manifest, &project_root, out_dir));

    let module_name = match module {
        Some(name) => name.to_owned(),
        None => default_go_module_name(&module_ir),
    };
    let go_version = lazuli_go_version
        .map(|s| s.to_owned())
        .unwrap_or_else(|| lazuli_codegen_go::LAZULI_GO_VERSION.to_owned());

    // Closed §6.2.1 error catalog (CODEGEN-GO-PLUGIN-001,
    // CODEGEN-GO-TYPE-007, …). Run BEFORE codegen so the emitter never
    // produces broken Go for a module that already fails policy. Errors
    // abort the run; warnings stream to stderr but still allow emission.
    let issues = lazuli_codegen_go::emitter::check::run_checks(&module_ir);
    let errors: Vec<_> = issues
        .iter()
        .filter(|i| {
            matches!(
                i.severity,
                lazuli_codegen_go::emitter::check::Severity::Error
            )
        })
        .collect();
    let warnings: Vec<_> = issues
        .iter()
        .filter(|i| {
            !matches!(
                i.severity,
                lazuli_codegen_go::emitter::check::Severity::Error
            )
        })
        .collect();
    for w in &warnings {
        eprintln!(
            "[{}] warn: {}{}{}",
            w.code,
            w.message,
            w.feature
                .as_deref()
                .map(|f| format!(" (feature `{f}`)"))
                .unwrap_or_default(),
            w.site
                .as_deref()
                .map(|s| format!(" at {s}"))
                .unwrap_or_default(),
        );
    }
    if !errors.is_empty() {
        for e in &errors {
            eprintln!(
                "[{}] error: {}{}{}",
                e.code,
                e.message,
                e.feature
                    .as_deref()
                    .map(|f| format!(" (feature `{f}`)"))
                    .unwrap_or_default(),
                e.site
                    .as_deref()
                    .map(|s| format!(" at {s}"))
                    .unwrap_or_default(),
            );
        }
        anyhow::bail!(
            "lazuli generate go: {} blocking issue(s) in the closed codegen error catalog",
            errors.len()
        );
    }

    // PG.C — compute plan-and-gate facts from the .lzi sources so
    // codegen emits `dist/go/plan/catalog.gen.go` when the package
    // authors plans.
    let plan_gate = collect_plan_gate_facts_for_generate(input);

    let options = lazuli_codegen_go::GoEmitOptions {
        module_name: Some(module_name),
        lazuli_go_version: go_version,
        check,
        plan_gate,
    };
    let files = if let Some((source_map, feature_file_ids)) = source_context.as_ref() {
        lazuli_codegen_go::generate_v1_with_manifest_and_source(
            &module_ir,
            &options,
            codegen_manifest.as_ref(),
            lazuli_codegen_go::GoSourceContext {
                source_map,
                feature_file_ids,
            },
        )
    } else {
        lazuli_codegen_go::generate_v1_with_manifest(
            &module_ir,
            &options,
            codegen_manifest.as_ref(),
        )
    };

    if check {
        // Coarse pass/fail signal (catalog above already aborted on
        // Error severity; the closed §6.2.1 catalog continues to grow
        // in cell I4). Enumerates what would be written.
        println!("lazuli generate go --check");
        println!("would emit {} file(s):", files.len());
        for file in &files {
            println!("  {}", file.path);
        }
        return Ok(());
    }

    let out_dir = out_dir.ok_or_else(|| {
        anyhow::anyhow!(
            "`lazuli generate go` requires --out <dir>; the emitter writes multiple files"
        )
    })?;

    fs::create_dir_all(out_dir)
        .with_context(|| format!("creating output directory {}", out_dir.display()))?;

    // Hostpoint deploy 2026-05-25 surfaced this: previous codegen runs
    // emitted resource X at sequence 001; a later run with new
    // dependency edges moved X to sequence 005. The new file landed
    // (005_*.sql), but the stale 001_*.sql kept sitting on disk — the
    // codegen only OVERWRITES, never DELETES. Pilots that just iterate
    // `dist/go/migrations/*.sql` (which is the documented migrate
    // pattern) ended up applying both the stale and the current
    // version of the same resource, in the wrong topological order,
    // and the stale one's FK to a not-yet-created table blew up
    // production deploys.
    //
    // Semantics now: `<out_dir>/migrations/` is FULLY OWNED by codegen.
    // Before writing the new generation, we nuke every existing
    // `.sql` and `.down.sql` file in that directory. The
    // gitignore on `dist/go/migrations/` (canonical in every Lazurite
    // scaffold) is the contract — users don't author files there,
    // they author in `app/migrations/` for hand-rolled SQL or via
    // a future `lazuli generate migration` command.
    //
    // If the run emits ZERO migration files (e.g. a feature-less
    // module), we still clean — the dir SHOULD be empty in that
    // case, and a leftover from a prior run would be just as wrong.
    let migrations_dir = out_dir.join("migrations");
    if migrations_dir.exists() {
        let entries = fs::read_dir(&migrations_dir).with_context(|| {
            format!(
                "reading migrations dir {} to clean stale files before regen",
                migrations_dir.display()
            )
        })?;
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            let is_sql = path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.eq_ignore_ascii_case("sql"))
                .unwrap_or(false);
            if is_sql {
                fs::remove_file(&path).with_context(|| {
                    format!(
                        "removing stale migration file {} before regen",
                        path.display()
                    )
                })?;
            }
        }
    }

    let mut handler_stubs_written = 0usize;
    let mut handler_stubs_skipped = 0usize;
    for file in &files {
        if file.path == "go.work" {
            write_go_work_preserving_entries(&project_root, &file.contents)?;
        } else if file.path.starts_with("app/features/") {
            // Handler stubs are Tier 1 portable code under
            // `app/features/<feature>/<name>.go` — written to the
            // project root, NOT under the codegen `out_dir`. They're
            // user territory once authored, so we skip files that
            // already exist (idempotent: scaffold-once, never
            // overwrite). See `docs/project-structure.md`.
            let target = project_root.join(&file.path);
            if target.exists() {
                handler_stubs_skipped += 1;
                continue;
            }
            // Legacy fallbacks — pre-pivot scaffolds had handlers at:
            //   1. `dist/go/<f>/<name>.go` (first failed pivot)
            //   2. `app/features/<f>/<name>.go` (flat layout, no
            //      `handlers/` sub-folder)
            // Don't overwrite either — consumer migration relocates
            // them deliberately. Both translations skip the
            // `handlers/` segment that the canonical path carries.
            let canonical = &file.path;
            let mut legacy_skipped = false;
            if let Some(after_features) = canonical.strip_prefix("app/features/") {
                if let Some((feature, after_feature)) = after_features.split_once('/') {
                    if let Some(name) = after_feature.strip_prefix("handlers/") {
                        let legacy_flat_app = format!("app/features/{feature}/{name}");
                        let legacy_dist = format!("dist/go/{feature}/{name}");
                        for legacy in [legacy_flat_app, legacy_dist] {
                            if project_root.join(&legacy).exists() {
                                handler_stubs_skipped += 1;
                                legacy_skipped = true;
                                break;
                            }
                        }
                    }
                }
            }
            if legacy_skipped {
                continue;
            }
            write_generated_file(&project_root, &file.path, &file.contents)?;
            handler_stubs_written += 1;
        } else {
            write_generated_file(out_dir, &file.path, &file.contents)?;
        }
    }

    let codegen_count = files.len() - handler_stubs_written - handler_stubs_skipped;
    println!("wrote {} file(s) to {}", codegen_count, out_dir.display());
    if handler_stubs_written > 0 {
        println!(
            "wrote {} handler stub(s) to {}/app/features/",
            handler_stubs_written,
            project_root.display(),
        );
    }
    if handler_stubs_skipped > 0 {
        println!(
            "skipped {} existing handler stub(s) (user-authored)",
            handler_stubs_skipped,
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_input_surfaces_error() {
        let result = generate_go(
            Path::new("__lazuli_no_such_input.lzi"),
            Some(Path::new(".")),
            None,
            None,
            true,
            false,
            false,
        );
        assert!(result.is_err());
    }
}
