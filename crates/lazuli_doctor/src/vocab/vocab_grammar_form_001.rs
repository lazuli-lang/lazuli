//! VOCAB-GRAMMAR-FORM-001 — deprecated `.lzi` grammar forms.
//!
//! ## Rule statement
//!
//! Fires on compatibility forms that still parse during migration windows but
//! are no longer canonical authoring vocabulary: `validates resource
//! @validator.X`, `validates field <name> @validator.X`, inline `previously
//! migrated|alias <old>` on a resource or field header, and legacy `validate
//! "./path.go"`. The rule reports the authored form and the canonical
//! replacement so automated migrations and CI can converge source onto the
//! indentation-first grammar.
//!
//! ## Severity profile
//!
//! Severity: `warning` in strict/profiled authoring runs, `error` in production
//! profile. Prototype callers may suppress the rule during a migration window,
//! but production treats deprecated grammar as non-shippable because it keeps
//! hidden compatibility paths alive.
//!
//! ## Fixture example
//!
//! ```lzi
//! feature account
//!   domain
//!     resource Account previously alias Customer
//!       email: Text required previously migrated email_address
//!       validates resource @validator.account
//!       validates field email @validator.email
//!       validate "./account.go"
//! ```
//!
//! Canonical fix:
//!
//! ```lzi
//! feature account
//!   domain
//!     resource Account
//!       previously alias Customer
//!       email: Text required
//!         previously migrated email_address
//!       validates @validator.account
//!       validates @validator.email
//!       validates field <name> "./account.go"
//! ```
//!
//! ## Proposal anchor
//!
//! Historical proposal: `docs/proposals/doctor-vocabulary-lints.md`
//! §VOCAB-GRAMMAR-FORM-001 (extracted to `lazuli-ops` in commit `acbc3c14`).
//! Runtime hint points at
//! `migrations/recipes/v0.X-to-v0.Y/VOCAB-GRAMMAR-FORM-001.md` when present.
//!
//! Diagnostic ID / code constant: `VOCAB-GRAMMAR-FORM-001`;
//! `Finding::CODE` is `pub const CODE: &'static str =
//! "VOCAB-GRAMMAR-FORM-001";`.

use std::path::{Path, PathBuf};

use lazuli_syntax::{FeatureSkeleton, ResourceDecl, parse_feature_skeletons};

const RECIPE: &str = "migrations/recipes/v0.X-to-v0.Y/VOCAB-GRAMMAR-FORM-001.md";

// -- output -------------------------------------------------------------------

/// One VOCAB-GRAMMAR-FORM-001 finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file.
    pub path: PathBuf,
    /// 1-indexed source line.
    pub line: usize,
    /// 1-indexed source column.
    pub column: usize,
    /// Deprecated authored form.
    pub old: String,
    /// Canonical replacement.
    pub new: String,
}

impl Finding {
    pub const CODE: &'static str = "VOCAB-GRAMMAR-FORM-001";

    pub fn message(&self) -> String {
        format!(
            "deprecated form '{}'; use '{}'. Hint: see {} if present.",
            self.old, self.new, RECIPE
        )
    }
}

// -- detection ----------------------------------------------------------------

/// Run VOCAB-GRAMMAR-FORM-001 over one `.lzi` source.
///
/// The primary path uses the canonical-indent parser so the rule walks feature
/// and resource blocks, not package-wide regexes. Inline `previously` provenance
/// is not preserved by the current AST; for that form we inspect the parsed
/// resource and field header lines inside their spans.
pub fn check(source: &str, path: &Path) -> Vec<Finding> {
    let mut findings = Vec::new();

    if let Ok(features) = parse_feature_skeletons(source) {
        for feature in &features {
            findings.extend(check_feature(source, feature, path));
        }
    }

    findings.extend(check_legacy_scoped_validates(source, path));
    findings.extend(check_inline_previously_headers(source, path));
    findings.extend(check_legacy_validate_path(source, path));
    findings.sort_by_key(|finding| (finding.line, finding.column, finding.old.clone()));
    findings.dedup_by(|left, right| {
        left.line == right.line && left.column == right.column && left.old == right.old
    });
    findings
}

fn check_feature(source: &str, feature: &FeatureSkeleton, path: &Path) -> Vec<Finding> {
    feature
        .resources
        .iter()
        .flat_map(|resource| check_resource(source, resource, path))
        .collect()
}

