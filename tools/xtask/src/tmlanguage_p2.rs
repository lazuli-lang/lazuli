/// Longest-literal-first ordering so overlapping prefixes (e.g. `timestamps`
/// vs `timestamp`) never short-circuit a longer match. Ties broken
/// alphabetically for determinism.
fn order_longest_first(lits: &[&str]) -> Vec<String> {
    let mut v: Vec<String> = lits.iter().map(|s| s.to_string()).collect();
    v.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
    v
}

/// Escape a literal for safe embedding inside a JSON tmLanguage regex
/// alternation. `serde_json` handles the JSON-string escaping of backslashes;
/// here we only escape regex metacharacters that can appear in a keyword
/// literal (`.` in dotted forms like `view.board`). Identifier characters and
/// `_` need no escaping.
fn escape_literal(lit: &str) -> String {
    let mut s = String::with_capacity(lit.len());
    for ch in lit.chars() {
        match ch {
            '.' | '+' | '*' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '|' | '^' | '$' | '\\' => {
                s.push('\\');
                s.push(ch);
            }
            _ => s.push(ch),
        }
    }
    s
}

/// Render the `#kw-*` rules object exactly the way it is serialized inside the
/// committed grammar so the `--check` comparison is byte-exact: 2-space
/// indent, keys in `GROUPS` order, each rule indented to the grammar's
/// `repository`-child depth (4 spaces) — matching `serde_json` pretty output
/// re-indented by 4.
fn render_kw_section(rules: &Map<String, Value>) -> Result<String, String> {
    // Serialize each rule with serde_json's 2-space pretty printer, then
    // re-indent so it nests under `"repository": {` at the file's depth.
    // Every generated rule is followed by either another rule or the
    // `_kw_generated_end` sentinel — so each line group always ends with a
    // trailing comma (the section is spliced *between* two sentinel lines).
    let mut lines = Vec::new();
    for (key, rule) in rules.iter() {
        let body = serde_json::to_string_pretty(rule)
            .map_err(|e| format!("serializing #{key} rule: {e}"))?;
        // Re-indent every line by 4 spaces (repository-child depth).
        let mut reindented = String::new();
        for line in body.lines() {
            reindented.push_str("    ");
            reindented.push_str(line);
            reindented.push('\n');
        }
        let trimmed = reindented.trim_end_matches('\n');
        lines.push(format!("    \"{}\": {},", key, trimmed.trim_start()));
    }
    Ok(lines.join("\n"))
}

fn workspace_root() -> PathBuf {
    // CARGO_MANIFEST_DIR is .../tools/xtask; climb two levels to the root.
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop(); // xtask
    p.pop(); // tools
    p
}

/// Entry point for the `gen-tmlanguage` subcommand.
pub fn run(check: bool) -> Result<(), String> {
    let grammar_path = workspace_root().join(GRAMMAR_REL);
    let raw = std::fs::read_to_string(&grammar_path)
        .map_err(|e| format!("reading {}: {e}", grammar_path.display()))?;

    let rules = build_kw_rules();
    let fresh_section = render_kw_section(&rules)?;

    let committed_section = extract_kw_section(&raw)?;

    if check {
        if committed_section.trim_end() == fresh_section.trim_end() {
            println!(
                "gen-tmlanguage --check: #kw-* section is fresh ({} rules).",
                rules.len()
            );
            Ok(())
        } else {
            Err(format!(
                "stale #kw-* section in {} — run `cargo xtask gen-tmlanguage`.\n\
                 --- committed ---\n{}\n--- fresh ---\n{}",
                GRAMMAR_REL, committed_section, fresh_section
            ))
        }
    } else {
        if committed_section.trim_end() == fresh_section.trim_end() {
            println!(
                "gen-tmlanguage: #kw-* section already fresh ({} rules), no write.",
                rules.len()
            );
            return Ok(());
        }
        let updated = splice_kw_section(&raw, &fresh_section)?;
        std::fs::write(&grammar_path, updated)
            .map_err(|e| format!("writing {}: {e}", grammar_path.display()))?;
        println!(
            "gen-tmlanguage: wrote {} #kw-* rules to {}.",
            rules.len(),
            GRAMMAR_REL
        );
        Ok(())
    }
}

/// Sentinel keys bounding the generated region inside `repository`.
const BEGIN_MARK: &str = "    \"_kw_generated_begin\":";
const END_MARK: &str = "    \"_kw_generated_end\":";

/// Extract the committed `#kw-*` section: everything strictly between the
/// begin/end sentinel marker lines.
fn extract_kw_section(raw: &str) -> Result<String, String> {
    let begin = raw
        .find(BEGIN_MARK)
        .ok_or_else(|| format!("sentinel `{}` not found in grammar", BEGIN_MARK.trim()))?;
    let after_begin = raw[begin..]
        .find('\n')
        .map(|n| begin + n + 1)
        .ok_or("malformed begin sentinel")?;
    let end = raw
        .find(END_MARK)
        .ok_or_else(|| format!("sentinel `{}` not found in grammar", END_MARK.trim()))?;
    Ok(raw[after_begin..end].trim_end_matches('\n').to_string())
}

/// Replace the committed `#kw-*` section between the sentinels with `fresh`.
fn splice_kw_section(raw: &str, fresh: &str) -> Result<String, String> {
    let begin = raw
        .find(BEGIN_MARK)
        .ok_or_else(|| format!("sentinel `{}` not found in grammar", BEGIN_MARK.trim()))?;
    let after_begin = raw[begin..]
        .find('\n')
        .map(|n| begin + n + 1)
        .ok_or("malformed begin sentinel")?;
    let end = raw
        .find(END_MARK)
        .ok_or_else(|| format!("sentinel `{}` not found in grammar", END_MARK.trim()))?;
    let mut out = String::with_capacity(raw.len() + fresh.len());
    out.push_str(&raw[..after_begin]);
    out.push_str(fresh);
    out.push('\n');
    out.push_str(&raw[end..]);
    Ok(out)
}

/// Dump every `(surface, context, scope, sigil)` group → its literals as
/// JSON, for mapping the registry to the hand-written tmLanguage alternations.
pub fn dump_groups() {
    let mut groups: BTreeMap<String, Vec<&'static str>> = BTreeMap::new();
    for c in ALL {
        let key = format!(
            "{:?} | {:?} | {} | {:?}",
            c.surface, c.context, c.scope, c.sigil
        );
        groups.entry(key).or_default().push(c.literal);
    }
    let mut out = serde_json::Map::new();
    for (k, mut lits) in groups {
        lits.sort_unstable();
        lits.dedup();
        out.insert(
            k,
            serde_json::Value::Array(lits.into_iter().map(|s| s.into()).collect()),
        );
    }
    match serde_json::to_string_pretty(&serde_json::Value::Object(out)) {
        Ok(s) => println!("{s}"),
        Err(e) => eprintln!("dump-groups: serialization failed: {e}"),
    }
}
