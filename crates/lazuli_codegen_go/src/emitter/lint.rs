//! Emitter lint — enforces invariants on rendered generated Go before write.
//! Per docs/proposals/bucket-ai-debug-loop-cycle.md §6.3.

use std::fmt;

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

pub fn check_generated_file(rendered: &str, file_path: &str) -> Result<(), LintError> {
    if file_path.ends_with(".go") {
        check_pattern_annotations(rendered, file_path)?;
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LintError {
    MissingPattern { file: String, line: usize },
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