fn check_resource(source: &str, resource: &ResourceDecl, path: &Path) -> Vec<Finding> {
    let mut findings = Vec::new();

    for validates in &resource.validates {
        let Some((line, column)) = find_line_in_span(
            source,
            resource.span.start,
            resource.span.end,
            "validates",
            validates,
        ) else {
            continue;
        };

        if let Some(rest) = validates.strip_prefix("resource ") {
            if rest.trim_start().starts_with("@validator.") {
                findings.push(Finding {
                    path: path.to_path_buf(),
                    line,
                    column,
                    old: format!("validates resource {}", rest.trim()),
                    new: format!("validates {}", rest.trim()),
                });
            }
        } else if let Some(rest) = validates.strip_prefix("field ") {
            let mut parts = rest.split_whitespace();
            let Some(field_name) = parts.next() else {
                continue;
            };
            let Some(target) = parts.next() else {
                continue;
            };
            if target.starts_with("@validator.") {
                findings.push(Finding {
                    path: path.to_path_buf(),
                    line,
                    column,
                    old: format!("validates field {field_name} {target}"),
                    new: format!("validates {target}"),
                });
            }
        }
    }

    if let Some(header) = line_at_offset(source, resource.span.start) {
        if let Some((old, new, column)) = inline_previously_header_form(header.text) {
            findings.push(Finding {
                path: path.to_path_buf(),
                line: header.line,
                column,
                old,
                new,
            });
        }
    }

    for field in &resource.fields {
        if let Some(header) = line_at_offset(source, field.span.start) {
            if let Some((old, new, column)) = inline_previously_header_form(header.text) {
                findings.push(Finding {
                    path: path.to_path_buf(),
                    line: header.line,
                    column,
                    old,
                    new,
                });
            }
        }
    }

    findings
}

fn check_legacy_validate_path(source: &str, path: &Path) -> Vec<Finding> {
    source
        .lines()
        .enumerate()
        .filter_map(|(idx, raw_line)| {
            let trimmed = strip_comment(raw_line).trim_start().trim_end();
            let rest = trimmed.strip_prefix("validate ")?;
            if !rest.trim_start().starts_with('"') {
                return None;
            }
            Some(Finding {
                path: path.to_path_buf(),
                line: idx + 1,
                column: raw_line.find("validate").map(|c| c + 1).unwrap_or(1),
                old: format!("validate {}", rest.trim()),
                new: format!("validates field <name> {}", rest.trim()),
            })
        })
        .collect()
}

fn check_legacy_scoped_validates(source: &str, path: &Path) -> Vec<Finding> {
    source
        .lines()
        .enumerate()
        .filter_map(|(idx, raw_line)| {
            let trimmed = strip_comment(raw_line).trim_start().trim_end();
            let rest = trimmed.strip_prefix("validates ")?;

            if let Some(target) = rest.strip_prefix("resource ") {
                let target = target.trim();
                if target.starts_with("@validator.") {
                    return Some(Finding {
                        path: path.to_path_buf(),
                        line: idx + 1,
                        column: raw_line.find("validates").map(|c| c + 1).unwrap_or(1),
                        old: format!("validates resource {target}"),
                        new: format!("validates {target}"),
                    });
                }
            }

            let field_rest = rest.strip_prefix("field ")?;
            let mut parts = field_rest.split_whitespace();
            let field_name = parts.next()?;
            let target = parts.next()?;
            if !target.starts_with("@validator.") {
                return None;
            }

            Some(Finding {
                path: path.to_path_buf(),
                line: idx + 1,
                column: raw_line.find("validates").map(|c| c + 1).unwrap_or(1),
                old: format!("validates field {field_name} {target}"),
                new: format!("validates {target}"),
            })
        })
        .collect()
}

fn check_inline_previously_headers(source: &str, path: &Path) -> Vec<Finding> {
    source
        .lines()
        .enumerate()
        .filter_map(|(idx, raw_line)| {
            let (old, new, column) = inline_previously_header_form(raw_line)?;
            Some(Finding {
                path: path.to_path_buf(),
                line: idx + 1,
                column,
                old,
                new,
            })
        })
        .collect()
}

// -- internals ----------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
struct SourceLine<'a> {
    line: usize,
    text: &'a str,
}

fn line_at_offset(source: &str, offset: usize) -> Option<SourceLine<'_>> {
    let mut start = 0usize;
    for (idx, line) in source.lines().enumerate() {
        let end = start + line.len();
        if offset >= start && offset <= end {
            return Some(SourceLine {
                line: idx + 1,
                text: line,
            });
        }
        start = end + 1;
    }
    None
}

fn find_line_in_span(
    source: &str,
    start: usize,
    end: usize,
    keyword: &str,
    tail: &str,
) -> Option<(usize, usize)> {
    let target = format!("{keyword} {tail}");
    let mut offset = 0usize;
    for (idx, raw_line) in source.lines().enumerate() {
        let line_end = offset + raw_line.len();
        if line_end < start {
            offset = line_end + 1;
            continue;
        }
        if offset > end {
            break;
        }

        let without_comment = strip_comment(raw_line);
        if without_comment.trim() == target {
            let column = raw_line.find(keyword).map(|col| col + 1).unwrap_or(1);
            return Some((idx + 1, column));
        }
        offset = line_end + 1;
    }
    None
}

