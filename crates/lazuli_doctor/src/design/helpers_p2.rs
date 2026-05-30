/// Returns every `style={{ ... }}` segment in `content`, line-anchored.
///
/// Brace depth is tracked across lines so that multi-line style props
/// (the common case for readable React code) are scanned in full.
/// Quoted strings inside the style block do NOT terminate the block —
/// the matcher only counts `{` / `}` outside `"..."` and `'...'` regions.
///
/// ## Examples
///
/// ```rust
/// use lazuli_doctor::design::helpers::iter_style_spans;
///
/// let body = r##"<div style={{ color: "#7c3aed" }} />"##;
/// let lines: Vec<&str> = body.lines().collect();
/// let spans = iter_style_spans(body, &lines);
/// assert_eq!(spans.len(), 1);
/// assert!(spans[0].segment.contains("color"));
/// ```
pub fn iter_style_spans<'a>(content: &'a str, lines: &[&'a str]) -> Vec<StyleSpan<'a>> {
    let mut out: Vec<StyleSpan<'a>> = Vec::new();
    let _ = content; // explicit: we operate on `lines`, content is contextual

    let mut state = ScanState::Outside;
    let mut current_start_in_line: usize = 0;
    let mut current_line: usize = 0;

    for (line_idx, line) in lines.iter().enumerate() {
        let bytes = line.as_bytes();
        let mut i = 0;
        if matches!(state, ScanState::Inside { .. }) {
            current_start_in_line = 0;
            current_line = line_idx;
        }
        while i < bytes.len() {
            match state {
                ScanState::Outside => {
                    if let Some(rest) = line.get(i..)
                        && let Some(rel) = rest.find("style=")
                    {
                        let pos = i + rel;
                        // require preceding char to be non-identifier (avoid `nostyle=` etc).
                        let ok = match pos.checked_sub(1).and_then(|p| bytes.get(p).copied()) {
                            None => true,
                            Some(b) => !is_ident_byte(b),
                        };
                        if !ok {
                            i = pos + "style=".len();
                            continue;
                        }
                        let after = pos + "style=".len();
                        // Look for `{{`. We tolerate a single `{` followed
                        // immediately by `{`; whitespace between is not
                        // accepted (matches conventional JSX style props).
                        if line.as_bytes().get(after).copied() == Some(b'{')
                            && line.as_bytes().get(after + 1).copied() == Some(b'{')
                        {
                            state = ScanState::Inside {
                                depth: 2,
                                in_string: None,
                            };
                            current_start_in_line = after + 2;
                            current_line = line_idx;
                            i = after + 2;
                        } else {
                            // not a style={{ block — skip past `style=` and continue.
                            i = after;
                        }
                    } else {
                        break;
                    }
                }
                ScanState::Inside {
                    ref mut depth,
                    ref mut in_string,
                } => {
                    let c = bytes[i];
                    match in_string {
                        Some(q) => {
                            if c == b'\\' && i + 1 < bytes.len() {
                                i += 2;
                                continue;
                            }
                            if c == *q {
                                *in_string = None;
                            }
                            i += 1;
                        }
                        None => {
                            if c == b'"' || c == b'\'' || c == b'`' {
                                *in_string = Some(c);
                                i += 1;
                            } else if c == b'{' {
                                *depth += 1;
                                i += 1;
                            } else if c == b'}' {
                                *depth -= 1;
                                if *depth == 0 {
                                    // End of style block (consumed the second `}` at `i`;
                                    // the matching first `}` of the pair is at `i-1`).
                                    // The segment excludes both closing braces — slice
                                    // ends at `i - 1`.
                                    let seg_end = i.saturating_sub(1);
                                    let seg_start = if current_line == line_idx {
                                        current_start_in_line
                                    } else {
                                        0
                                    };
                                    let seg = &line[seg_start..seg_end.max(seg_start)];
                                    out.push(StyleSpan {
                                        line_idx_0based: line_idx,
                                        segment: seg,
                                    });
                                    state = ScanState::Outside;
                                    i += 1;
                                } else {
                                    i += 1;
                                }
                            } else {
                                i += 1;
                            }
                        }
                    }
                }
            }
        }
        // End-of-line: if still inside, emit the rest of the line as one
        // segment so per-line rules see it.
        if let ScanState::Inside { .. } = state {
            let seg = if current_line == line_idx {
                &line[current_start_in_line..]
            } else {
                &line[..]
            };
            if !seg.is_empty() {
                out.push(StyleSpan {
                    line_idx_0based: line_idx,
                    segment: seg,
                });
            }
        }
    }
    out
}

#[derive(Debug)]
enum ScanState {
    Outside,
    Inside { depth: usize, in_string: Option<u8> },
}

#[cfg(test)]
mod tests {
    include!("helpers_tests.rs");
}
