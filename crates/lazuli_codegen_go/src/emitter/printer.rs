//! Tiny purpose-built Go printer. Mirrors `lazuli_openapi::YamlEmitter`
//! (`crates/lazuli_openapi/src/lib.rs:583`): tab-indented lines, no
//! string templating, deterministic output. We avoid `gofmt` shell-out
//! and any third-party template engine per proposal §2.2.
//!
//! Boundary discipline: this struct knows nothing about IR shapes. It
//! emits raw Go syntax with predictable indentation and column-aligned
//! struct-tag rows; the per-kind walkers in `module.rs` etc. drive it.

use std::fmt::Write;

/// Tab-indented line writer with column-aligned row emission helpers.
/// Goroutines never see this — it's a single-threaded string builder
/// per generated file.
pub struct GoPrinter {
    buf: String,
    indent_level: usize,
}

impl GoPrinter {
    /// Construct an empty printer at indent level 0.
    ///
    /// ## Examples
    ///
    /// ```
    /// use lazuli_codegen_go::emitter::printer::GoPrinter;
    /// let p = GoPrinter::new();
    /// assert_eq!(p.finish(), String::new());
    /// ```
    pub fn new() -> Self {
        Self {
            buf: String::with_capacity(4096),
            indent_level: 0,
        }
    }

    /// Consume the printer and return the produced source. Idempotent.
    ///
    /// ## Examples
    ///
    /// ```
    /// use lazuli_codegen_go::emitter::printer::GoPrinter;
    /// let mut p = GoPrinter::new();
    /// p.line("hello");
    /// assert_eq!(p.finish(), "hello\n");
    /// ```
    pub fn finish(self) -> String {
        self.buf
    }

    /// Emit a fully-formed line at the current indent level. The caller
    /// is responsible for the line's content; we add the leading tabs
    /// and the trailing `\n`.
    ///
    /// ## Examples
    ///
    /// ```
    /// use lazuli_codegen_go::emitter::printer::GoPrinter;
    /// let mut p = GoPrinter::new();
    /// p.indent();
    /// p.line("foo");
    /// assert_eq!(p.finish(), "\tfoo\n");
    /// ```
    pub fn line(&mut self, s: &str) {
        for _ in 0..self.indent_level {
            self.buf.push('\t');
        }
        self.buf.push_str(s);
        self.buf.push('\n');
    }

    /// Emit a fully-formed line without applying indentation. Used for
    /// Go directives that must begin at column 0.
    ///
    /// ## Examples
    ///
    /// ```
    /// use lazuli_codegen_go::emitter::printer::GoPrinter;
    /// let mut p = GoPrinter::new();
    /// p.indent();
    /// p.raw_line("//line foo.lzi:1:1");
    /// assert!(p.finish().starts_with("//line"));
    /// ```
    pub fn raw_line(&mut self, s: &str) {
        self.buf.push_str(s);
        self.buf.push('\n');
    }

    /// Emit a Go `//line` directive at the current cursor.
    /// Format: `//line <file>:<line>:<col>` followed by newline.
    /// Per Go spec, the directive must start at column 0.
    ///
    /// ## Examples
    ///
    /// ```
    /// use lazuli_codegen_go::emitter::printer::GoPrinter;
    /// let mut p = GoPrinter::new();
    /// p.line_directive("features/foo.lzi", 42, 1);
    /// assert_eq!(p.finish(), "//line features/foo.lzi:42:1\n");
    /// ```
    pub fn line_directive(&mut self, file: &str, line: u32, col: u32) {
        self.raw_line(&format!("//line {}:{}:{}", file, line, col));
    }

    /// Emit a blank line. Never carries indent prefix.
    ///
    /// ## Examples
    ///
    /// ```
    /// use lazuli_codegen_go::emitter::printer::GoPrinter;
    /// let mut p = GoPrinter::new();
    /// p.blank();
    /// assert_eq!(p.finish(), "\n");
    /// ```
    pub fn blank(&mut self) {
        self.buf.push('\n');
    }

    /// Increase the indent level by one tab. Has no immediate effect on
    /// the buffer; subsequent [`GoPrinter::line`] calls pick it up.
    ///
    /// ## Examples
    ///
    /// ```
    /// use lazuli_codegen_go::emitter::printer::GoPrinter;
    /// let mut p = GoPrinter::new();
    /// p.indent();
    /// p.line("x");
    /// assert_eq!(p.finish(), "\tx\n");
    /// ```
    pub fn indent(&mut self) {
        self.indent_level += 1;
    }

