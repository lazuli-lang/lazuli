/// GAP-R2 — emit the record's `Validate() error` method. Delegates to the
/// runtime's reflection-thin `lazuli.ValidateValue`, which walks the struct
/// fields and invokes any `interface{ Validate() error }` it finds —
/// including the W1 semantic-scalar carriers and elements of `Many<Record>`
/// slices. Founding principle: the generated method is one wire call, the
/// traversal lives in the runtime (`runtime/go/lazuli/nested_validate.go`).
fn emit_record_validate(p: &mut GoPrinter, pascal: &str) {
    p.line(&format!(
        "// Validate runs nested field validation for {pascal} — each field that"
    ));
    p.line("// carries its own `Validate() error` (semantic carriers, nested records,");
    p.line("// and elements of `Many<Record>` slices) is checked. Wire-thin: the");
    p.line("// traversal lives in `lazuli.ValidateValue`.");
    emit_pattern_header(p, PATTERN_RECORD_VALIDATE);
    p.line(&format!("func (m {pascal}) Validate() error {{"));
    p.indent();
    p.line("return lazuli.ValidateValue(m)");
    p.dedent();
    p.line("}");
}

// ----------------------------------------------------------------------------
// Helpers
// ----------------------------------------------------------------------------

/// Decoded form of one struct row. Comments interleave between rows;
/// rendering uses `aligned_struct_rows` for the data rows so the
/// `db`/`json` tag columns line up.
struct TaggedField {
    name: String,
    go_type: String,
    db_col: String,
    json_suffix: String,
    /// B3 — when set, append `validate:"<value>"` to the tag. Used for
    /// plugin-contributed semantic types: the value is
    /// `<plugin.name>.<validator>` (e.g. `scalars-br.ValidateCPF`).
    /// The runtime dispatcher reads this tag to invoke the plugin
    /// adapter's exported validator function.
    /// See `docs/proposals/semantic-types-plugin-locales.md` §Codegen.
    validate: Option<String>,
    /// When set, render as a standalone comment line — used for
    /// `derived from` columns that live in DDL only.
    comment: Option<String>,
}

/// Emit the struct body. Every data row pulls its column widths from
/// a single global pass over the typed fields so the `name`, `type`,
/// and `tag` columns stay aligned even when a stand-alone comment row
/// interrupts the visual block (gofmt resets alignment across comments
/// — we keep it consistent to mirror the runtime-spike fixture).
fn write_struct_rows(p: &mut GoPrinter, tagged: &[TaggedField]) {
    // Two-pass: build rendered rows first so we can compute global
    // name/type/tag widths, then emit with `aligned_struct_rows` for
    // contiguous data slices (comments interrupt the slice, but each
    // slice is rendered with the *global* widths so columns line up
    // across the comment line).
    let max_db_width = tagged
        .iter()
        .filter(|f| f.comment.is_none())
        .map(|f| db_segment(&f.db_col).len())
        .max()
        .unwrap_or(0);

    enum Row {
        Data {
            name: String,
            ty: String,
            tag: String,
        },
        Comment(String),
    }

    let rows: Vec<Row> = tagged
        .iter()
        .map(|field| match &field.comment {
            Some(comment) => Row::Comment(comment.clone()),
            None => Row::Data {
                name: field.name.clone(),
                ty: field.go_type.clone(),
                tag: build_tag(
                    &field.db_col,
                    &field.json_suffix,
                    field.validate.as_deref(),
                    max_db_width,
                ),
            },
        })
        .collect();

    let name_width = rows
        .iter()
        .filter_map(|r| match r {
            Row::Data { name, .. } => Some(name.len()),
            _ => None,
        })
        .max()
        .unwrap_or(0);
    let ty_width = rows
        .iter()
        .filter_map(|r| match r {
            Row::Data { ty, .. } => Some(ty.len()),
            _ => None,
        })
        .max()
        .unwrap_or(0);

    for row in &rows {
        match row {
            Row::Comment(text) => p.line(text),
            Row::Data { name, ty, tag } => {
                let mut scratch = String::with_capacity(name_width + ty_width + tag.len() + 4);
                let _ = write!(
                    scratch,
                    "{name:<name_width$} {ty:<ty_width$} {tag}",
                    name = name,
                    name_width = name_width,
                    ty = ty,
                    ty_width = ty_width,
                    tag = tag,
                );
                p.line(&scratch);
            }
        }
    }
}

/// Format the `db:"…"` token segment (no surrounding back-ticks). Used
/// to compute the alignment width for the json column.
fn db_segment(db_col: &str) -> String {
    format!("db:\"{}\"", db_col)
}

/// Build a column-aligned tag string padding the `db:"…"` portion so
/// the `json:` token aligns across the struct. Mirrors the pattern
/// proven in `runtime.rs:664-682`. When `validate` is set, a
/// `validate:"<value>"` clause follows the `json:` clause; this is
/// the B3 plugin-semantic dispatch tag.
fn build_tag(
    db_col: &str,
    json_suffix: &str,
    validate: Option<&str>,
    max_db_width: usize,
) -> String {
    let db_part = db_segment(db_col);
    let pad = max_db_width.saturating_sub(db_part.len());
    match validate {
        Some(v) => format!(
            "`{}{} json:\"{}\" validate:\"{}\"`",
            db_part,
            " ".repeat(pad),
            json_suffix,
            v
        ),
        None => format!("`{}{} json:\"{}\"`", db_part, " ".repeat(pad), json_suffix),
    }
}
