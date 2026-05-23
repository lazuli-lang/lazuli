//! Import accumulator. Buckets entries into three groups (stdlib /
//! lazuli runtime / third-party) and emits a blank-line-separated
//! `import (...)` block mirroring `goimports` default behaviour. We
//! classify by prefix, not heuristic parsing — the closed catalog is
//! tight enough.
//!
//! Per proposal §2.4 "Determinism": ordering is alphabetical inside
//! each group; duplicates collapse via `BTreeSet`. Per §11, every
//! Lazuli reference resolves through this layer, never as a raw
//! string write into `printer.line`.

use std::collections::{BTreeMap, BTreeSet};

use super::printer::GoPrinter;

/// The canonical Lazuli Go module root. Every `lazuli.dev/runtime/…`
/// import classifies as "lazuli" so the printer emits it in the
/// middle group, after stdlib and before third-party.
const LAZULI_RUNTIME_PREFIX: &str = "lazuli.dev/runtime/";

/// Accumulates Go import paths into three sorted, dedup'd buckets.
/// Cheap to construct; the emitter usually creates one per generated
/// file and finalises it before writing the file body.
#[derive(Debug, Default, Clone)]
pub struct ImportSet {
    stdlib: BTreeSet<String>,
    lazuli: BTreeSet<String>,
    third_party: BTreeSet<String>,
    /// Explicitly-aliased third-party imports — emitted as
    /// `alias "path"`. Used when the path's last segment is not a
    /// valid Go identifier (e.g. `lazuli.dev/plugin/scalars-br` →
    /// `scalarsbr "lazuli.dev/plugin/scalars-br"`).
    third_party_aliased: BTreeMap<String, String>,
}

impl ImportSet {
    pub fn new() -> Self {
        Self::default()
    }

    /// Classify and add an import path. Empty paths and duplicates
    /// are silently dropped. Classification rules (proposal §2.4):
    /// - stdlib: first segment has no `.` (e.g. `time`, `embed`,
    ///   `encoding/json`)
    /// - lazuli: starts with `lazuli.dev/runtime/`
    /// - third_party: everything else (includes `github.com/...`,
    ///   `golang.org/x/...`, `cloud.google.com/...`)
    pub fn add(&mut self, path: &str) {
        if path.is_empty() {
            return;
        }
        let owned = path.to_owned();
        if owned.starts_with(LAZULI_RUNTIME_PREFIX) {
            self.lazuli.insert(owned);
        } else if is_stdlib_path(&owned) {
            self.stdlib.insert(owned);
        } else {
            self.third_party.insert(owned);
        }
    }

    /// Add a third-party import with an explicit local alias. The
    /// alias is the identifier used in source code (e.g. `scalarsbr`)
    /// and the path is the import path (e.g.
    /// `lazuli.dev/plugin/scalars-br`). Two adds with the same alias
    /// + path collapse; adds with the same alias but a different
    /// path are unsupported (the second silently overwrites — caller
    /// must avoid alias collisions).
    pub fn add_aliased(&mut self, alias: &str, path: &str) {
        if alias.is_empty() || path.is_empty() {
            return;
        }
        self.third_party_aliased
            .insert(alias.to_owned(), path.to_owned());
    }

    /// `true` when no imports are queued. The emitter uses this to
    /// skip emitting an empty `import ()` block (which `gofmt` would
    /// remove).
    pub fn is_empty(&self) -> bool {
        self.stdlib.is_empty()
            && self.lazuli.is_empty()
            && self.third_party.is_empty()
            && self.third_party_aliased.is_empty()
    }

    /// Emit the full `import (...)` block. Groups separated by a
    /// single blank line; entries inside each group are
    /// alphabetically sorted (the `BTreeSet` guarantees that for
    /// free).
    pub fn emit(&self, p: &mut GoPrinter) {
        if self.is_empty() {
            return;
        }
        // The block sits at column 0; entries get one tab indent.
        p.line("import (");
        p.indent();
        let mut wrote_any = false;
        wrote_any |= write_group(p, &self.stdlib, false);
        wrote_any |= write_group(p, &self.lazuli, wrote_any);
        wrote_any |= write_group(p, &self.third_party, wrote_any);
        let _ = write_aliased_group(p, &self.third_party_aliased, wrote_any);
        p.dedent();
        p.line(")");
    }
}