    /// Decrease the indent level by one tab. Saturates at zero — callers
    /// can over-dedent without panicking.
    ///
    /// ## Examples
    ///
    /// ```
    /// use lazuli_codegen_go::emitter::printer::GoPrinter;
    /// let mut p = GoPrinter::new();
    /// p.dedent();
    /// p.line("root");
    /// assert_eq!(p.finish(), "root\n");
    /// ```
    pub fn dedent(&mut self) {
        if self.indent_level > 0 {
            self.indent_level -= 1;
        }
    }

    /// Emit the canonical Code Connect-style banner so `gopls` and
    /// linters skip the file (proposal §11 invariant). Includes the
    /// source path the emitter walked, then the `package` directive
    /// and a blank line before the import block.
    ///
    /// ## Examples
    ///
    /// ```
    /// use lazuli_codegen_go::emitter::printer::GoPrinter;
    /// let mut p = GoPrinter::new();
    /// p.banner("features/foo.lzi", "foo");
    /// let out = p.finish();
    /// assert!(out.contains("DO NOT EDIT"));
    /// assert!(out.contains("package foo"));
    /// ```
    pub fn banner(&mut self, source_path: &str, package_name: &str) {
        self.line("// Code generated by lazuli; DO NOT EDIT.");
        self.line(&format!("// source: {}", source_path));
        self.blank();
        self.line(&format!("package {}", package_name));
        self.blank();
    }

    /// Emit a `package` directive followed by a blank line. Useful when
    /// the banner was emitted by the caller (e.g. `go.mod` doesn't need
    /// a `package` directive — only `banner` is invoked there).
    ///
    /// ## Examples
    ///
    /// ```
    /// use lazuli_codegen_go::emitter::printer::GoPrinter;
    /// let mut p = GoPrinter::new();
    /// p.package("foo");
    /// assert_eq!(p.finish(), "package foo\n\n");
    /// ```
    pub fn package(&mut self, name: &str) {
        self.line(&format!("package {}", name));
        self.blank();
    }

    /// Emit aligned two-column rows. Pads the first column to the
    /// width of the widest entry; the second column starts after a
    /// single space gap. Used for the `db:"…" json:"…"` struct-tag
    /// alignment proven in `runtime.rs:159-160`.
    ///
    /// ## Examples
    ///
    /// ```
    /// use lazuli_codegen_go::emitter::printer::GoPrinter;
    /// let mut p = GoPrinter::new();
    /// p.aligned_rows(&[("ID", "lazuli.ID"), ("CreatedAt", "lazuli.Time")]);
    /// assert!(p.finish().contains("ID        lazuli.ID"));
    /// ```
    pub fn aligned_rows(&mut self, rows: &[(&str, &str)]) {
        if rows.is_empty() {
            return;
        }
        let width = rows.iter().map(|(left, _)| left.len()).max().unwrap_or(0);
        for (left, right) in rows {
            // We hand-build the padded form into a scratch buffer to
            // keep `line` as the only writer that touches `self.buf`.
            let mut scratch = String::with_capacity(left.len() + width + right.len() + 2);
            let _ = write!(
                scratch,
                "{left:<width$} {right}",
                left = left,
                width = width,
                right = right
            );
            self.line(&scratch);
        }
    }

    /// Emit aligned three-column rows. Pads columns 1 and 2 to the
    /// width of the widest entry in each column; column 3 starts after
    /// a single space gap. Used for the
    /// `<Name> <GoType> \`db:"…" json:"…"\`` struct-row alignment
    /// proven in `runtime.rs:686-702`. Columns 1 and 2 carry the field
    /// declaration; column 3 is the back-tick wrapped tag block.
    ///
    /// ## Examples
    ///
    /// ```
    /// use lazuli_codegen_go::emitter::printer::GoPrinter;
    /// let mut p = GoPrinter::new();
    /// p.aligned_struct_rows(&[("ID", "lazuli.ID", "`db:\"id\"`")]);
    /// assert!(p.finish().contains("ID lazuli.ID `db:\"id\"`"));
    /// ```
    pub fn aligned_struct_rows(&mut self, rows: &[(&str, &str, &str)]) {
        if rows.is_empty() {
            return;
        }
        let name_width = rows.iter().map(|(n, _, _)| n.len()).max().unwrap_or(0);
        let type_width = rows.iter().map(|(_, t, _)| t.len()).max().unwrap_or(0);
        for (name, ty, tag) in rows {
            let mut scratch = String::with_capacity(name_width + type_width + tag.len() + 4);
            let _ = write!(
                scratch,
                "{name:<name_width$} {ty:<type_width$} {tag}",
                name = name,
                name_width = name_width,
                ty = ty,
                type_width = type_width,
                tag = tag,
            );
            self.line(&scratch);
        }
    }
}

