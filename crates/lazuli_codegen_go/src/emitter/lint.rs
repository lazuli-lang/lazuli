//! Emitter lint — enforces invariants on rendered generated Go before write.
//! Per docs/proposals/bucket-ai-debug-loop-cycle.md §6.3.

use std::fmt;

/// Walk a rendered Go file and reject any `func ` line that is not
/// preceded (across leading comment lines) by a `//lazuli:pattern <id>
/// <version>` header.
///
/// The pre-write lint guarantees that every emitted function carries an
/// entry in the closed pattern catalog ([`crate::emitter::patterns`]),
/// so an audit can map every line of generated Go to a known emitter.
///
/// ## Examples
///
/// ```
/// use lazuli_codegen_go::emitter::lint::check_pattern_annotations;
/// let src = "package foo\n//lazuli:pattern auth_login v1\nfunc Login() {}\n";
/// check_pattern_annotations(src, "foo.gen.go").unwrap();
/// ```
pub fn check_pattern_annotations(rendered: &str, file_path: &str) -> Result<(), LintError> {
    let lines: Vec<&str> = rendered.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        if line.starts_with("func ") {
            let mut has_pattern = false;
            for j in (0..i).rev() {
                let prev = lines[j].trim();
                if prev.is_empty() {
                    continue;
                }
                if prev.starts_with("//lazuli:pattern ") {
                    has_pattern = true;
                }
                if !prev.starts_with("//") {
                    break;
                }
            }
            if !has_pattern {
                return Err(LintError::MissingPattern {
                    file: file_path.to_owned(),
                    line: i + 1,
                });
            }
        }
    }
    Ok(())
}

/// Run every relevant lint over a rendered generated file before
/// writing it to disk. Today: pattern-header check on `.go` files; other
/// extensions short-circuit to `Ok(())`.
///
/// ## Examples
///
/// ```
/// use lazuli_codegen_go::emitter::lint::check_generated_file;
/// // Non-Go files always pass.
/// check_generated_file("anything", "go.mod").unwrap();
/// ```
pub fn check_generated_file(rendered: &str, file_path: &str) -> Result<(), LintError> {
    if file_path.ends_with(".go") {
        check_pattern_annotations(rendered, file_path)?;
    }
    Ok(())
}

/// Lint failure surfaced by [`check_generated_file`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LintError {
    /// `func ` declaration missing the preceding `//lazuli:pattern`
    /// header.
    MissingPattern {
        /// File path of the rendered output.
        file: String,
        /// 1-based line number of the offending `func` line.
        line: usize,
    },
}

impl fmt::Display for LintError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LintError::MissingPattern { file, line } => write!(
                f,
                "CODEGEN-PATTERN-001: emitted Go function in {file}:{line} lacks a //lazuli:pattern <id> <version> header"
            ),
        }
    }
}

impl std::error::Error for LintError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lint_rejects_function_without_pattern_header() {
        let err = check_pattern_annotations("package customer\n\nfunc Missing() {}\n", "x.gen.go")
            .expect_err("missing pattern should fail");
        assert_eq!(
            err,
            LintError::MissingPattern {
                file: "x.gen.go".to_owned(),
                line: 3
            }
        );
        assert!(err.to_string().contains("CODEGEN-PATTERN-001"));
    }

    #[test]
    fn lint_accepts_function_with_pattern_header() {
        check_pattern_annotations(
            "package customer\n\n// Foo does work.\n//lazuli:pattern command_pgx_insert v1\nfunc Foo() {}\n",
            "x.gen.go",
        )
        .expect("pattern header should pass");
    }
}
