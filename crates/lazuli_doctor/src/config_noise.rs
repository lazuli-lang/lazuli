//! Frente 1 — `CONFIG-NOISE-001` heuristic.
//!
//! Counts the comment-vs-semantic ratio of a TOML-shaped config file.
//! The rule motivation: when a user's `Lazurite.toml` has more comment
//! lines than semantic lines, the framework is leaking intent into the
//! user's file — knowledge that should live in framework defaults or
//! canonical docs is instead bloating every pilot's config.
//!
//! Severity is informational (warning) by design; the rule never
//! gates. See `docs/canonical-semantics.md#config-hygiene`.

/// Comment-vs-semantic metrics for a single config file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConfigNoiseMetrics {
    /// Lines that are entirely `#`-prefixed (after trim), plus the
    /// count of lines carrying a trailing inline `# ...` comment.
    pub comment_lines: usize,
    /// Non-comment, non-blank lines.
    pub semantic_lines: usize,
}

impl ConfigNoiseMetrics {
    /// Rule fires strictly: `comment_lines > semantic_lines`. The
    /// boundary case (equal counts → 1:1 ratio) does NOT fire — the
    /// rule signals dominance, not parity.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use lazuli_doctor::config_noise::ConfigNoiseMetrics;
    ///
    /// let dominant = ConfigNoiseMetrics { comment_lines: 5, semantic_lines: 3 };
    /// assert!(dominant.fires());
    ///
    /// // Equal counts (1:1) — does NOT fire.
    /// let parity = ConfigNoiseMetrics { comment_lines: 3, semantic_lines: 3 };
    /// assert!(!parity.fires());
    /// ```
    pub fn fires(&self) -> bool {
        self.comment_lines > self.semantic_lines
    }

    /// Comment / semantic ratio. When `semantic_lines == 0` returns
    /// `comment_lines as f64` (sentinel — caller renders it as
    /// "all-comment file"); avoids div-by-zero panics.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use lazuli_doctor::config_noise::ConfigNoiseMetrics;
    ///
    /// let m = ConfigNoiseMetrics { comment_lines: 6, semantic_lines: 3 };
    /// assert_eq!(m.ratio(), 2.0);
    ///
    /// // Sentinel ratio when there is no semantic content.
    /// let only_comments = ConfigNoiseMetrics { comment_lines: 4, semantic_lines: 0 };
    /// assert_eq!(only_comments.ratio(), 4.0);
    /// ```
    pub fn ratio(&self) -> f64 {
        if self.semantic_lines == 0 {
            self.comment_lines as f64
        } else {
            self.comment_lines as f64 / self.semantic_lines as f64
        }
    }
}

/// Compute the comment-vs-semantic ratio for a TOML-shaped string.
///
/// Counting rules:
/// - Blank lines: ignored.
/// - Lines whose first non-whitespace char is `#`: count as one
///   `comment_line`, contribute 0 `semantic_lines`.
/// - Other lines: count as one `semantic_line`. Additionally, if the
///   line has a `#` character that is NOT at position 0 of the trimmed
///   string (i.e. follows a value), it counts as one additional
///   `comment_line` (trailing inline comment).
///
/// The trailing-`#` detection is intentionally naive: it does not
/// account for `#` inside quoted strings, which is rare enough in TOML
/// configs to be acceptable noise for a heuristic rule.
///
/// ## Examples
///
/// ```rust
/// use lazuli_doctor::config_noise::config_noise_metrics;
///
/// let body = "# header\nkey = 1\nkey2 = 2  # trailing\n";
/// let m = config_noise_metrics(body);
/// assert_eq!(m.comment_lines, 2); // 1 full-line + 1 trailing
/// assert_eq!(m.semantic_lines, 2);
/// assert!(!m.fires());
/// ```
pub fn config_noise_metrics(contents: &str) -> ConfigNoiseMetrics {
    let mut comment_lines = 0usize;
    let mut semantic_lines = 0usize;
    for raw in contents.lines() {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with('#') {
            comment_lines += 1;
            continue;
        }
        semantic_lines += 1;
        if let Some(hash_idx) = trimmed.find('#') {
            if hash_idx > 0 {
                comment_lines += 1;
            }
        }
    }
    ConfigNoiseMetrics {
        comment_lines,
        semantic_lines,
    }
}

#[cfg(test)]
mod tests {
    use super::config_noise_metrics;

    /// Clean file (no comments) — never fires.
    #[test]
    fn clean_file_does_not_fire() {
        let body = r#"
[project]
name = "myapp"
module = "github.com/me/myapp"

[lazuli]
runtime = "0.1.0"
"#;
        let m = config_noise_metrics(body);
        assert_eq!(m.comment_lines, 0);
        assert!(m.semantic_lines > 0);
        assert!(!m.fires());
    }

    /// Heavy commented file — fires.
    #[test]
    fn heavy_commented_file_fires() {
        let body = r#"
# top-level project metadata follows
# the name field is the human-readable label
# the module field is the Go module path
# this comment block dominates the file
[project]
name = "myapp"
module = "github.com/me/myapp"
"#;
        let m = config_noise_metrics(body);
        // 4 comment lines, 3 semantic lines.
        assert_eq!(m.comment_lines, 4);
        assert_eq!(m.semantic_lines, 3);
        assert!(m.fires());
    }

    /// All-comment file (no semantic content) — fires loudly.
    #[test]
    fn all_comment_file_fires() {
        let body = "# only commentary\n# more commentary\n# even more\n";
        let m = config_noise_metrics(body);
        assert_eq!(m.comment_lines, 3);
        assert_eq!(m.semantic_lines, 0);
        assert!(m.fires());
        // Sentinel ratio when semantic_lines == 0.
        assert_eq!(m.ratio(), 3.0);
    }

    /// Boundary: equal counts (1:1) — does NOT fire.
    #[test]
    fn boundary_one_to_one_does_not_fire() {
        let body = "# one comment\nkey = 1\n# two\nkey2 = 2\n";
        let m = config_noise_metrics(body);
        assert_eq!(m.comment_lines, 2);
        assert_eq!(m.semantic_lines, 2);
        assert!(!m.fires());
    }

    /// Inline trailing comment counts toward `comment_lines` AND
    /// keeps its `semantic_lines` count.
    #[test]
    fn inline_trailing_comment_counts_both_ways() {
        let body = "key = 1  # trailing note\nkey2 = 2\nkey3 = 3\n";
        let m = config_noise_metrics(body);
        assert_eq!(m.comment_lines, 1);
        assert_eq!(m.semantic_lines, 3);
        assert!(!m.fires());
    }

    /// Realistic shape: the slimmed Lazurite.toml we ship in
    /// `lazurite/templates/default/Lazurite.toml.tmpl` (~50 lines)
    /// must NOT fire.
    #[test]
    fn slim_canonical_template_does_not_fire() {
        // Mirrors the post-Frente-1 template body.
        let body = r#"
[project]
name = "myapp"
module = "github.com/me/myapp"
schema = 1

[lazuli]
runtime = "0.1.0"

[doctor]
profile = "strict"
coverage = "tdd-strict"
"#;
        let m = config_noise_metrics(body);
        assert_eq!(m.comment_lines, 0);
        assert!(m.semantic_lines >= 7);
        assert!(!m.fires());
    }
}
