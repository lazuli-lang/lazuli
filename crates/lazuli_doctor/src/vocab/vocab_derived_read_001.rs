//! VOCAB-DERIVED-READ-001 — handler-computed read-only field drift.
//!
//! Fires when a resource field is:
//!   - optional (not `required`)
//!   - has no explicit `default` value
//!   - has no `@cap.*` capability tier
//!   - has no existing `derived from <expr>` annotation
//!   - is NEVER assigned a value in any `command.creates.<field> =`,
//!     `command.updates.<field> =`, `job.creates.<field> =`, or
//!     `job.updates.<field> =` write site
//!
//! These fields are likely computed every read — the vocabulary already
//! names `derived from <expression>` for exactly this intent
//! (docs/invariants.md:89-92).
//!
//! Detection is IR-walk only (no handler Go source needed).
//!
//! Severity: `warning` (strict-profile), `warning` (production-profile).
//! Lower severity than VOCAB-UNION-001 because materialised computed values
//! are a legitimate optimisation for indexability.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use lazuli_ir::{self as ir, CommandEffect, Feature, TypeRef};

// ── output ───────────────────────────────────────────────────────────────────

/// One VOCAB-DERIVED-READ-001 finding: a resource field that is never
/// written by any command or declarative job and should likely be
/// `derived from <expr>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file.
    pub path: PathBuf,
    /// Resource name.
    pub resource: String,
    /// Field name flagged as never-written.
    pub field: String,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "VOCAB-DERIVED-READ-001";

    /// Render the "field is never written" message and prompt the
    /// author to switch to `derived from <expr>` if it's computed.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::vocab::vocab_derived_read_001::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("f.lzi"),
    ///     resource: "Order".into(),
    ///     field: "total_with_tax".into(),
    /// };
    /// assert!(f.message().contains("derived from"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "field `{}.{}` is never written by any command or job — \
             if it is computed at read time, consider `derived from <expr>` \
             (docs/invariants.md:89-92). If the field is intentionally a \
             read-only/materialized column, add \
             `@doctor.allow(VOCAB-DERIVED-READ-001, reason: \"...\")` near the resource.",
            self.resource, self.field,
        )
    }
}

// ── detection ────────────────────────────────────────────────────────────────

/// Run VOCAB-DERIVED-READ-001 over one feature's resources.
///
/// `path` is the source `.lzi` file; no I/O is performed here.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::vocab::vocab_derived_read_001::check;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower a feature with resources + commands");
/// let _ = check(&feature, Path::new("billing.lzi"));
/// ```
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    // Waiver wiring (spec 0028): honor the canonical escape hatch the
    // message advertises — `@doctor.allow(VOCAB-DERIVED-READ-001, reason: "…")`
    // (or the legacy `# doctor:allow VOCAB-DERIVED-READ-001`) — so an
    // intentionally read-only/materialized field is suppressible. Without
    // this the message lied: the opt-out was inert and the finding fired
    // regardless. Read failures degrade to "no opt-out applied".
    if crate::allow_comment::file_contains_doctor_allow(path, Finding::CODE) {
        return Vec::new();
    }
    let written = collect_write_sites(&feature.commands, &feature.jobs);
    feature
        .resources
        .iter()
        .flat_map(|r| check_resource(r, &written, path))
        .collect()
}

// ── internals ────────────────────────────────────────────────────────────────

/// Sentinel inserted into the write-set when a command uses `creates X from
/// input`, signalling that field-level write enumeration is unavailable and
/// the entire resource should be treated as fully written.
const FROM_INPUT_SENTINEL: &str = "\0from_input";

/// Check a single resource for never-written optional fields.
fn check_resource(
    resource: &ir::Resource,
    written: &BTreeMap<String, BTreeSet<String>>,
    path: &Path,
) -> Vec<Finding> {
    let mut out = Vec::new();

    // If any command/job does `creates <Resource> from input`, all fields of
    // that resource may be written via the input mapping; skip to avoid FPs.
    if written
        .get(&resource.name)
        .is_some_and(|s| s.contains(FROM_INPUT_SENTINEL))
    {
        return out;
    }

    let written_here = written.get(&resource.name);
    for field in &resource.fields {
        if should_skip(field, written_here) {
            continue;
        }
        out.push(Finding {
            path: path.to_path_buf(),
            resource: resource.name.clone(),
            field: field.name.clone(),
        });
    }
    out
}

/// Returns `true` if the field should NOT trigger the diagnostic.
fn should_skip(field: &ir::Field, written_fields: Option<&BTreeSet<String>>) -> bool {
    // Primary key — runtime-managed, never a derived candidate.
    if field.name == "id" {
        return true;
    }
    // Already annotated `derived from <expr>` — nothing to suggest.
    if field.derived_from.is_some() {
        return true;
    }
    // Required fields must be set on creates — a "never written" signal here
    // is more likely a different bug than a derived-field opportunity.
    if field.required {
        return true;
    }
    // Explicit `default` implies intentional storage semantics.
    if field.default.is_some() {
        return true;
    }
    // `@cap.*` capability tiers imply storage semantics incompatible with
    // `derived from` (e.g. @cap.Hashed stores a hash, @cap.Token issues tokens).
    if matches!(&field.type_ref, TypeRef::Capability(_)) {
        return true;
    }
    // Field is explicitly assigned in at least one write site.
    if written_fields.is_some_and(|s| s.contains(&field.name)) {
        return true;
    }
    false
}

/// Collect all field names assigned in `creates`/`updates` effects across
/// commands and declarative jobs, keyed by the local resource name.
fn collect_write_sites(
    commands: &[ir::Command],
    jobs: &[ir::Job],
) -> BTreeMap<String, BTreeSet<String>> {
    let mut written: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

    for cmd in commands {
        add_effect_writes(&cmd.effect, &mut written);
    }
    for job in jobs {
        if let ir::JobBody::Declarative(decl) = &job.body {
            add_effect_writes(&decl.effect, &mut written);
        }
    }
    written
}

fn add_effect_writes(effect: &CommandEffect, written: &mut BTreeMap<String, BTreeSet<String>>) {
    match effect {
        CommandEffect::Creates(c) => {
            let entry = written.entry(c.resource.name.clone()).or_default();
            for a in &c.assignments {
                entry.insert(a.field.clone());
            }
            // `creates X from input` writes fields via the input type mapping;
            // we can't enumerate them, so insert a sentinel to skip the resource.
            if c.from_input {
                entry.insert(FROM_INPUT_SENTINEL.to_owned());
            }
        }
        CommandEffect::Updates(u) => {
            let entry = written.entry(u.resource.name.clone()).or_default();
            for a in &u.assignments {
                entry.insert(a.field.clone());
            }
        }
        CommandEffect::Deletes(_)
        | CommandEffect::Reorders(_)
        | CommandEffect::Returns(_)
        | CommandEffect::None => {}
    }
}

// ── tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    include!("vocab_derived_read_001_tests.rs");
}