fn write_group(p: &mut GoPrinter, group: &BTreeSet<String>, needs_separator: bool) -> bool {
    if group.is_empty() {
        return false;
    }
    if needs_separator {
        p.blank();
    }
    for path in group {
        p.line(&format!("\"{}\"", path));
    }
    true
}

fn write_aliased_group(
    p: &mut GoPrinter,
    group: &BTreeMap<String, String>,
    needs_separator: bool,
) -> bool {
    if group.is_empty() {
        return false;
    }
    if needs_separator {
        p.blank();
    }
    for (alias, path) in group {
        p.line(&format!("{alias} \"{path}\""));
    }
    true
}

/// stdlib paths have no `.` in the first segment (e.g. `encoding/json`
/// vs `github.com/foo/bar`). This mirrors how `goimports` recognises
/// the standard library without bundling the full list.
fn is_stdlib_path(path: &str) -> bool {
    let first = path.split('/').next().unwrap_or(path);
    !first.contains('.')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_stdlib_lazuli_thirdparty() {
        let mut imports = ImportSet::new();
        imports.add("time");
        imports.add("encoding/json");
        imports.add("lazuli.dev/runtime/lazuli");
        imports.add("github.com/jackc/pgx/v5");
        assert_eq!(imports.stdlib.len(), 2);
        assert_eq!(imports.lazuli.len(), 1);
        assert_eq!(imports.third_party.len(), 1);
    }

    #[test]
    fn deduplicates_repeated_paths() {
        let mut imports = ImportSet::new();
        imports.add("time");
        imports.add("time");
        imports.add("lazuli.dev/runtime/lazuli");
        imports.add("lazuli.dev/runtime/lazuli");
        assert_eq!(imports.stdlib.len(), 1);
        assert_eq!(imports.lazuli.len(), 1);
    }

    #[test]
    fn empty_path_is_dropped() {
        let mut imports = ImportSet::new();
        imports.add("");
        assert!(imports.is_empty());
    }

    #[test]
    fn emit_produces_three_group_block() {
        let mut imports = ImportSet::new();
        imports.add("github.com/jackc/pgx/v5");
        imports.add("time");
        imports.add("lazuli.dev/runtime/lazuli");
        let mut printer = GoPrinter::new();
        imports.emit(&mut printer);
        let out = printer.finish();
        // Stdlib first, lazuli middle, third-party last; blank
        // lines between groups.
        let expected = "\
import (
\t\"time\"

\t\"lazuli.dev/runtime/lazuli\"

\t\"github.com/jackc/pgx/v5\"
)
";
        assert_eq!(out, expected);
    }

    #[test]
    fn emit_with_only_stdlib_omits_separator() {
        let mut imports = ImportSet::new();
        imports.add("time");
        imports.add("embed");
        let mut printer = GoPrinter::new();
        imports.emit(&mut printer);
        let out = printer.finish();
        // BTreeSet alphabetises: `embed` before `time`. No trailing
        // blank line between groups when only stdlib has entries.
        assert_eq!(out, "import (\n\t\"embed\"\n\t\"time\"\n)\n");
    }

    #[test]
    fn emit_empty_set_writes_nothing() {
        let imports = ImportSet::new();
        let mut printer = GoPrinter::new();
        imports.emit(&mut printer);
        assert_eq!(printer.finish(), String::new());
    }

    #[test]
    fn deterministic_ordering_across_runs() {
        // Insertion order varies; output order must not.
        let mut a = ImportSet::new();
        a.add("github.com/zzz/last");
        a.add("github.com/aaa/first");
        a.add("lazuli.dev/runtime/lazuli/auth");
        a.add("lazuli.dev/runtime/lazuli");

        let mut b = ImportSet::new();
        b.add("lazuli.dev/runtime/lazuli");
        b.add("github.com/aaa/first");
        b.add("github.com/zzz/last");
        b.add("lazuli.dev/runtime/lazuli/auth");

        let mut pa = GoPrinter::new();
        let mut pb = GoPrinter::new();
        a.emit(&mut pa);
        b.emit(&mut pb);
        assert_eq!(pa.finish(), pb.finish());
    }
}
