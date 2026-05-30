/// Parse one `step <N>: <ref>` row.
fn parse_wizard_step(line: &SourceLine<'_>, value: &str) -> Result<WizardStepAst, ParseError> {
    let rest = value
        .strip_prefix("step ")
        .ok_or_else(|| line_error(line, "`wizard` rows are `step <N>: <ref>`"))?;
    let (index_raw, ref_raw) = rest
        .split_once(':')
        .ok_or_else(|| line_error(line, "`wizard` step must be `step <N>: <ref>`"))?;
    let index: u32 = index_raw
        .trim()
        .parse()
        .map_err(|_| line_error(line, "`wizard` step index must be a positive integer"))?;
    if index == 0 {
        return Err(line_error(
            line,
            "`wizard` step index must be a positive integer",
        ));
    }
    let ref_name = ref_raw.trim().to_owned();
    if !is_kebab_or_snake_ident(&ref_name) {
        return Err(line_error_owned(
            line,
            format!(
                "`wizard` step ref `{}` must be a kebab/snake identifier",
                ref_name
            ),
        ));
    }
    Ok(WizardStepAst {
        index,
        ref_name,
        span: Span::new(line.start, line.end),
    })
}