fn inline_previously_header_form(line: &str) -> Option<(String, String, usize)> {
    let stripped = strip_comment(line);
    let trimmed = stripped.trim();
    let marker = if let Some(index) = trimmed.find(" previously migrated ") {
        ("previously migrated", index)
    } else if let Some(index) = trimmed.find(" previously alias ") {
        ("previously alias", index)
    } else {
        return None;
    };

    let old_name = trimmed[marker.1 + marker.0.len() + 2..]
        .split_whitespace()
        .next()?;
    let header = trimmed[..marker.1].trim_end();
    let indent = line.find(header).unwrap_or(0);
    let child_indent = " ".repeat(indent + 2);

    Some((
        format!("{header} {} {old_name}", marker.0),
        format!("{header}\n{child_indent}{} {old_name}", marker.0),
        line.find(marker.0).map(|col| col + 1).unwrap_or(1),
    ))
}

fn strip_comment(line: &str) -> &str {
    let mut in_string = false;
    let mut escaped = false;

    for (idx, ch) in line.char_indices() {
        if ch == '#' && !in_string {
            return &line[..idx];
        }

        if ch == '"' && !escaped {
            in_string = !in_string;
        }

        escaped = ch == '\\' && !escaped;
        if ch != '\\' {
            escaped = false;
        }
    }

    line
}

// -- tests --------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn check_src(source: &str) -> Vec<Finding> {
        check(source, Path::new("features/test/test.lzi"))
    }

    fn feature(body: &str) -> String {
        format!("feature test\n  domain\n{body}")
    }

    #[test]
    fn positive_validates_resource_validator_fires() {
        let findings = check_src(&feature(
            "    resource Account\n      id: ID required\n      validates resource @validator.account\n",
        ));

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].old, "validates resource @validator.account");
        assert_eq!(findings[0].new, "validates @validator.account");
        assert_eq!(Finding::CODE, "VOCAB-GRAMMAR-FORM-001");
    }

    #[test]
    fn negative_canonical_validates_validator_does_not_fire() {
        assert!(
            check_src(&feature(
                "    resource Account\n      id: ID required\n      validates @validator.account\n",
            ))
            .is_empty()
        );
    }

    #[test]
    fn positive_validates_field_validator_fires() {
        let findings = check_src(&feature(
            "    resource Account\n      email: Text required\n      validates field email @validator.email\n",
        ));

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].old, "validates field email @validator.email");
        assert_eq!(findings[0].new, "validates @validator.email");
    }

    #[test]
    fn negative_resource_inline_validator_path_does_not_count_as_scoped_validator() {
        assert!(check_src(&feature(
            "    resource Account\n      id: ID required\n      validates resource \"./account.go\"\n",
        ))
        .is_empty());
    }

    #[test]
    fn positive_inline_previously_on_resource_header_fires() {
        let findings = check_src(&feature(
            "    resource Account previously migrated Customer\n      id: ID required\n",
        ));

        assert_eq!(findings.len(), 1);
        assert_eq!(
            findings[0].old,
            "resource Account previously migrated Customer"
        );
        assert!(
            findings[0]
                .new
                .contains("\n      previously migrated Customer")
        );
    }

    #[test]
    fn negative_child_previously_does_not_fire() {
        assert!(
            check_src(&feature(
                "    resource Account\n      previously migrated Customer\n      id: ID required\n",
            ))
            .is_empty()
        );
    }

    #[test]
    fn positive_validate_path_fires() {
        let findings = check_src(&feature(
            "    resource Account\n      id: ID required\n      validate \"./account.go\"\n",
        ));

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].old, "validate \"./account.go\"");
        assert_eq!(findings[0].new, "validates field <name> \"./account.go\"");
    }

    #[test]
    fn negative_command_validate_validator_does_not_fire() {
        assert!(
            check_src("feature test\n  command create\n    validate @validator.account\n")
                .is_empty()
        );
    }

    #[test]
    fn golden_combines_all_four_forms() {
        let findings = check_src(&feature(
            "    resource Account previously alias Customer\n      id: ID required\n      email: Text required previously migrated email_address\n      validates resource @validator.account\n      validates field email @validator.email\n      validate \"./account.go\"\n",
        ));

        assert_eq!(findings.len(), 5);
        assert!(
            findings
                .iter()
                .any(|f| f.old == "validates resource @validator.account")
        );
        assert!(
            findings
                .iter()
                .any(|f| f.old == "validates field email @validator.email")
        );
        assert!(
            findings
                .iter()
                .any(|f| f.old == "resource Account previously alias Customer")
        );
        assert!(
            findings
                .iter()
                .any(|f| f.old == "email: Text required previously migrated email_address")
        );
        assert!(
            findings
                .iter()
                .any(|f| f.old == "validate \"./account.go\"")
        );
    }
}
