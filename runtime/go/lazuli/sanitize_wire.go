package lazuli

// sanitizeColumnValues walks the resolved binding columns and rewrites
// each value whose column is declared `validate sanitize_html(<profile>)`
// on the resource with its sanitized form. Mirrors `encryptColumnValues`
// exactly: `cols` and `values` are parallel slices (index `i` of `cols`
// names the column whose resolved binding sits at `values[i]`), and the
// function mutates `values` in place so SQL placeholder ordering stays
// stable. It never returns an error — bluemonday's `Sanitize` cannot
// fail — but keeps the `error` return so the call sites read identically
// to the encryption path and a future fail-open/closed knob has a home.
//
// Wire-thin: zero HTML knowledge here. The markup work happens in
// `SanitizeHTML` (one call into `bluemonday.Policy.Sanitize`); this
// function is the SQL-side glue that threads each column's profile from
// `Resource.SanitizeColumns` (populated by codegen) onto the bound value.
//
// Applied at the WRITE boundary BEFORE the driver sees the value, so the
// stored column never holds raw attacker HTML — this is the patch that
// turns the previously-no-op `sanitize_html` constraint into a real
// stored-XSS guard.
func sanitizeColumnValues(res *resourceErased, cols []string, values []any) error {
	if res == nil || len(res.SanitizeColumns) == 0 {
		return nil
	}
	for i, col := range cols {
		profile, ok := res.SanitizeColumns[unquoteIdent(col)]
		if !ok {
			continue
		}
		switch v := values[i].(type) {
		case string:
			values[i] = SanitizeHTML(SanitizeHTMLProfile(profile), v)
		case *string:
			// Optional field bound as a pointer: a nil pointer means
			// the field was omitted (partial update) — leave it so the
			// SET clause is skipped / NULL is written. A non-nil pointer
			// is sanitized in place behind a fresh pointer so the
			// caller's input struct is not mutated.
			if v == nil {
				continue
			}
			s := SanitizeHTML(SanitizeHTMLProfile(profile), *v)
			values[i] = &s
		default:
			// Column metadata claimed sanitization but the binding holds
			// a non-string value. That is a generator bug (sanitize_html
			// only lowers onto text fields); skip rather than panic so a
			// mismatch degrades to "unchanged" instead of crashing the
			// request. The codegen test asserts the mapping only emits
			// for string-typed fields.
			continue
		}
	}
	return nil
}