impl Default for GoPrinter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_printer_finishes_with_empty_string() {
        let printer = GoPrinter::new();
        assert_eq!(printer.finish(), String::new());
    }

    #[test]
    fn line_then_indent_then_line_indents_second_only() {
        let mut printer = GoPrinter::new();
        printer.line("foo");
        printer.indent();
        printer.line("bar");
        printer.dedent();
        printer.line("baz");
        let out = printer.finish();
        // First and third lines are at depth 0 (no tab); middle line
        // gets exactly one tab.
        assert_eq!(out, "foo\n\tbar\nbaz\n");
    }

    #[test]
    fn blank_emits_newline_without_indent() {
        let mut printer = GoPrinter::new();
        printer.indent();
        printer.blank();
        printer.line("after blank");
        assert_eq!(printer.finish(), "\n\tafter blank\n");
    }

    #[test]
    fn raw_line_bypasses_indent() {
        let mut printer = GoPrinter::new();
        printer.indent();
        printer.raw_line("//line features/foo.lzi:42:1");
        printer.line("inside");
        assert_eq!(printer.finish(), "//line features/foo.lzi:42:1\n\tinside\n");
    }

    #[test]
    fn line_directive_uses_go_source_map_format() {
        let mut printer = GoPrinter::new();
        printer.indent();
        printer.line_directive("features/foo.lzi", 42, 1);
        assert_eq!(printer.finish(), "//line features/foo.lzi:42:1\n");
    }

    #[test]
    fn banner_includes_source_and_package() {
        let mut printer = GoPrinter::new();
        printer.banner("examples/full-capsule/full-capsule.lzi", "customer");
        let out = printer.finish();
        assert!(out.contains("// Code generated by lazuli; DO NOT EDIT."));
        assert!(out.contains("// source: examples/full-capsule/full-capsule.lzi"));
        assert!(out.contains("\npackage customer\n"));
    }

    #[test]
    fn aligned_rows_pads_first_column_to_widest() {
        let mut printer = GoPrinter::new();
        printer.aligned_rows(&[("ID", "lazuli.ID"), ("CreatedAt", "lazuli.Time")]);
        // CreatedAt is 9 chars, so ID (2 chars) gets 7 trailing spaces.
        assert_eq!(
            printer.finish(),
            "ID        lazuli.ID\nCreatedAt lazuli.Time\n"
        );
    }

    #[test]
    fn aligned_rows_empty_input_is_noop() {
        let mut printer = GoPrinter::new();
        printer.aligned_rows(&[]);
        assert_eq!(printer.finish(), String::new());
    }

    #[test]
    fn aligned_struct_rows_pads_three_columns() {
        let mut printer = GoPrinter::new();
        printer.indent();
        printer.aligned_struct_rows(&[
            ("ID", "lazuli.ID", "`db:\"id\"         json:\"id\"`"),
            (
                "CreatedAt",
                "lazuli.Time",
                "`db:\"created_at\" json:\"created_at\"`",
            ),
        ]);
        let out = printer.finish();
        // CreatedAt = 9 chars name; widest type = "lazuli.Time" = 11 chars.
        // ID pads to 9 chars; lazuli.ID pads to 11 chars.
        assert_eq!(
            out,
            "\tID        lazuli.ID   `db:\"id\"         json:\"id\"`\n\tCreatedAt lazuli.Time `db:\"created_at\" json:\"created_at\"`\n"
        );
    }

    #[test]
    fn aligned_struct_rows_empty_input_is_noop() {
        let mut printer = GoPrinter::new();
        printer.aligned_struct_rows(&[]);
        assert_eq!(printer.finish(), String::new());
    }

    #[test]
    fn dedent_below_zero_is_floor() {
        // Defensive: caller can over-dedent without panicking.
        let mut printer = GoPrinter::new();
        printer.dedent();
        printer.dedent();
        printer.line("at root");
        assert_eq!(printer.finish(), "at root\n");
    }
}
