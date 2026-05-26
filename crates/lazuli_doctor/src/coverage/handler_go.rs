//! `handler_go` coverage layer.
//!
//! Parses Go's `go test -coverprofile=...` output if a `coverage.out`
//! file is present at the project root (or a path declared in
//! `[doctor.coverage]`). Format (per the `cover` tool):
//!
//! ```text
//! mode: set
//! github.com/.../foo.go:1.2,3.4 5 1
//! github.com/.../foo.go:6.7,8.9 4 0
//! ```
//!
//! Each non-mode line carries:
//!
//! - `file:start_line.start_col,end_line.end_col`
//! - `num_statements`
//! - `count` (>0 means "executed")
//!
//! `covered_lines` = sum of `num_statements` where `count > 0`.
//! `total_lines` = sum of `num_statements`.
//!
//! When no coverprofile file is present, the layer reports
//! `total = 0` (vacuous pass) and surfaces `source = "go-coverprofile"`
//! `raw_file = "<looked-at-path>"` so the JSON disclosure is honest.

use std::fs;
use std::path::Path;

use super::LayerCoverage;

/// Canonical filename Go uses by default for `-coverprofile=<f>`. We
/// look at the project root first; users can override via
/// `[doctor.coverage].handler_go_coverprofile` in a future iteration.
pub const DEFAULT_COVERPROFILE: &str = "coverage.out";

/// Compute the `handler_go` coverage layer. When no project root is
/// supplied or no coverprofile is found, returns a vacuous-pass layer
/// (`total = 0`) with `source = "go-coverprofile"` so the disclosure
/// remains honest.
pub fn compute(project_root: Option<&Path>) -> LayerCoverage {
    let Some(root) = project_root else {
        return LayerCoverage::new(0, 0).with_source("go-coverprofile");
    };
    let candidates = [
        root.join(DEFAULT_COVERPROFILE),
        root.join("dist").join("go").join(DEFAULT_COVERPROFILE),
        root.join("app").join(DEFAULT_COVERPROFILE),
    ];
    let coverprofile_path = candidates.iter().find(|p| p.exists()).cloned();
    let Some(path) = coverprofile_path else {
        let mut layer = LayerCoverage::new(0, 0).with_source("go-coverprofile");
        layer.raw_file = Some(format!(
            "absent: {}",
            root.join(DEFAULT_COVERPROFILE).display()
        ));
        return layer;
    };
    let Ok(contents) = fs::read_to_string(&path) else {
        let mut layer = LayerCoverage::new(0, 0).with_source("go-coverprofile");
        layer.raw_file = Some(format!("unreadable: {}", path.display()));
        return layer;
    };
    let (covered, total) = parse_coverprofile(&contents);
    let mut layer = LayerCoverage::new(covered, total).with_source("go-coverprofile");
    layer.raw_file = Some(path.display().to_string());
    layer
}

/// Parses Go's coverprofile format. Returns `(covered_statements,
/// total_statements)`.
///
/// ## Examples
///
/// ```rust
/// use lazuli_doctor::coverage::handler_go::parse_coverprofile;
///
/// let body = "mode: set\n\
///     github.com/x/foo.go:1.0,3.0 5 1\n\
///     github.com/x/foo.go:4.0,6.0 3 0\n";
/// let (covered, total) = parse_coverprofile(body);
/// assert_eq!(total, 8);
/// assert_eq!(covered, 5);
/// ```
pub fn parse_coverprofile(contents: &str) -> (usize, usize) {
    let mut total = 0usize;
    let mut covered = 0usize;
    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("mode:") {
            continue;
        }
        // `file:Lstart.Cstart,Lend.Cend numstmt count`
        let mut iter = trimmed.split_ascii_whitespace();
        let Some(_span) = iter.next() else { continue };
        let Some(num_stmt_s) = iter.next() else { continue };
        let Some(count_s) = iter.next() else { continue };
        let Ok(num_stmt) = num_stmt_s.parse::<usize>() else {
            continue;
        };
        let Ok(count) = count_s.parse::<i64>() else {
            continue;
        };
        total += num_stmt;
        if count > 0 {
            covered += num_stmt;
        }
    }
    (covered, total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_project_root_yields_zero_total() {
        let l = compute(None);
        assert_eq!(l.total, 0);
        assert_eq!(l.covered, 0);
    }

    #[test]
    fn parse_coverprofile_handles_mixed_lines() {
        let contents = "mode: set\n\
            github.com/x/foo.go:1.0,3.0 5 1\n\
            github.com/x/foo.go:4.0,6.0 3 0\n\
            github.com/x/bar.go:1.0,2.0 2 7\n";
        let (covered, total) = parse_coverprofile(contents);
        assert_eq!(total, 10);
        assert_eq!(covered, 7);
    }

    #[test]
    fn missing_coverprofile_marks_raw_file_absent() {
        let tmp = tempfile::tempdir().unwrap();
        let l = compute(Some(tmp.path()));
        assert_eq!(l.total, 0);
        assert!(l.raw_file.as_deref().unwrap().starts_with("absent:"));
    }

    #[test]
    fn coverprofile_at_project_root_parsed() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join(DEFAULT_COVERPROFILE),
            "mode: set\n\
             github.com/x/foo.go:1.0,3.0 4 1\n\
             github.com/x/foo.go:4.0,6.0 2 0\n",
        )
        .unwrap();
        let l = compute(Some(tmp.path()));
        assert_eq!(l.total, 6);
        assert_eq!(l.covered, 4);
    }
}
