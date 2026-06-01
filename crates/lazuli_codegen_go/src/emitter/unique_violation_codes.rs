//! Per-constraint domain-code registration glue (`<feature>/unique_codes.gen.go`).
//!
//! The unique-constraint twin of the spec-0014 `restrict on_delete ... error
//! <CODE>` seam. For every `unique a, b error <CODE>` constraint on a resource
//! this emits an `init()` that calls
//! `lazuli.RegisterUniqueViolationCode("<constraint_name>", "<CODE>")`, binding
//! the deterministically-named Postgres UNIQUE constraint to the pilot's
//! domain error code. At runtime `classifyDBError` consults that registry on a
//! 23505 and surfaces `<CODE>` (HTTP 409) instead of the generic
//! `unique_violation`.
//!
//! The constraint NAME is produced by `migration_ddl::unique_violation_codes`
//! — the SAME function family that names the emitted DDL constraint — so the
//! registered key is byte-identical to the `pgErr.ConstraintName` Postgres
//! reports. This is the load-bearing correctness property: if these two names
//! could drift, the remap would silently miss.

use lazuli_ir::Feature;

use super::migration_ddl::unique_violation_codes;
use super::printer::GoPrinter;

/// Emit `<feature>/unique_codes.gen.go`, or `None` when no resource in the
/// feature declares a `unique ... error <CODE>` constraint (so `module.rs`
/// skips the file entirely, keeping the output listing signal-rich).
pub fn emit_unique_violation_codes_file(source_label: &str, feature: &Feature) -> Option<String> {
    // Gather (constraint_name, domain_code) pairs across all resources, sorted
    // by resource name then constraint name for byte-stable output regardless
    // of IR vec order.
    let mut resources: Vec<_> = feature.resources.iter().collect();
    resources.sort_by(|a, b| a.name.cmp(&b.name));

    let mut pairs: Vec<(String, String)> = Vec::new();
    for resource in resources {
        let mut resource_pairs = unique_violation_codes(resource);
        resource_pairs.sort_by(|a, b| a.0.cmp(&b.0));
        pairs.extend(resource_pairs);
    }
    if pairs.is_empty() {
        return None;
    }

    let mut p = GoPrinter::new();
    p.banner(source_label, &super::casing::gen_package_name(&feature.name));
    p.line("import \"lazuli.dev/runtime/lazuli\"");
    p.blank();
    p.line("// init registers the per-constraint domain error codes authored via");
    p.line("// `unique <fields> error <CODE>`. classifyDBError remaps a 23505 whose");
    p.line("// ConstraintName matches one of these into the pinned domain code.");
    p.line("func init() {");
    p.indent();
    for (constraint_name, code) in &pairs {
        p.line(&format!(
            "lazuli.RegisterUniqueViolationCode({constraint_name:?}, {code:?})"
        ));
    }
    p.dedent();
    p.line("}");

    Some(p.finish())
}
