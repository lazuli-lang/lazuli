use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Wire shape of the runtime error envelope `lazuli debug` ingests.
///
/// Mirrors the JSON the Lazuli Go runtime emits on a typed-error or
/// panic fallout. Every field is optional except the closed-catalog
/// discriminators (`code`/`surface`/`capsule`/`feature`/`kind`/`op`)
/// — the triage path needs those to look up the right IR slot.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ErrorEnvelopeInput {
    /// Stable error code (e.g. `policy_denied`).
    pub code: String,
    /// Where the error originated. Closed catalog: `user_dsl`,
    /// `lib_internal`, `codegen_bug`, `adapter_runtime`.
    pub surface: String,
    /// Capsule that hosted the failing op.
    pub capsule: String,
    /// Feature the op lives under.
    pub feature: String,
    /// Op kind (`command`/`query`/...).
    pub kind: String,
    /// Op name.
    pub op: String,
    /// Optional source-path hint (relative to the project root).
    pub source: Option<String>,
    /// Optional human message captured at fault time.
    pub message: Option<String>,
    /// Field name when the error narrows to one field.
    #[serde(default)]
    pub field: Option<String>,
    /// HTTP path when the surface had one.
    #[serde(default)]
    pub path: Option<String>,
    /// Author-facing reason override.
    #[serde(default)]
    pub reason: Option<String>,
}

/// Triage bundle returned by `run_debug`.
#[derive(Debug, Serialize)]
pub struct DebugBundle {
    /// Original envelope (echoed verbatim for context).
    pub error: ErrorEnvelopeInput,
    /// Recommended remediation phrase based on `error.surface`.
    pub recommended_action: String,
    /// Snippet of the offending `.lzi` block, when located.
    pub lzi_block: Option<String>,
    /// Slice of the IR around the failing op.
    pub ir_snippet: Option<serde_json::Value>,
    /// Rough JSON-token estimate (size/4) for downstream LLM
    /// quota budgeting.
    pub estimated_tokens: u32,
}

/// Build a [`DebugBundle`] for one runtime [`ErrorEnvelopeInput`].
///
/// Loads the project's IR, locates the failing op, extracts the
/// relevant `.lzi` snippet (when the envelope carries a `source`
/// pointer), and computes a token estimate so agents can budget their
/// follow-up prompts.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_cli::debug::{run_debug, ErrorEnvelopeInput};
///
/// // let bundle = run_debug(Path::new("."), envelope)?;
/// ```
pub fn run_debug(
    project_root: &Path,
    error_input: ErrorEnvelopeInput,
) -> Result<DebugBundle, Box<dyn std::error::Error>> {
    let ir = load_project_ir(project_root, &error_input)?;
    let op_ir = find_op_in_ir(
        &ir,
        &error_input.capsule,
        &error_input.feature,
        &error_input.kind,
        &error_input.op,
    );

    let lzi_block = if let Some(source) = &error_input.source {
        extract_lzi_block(project_root, source).ok()
    } else {
        None
    };

    let recommended_action = recommended_action_for_surface(&error_input.surface).to_owned();

    let bundle = DebugBundle {
        error: error_input,
        recommended_action,
        lzi_block,
        ir_snippet: op_ir,
        estimated_tokens: 0,
    };
    let estimated_tokens = estimate_tokens(&bundle);

    Ok(DebugBundle {
        estimated_tokens,
        ..bundle
    })
}

/// Map the envelope's `surface` field to a short remediation
/// sentence. The catalog mirrors the closed list in
/// [`ErrorEnvelopeInput::surface`].
///
/// ## Examples
///
/// ```ignore
/// use lazuli_cli::debug::recommended_action_for_surface;
/// assert!(recommended_action_for_surface("user_dsl").contains(".lzi"));
/// ```
pub fn recommended_action_for_surface(surface: &str) -> &'static str {
    match surface {
        "user_dsl" => "Read the `.lzi` block above and the IR snippet. Modify the .lzi source.",
        "lib_internal" => {
            "This is a Lazuli runtime bug. File an issue at https://github.com/lazuli-lang/lazuli/issues/new with this bundle."
        }
        "codegen_bug" => {
            "This is a codegen bug. Reproduce, then file an issue against crates/lazuli_codegen_go."
        }
        "adapter_runtime" => {
            "This is a failure in an external adapter. Check the adapter's logs and policy."
        }
        _ => "Unknown surface; treat as user_dsl by default.",
    }
}

/// Rough token estimate for a serialized [`DebugBundle`] (bytes / 4).
/// Good enough for "does this fit a prompt window?" heuristics; not a
/// tokenizer.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_cli::debug::estimate_tokens;
/// // let count = estimate_tokens(&bundle);
/// ```
pub fn estimate_tokens(bundle: &DebugBundle) -> u32 {
    let json_str = serde_json::to_string(bundle).unwrap_or_default();
    (json_str.len() / 4) as u32
}

fn load_project_ir(
    project_root: &Path,
    error_input: &ErrorEnvelopeInput,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let source_path = error_input
        .source
        .as_deref()
        .and_then(|source| resolve_source_path(project_root, source).ok())
        .or_else(|| find_single_lzi(project_root))
        .unwrap_or_else(|| super::inspect_source_path(project_root));
    let source = fs::read_to_string(&source_path)?;
    let mut expansions = super::ExpandSet::default();
    expansions.summary = true;
    Ok(super::inspect_json_value(
        &source,
        &source_path,
        project_root,
        expansions,
        &[],
    )?)
}

fn find_op_in_ir(
    ir: &serde_json::Value,
    _capsule: &str,
    feature: &str,
    kind: &str,
    op: &str,
) -> Option<serde_json::Value> {
    let root = ir.get("ir").unwrap_or(ir);
    let feature_json = root
        .get("features")?
        .as_array()?
        .iter()
        .find(|candidate| candidate.get("name").and_then(|v| v.as_str()) == Some(feature))?;

    if kind == "agent" {
        if let Some(agent) = find_named_object(feature_json.get("agents"), op) {
            return Some(agent.clone());
        }
    }

    if let Some(summary) = feature_json.get("summary") {
        let list_key = match kind {
            "command" => "commands",
            "query" => "queries",
            "job" => "jobs",
            "webhook" => "webhooks",
            "notification" => "notifications",
            "event" => "events",
            other => other,
        };
        if summary
            .get(list_key)
            .and_then(|value| value.as_array())
            .is_some_and(|items| items.iter().any(|item| item.as_str() == Some(op)))
        {
            return Some(serde_json::json!({
                "feature": feature,
                "kind": kind,
                "op": op,
                "summary": summary,
            }));
        }
    }

    Some(feature_json.clone())
}

fn find_named_object<'a>(
    value: Option<&'a serde_json::Value>,
    name: &str,
) -> Option<&'a serde_json::Value> {
    value?
        .as_array()?
        .iter()
        .find(|candidate| candidate.get("name").and_then(|v| v.as_str()) == Some(name))
}

/// Read the source file referenced by `source` (relative to
/// `project_root`) and return its contents verbatim. Used to seed the
/// `lzi_block` field of a [`DebugBundle`] so the agent sees the
/// authored source alongside the error.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_cli::debug::extract_lzi_block;
/// // let src = extract_lzi_block(Path::new("."), "app.lzi");
/// ```
pub fn extract_lzi_block(
    project_root: &Path,
    source: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let path = resolve_source_path(project_root, source)?;
    let line = source_line(source).unwrap_or(1);
    let contents = fs::read_to_string(path)?;
    let lines: Vec<&str> = contents.lines().collect();
    if lines.is_empty() {
        return Ok(String::new());
    }

    let start_index = line.saturating_sub(1).min(lines.len() - 1);
    let start_indent = indentation(lines[start_index]);
    let mut block_start = start_index;
    while block_start > 0 {
        let prev = lines[block_start - 1];
        if prev.trim().is_empty() {
            block_start -= 1;
            continue;
        }
        let prev_indent = indentation(prev);
        if prev_indent < start_indent || prev.trim_start().starts_with("feature ") {
            block_start -= 1;
            break;
        }
        block_start -= 1;
    }

    let header_indent = indentation(lines[block_start]);
    let mut block_end = lines.len();
    for idx in (block_start + 1)..lines.len() {
        let current = lines[idx];
        if current.trim().is_empty() {
            continue;
        }
        if indentation(current) <= header_indent {
            block_end = idx;
            break;
        }
    }

    Ok(lines[block_start..block_end].join("\n"))
}

fn resolve_source_path(
    project_root: &Path,
    source: &str,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let path_part = source
        .rsplit_once(':')
        .map(|(left, _)| left)
        .and_then(|left| left.rsplit_once(':').map(|(path, _)| path))
        .unwrap_or(source);
    let raw = Path::new(path_part);
    if raw.is_absolute() && raw.exists() {
        return Ok(raw.to_path_buf());
    }

    let joined = project_root.join(raw);
    if joined.exists() {
        return Ok(joined);
    }
    if raw.exists() {
        return Ok(raw.to_path_buf());
    }

    let file_name = raw
        .file_name()
        .ok_or_else(|| format!("source path has no file name: {source}"))?;
    if let Some(found) = find_file_by_name(project_root, file_name) {
        return Ok(found);
    }

    Err(format!("failed to resolve source path: {source}").into())
}

fn find_single_lzi(project_root: &Path) -> Option<PathBuf> {
    let mut found = Vec::new();
    collect_lzi_files(project_root, &mut found);
    if found.len() == 1 { found.pop() } else { None }
}

fn find_file_by_name(project_root: &Path, file_name: &std::ffi::OsStr) -> Option<PathBuf> {
    let mut stack = vec![project_root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.file_name() == Some(file_name) {
                return Some(path);
            }
        }
    }
    None
}

fn collect_lzi_files(dir: &Path, found: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_lzi_files(&path, found);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("lzi") {
            found.push(path);
        }
    }
}

fn source_line(source: &str) -> Option<usize> {
    source
        .rsplit(':')
        .nth(1)
        .and_then(|line| line.parse::<usize>().ok())
}

fn indentation(line: &str) -> usize {
    line.chars().take_while(|ch| *ch == ' ').count()
}

/// Render a [`DebugBundle`] as the canonical human-readable markdown
/// report (sections: Error, Recommended action, .lzi block, IR
/// snippet, Estimated tokens).
///
/// ## Examples
///
/// ```ignore
/// use lazuli_cli::debug::format_markdown;
/// // let md = format_markdown(&bundle);
/// ```
pub fn format_markdown(bundle: &DebugBundle) -> String {
    let mut out = String::new();
    out.push_str("# Lazuli Debug Bundle\n\n");
    out.push_str("## Error\n\n");
    out.push_str("```json\n");
    out.push_str(&serde_json::to_string_pretty(&bundle.error).unwrap_or_default());
    out.push_str("\n```\n\n");
    out.push_str("## Recommended action\n\n");
    out.push_str(&bundle.recommended_action);
    out.push_str("\n\n");
    if let Some(block) = &bundle.lzi_block {
        out.push_str("## LZI block\n\n```lzi\n");
        out.push_str(block);
        out.push_str("\n```\n\n");
    }
    if let Some(snippet) = &bundle.ir_snippet {
        out.push_str("## IR snippet\n\n```json\n");
        out.push_str(&serde_json::to_string_pretty(snippet).unwrap_or_default());
        out.push_str("\n```\n\n");
    }
    out.push_str(&format!("Estimated tokens: {}\n", bundle.estimated_tokens));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_error_envelope_with_field_specifics() {
        let input = r#"{"code":"field_invalid","surface":"user_dsl","capsule":"crm","feature":"customer","kind":"command","op":"create_customer","source":"features/customer.lzi:42:8","message":"bad field","field":"email","path":"input.email","reason":"invalid_format"}"#;
        let envelope: ErrorEnvelopeInput = serde_json::from_str(input).unwrap();

        assert_eq!(envelope.code, "field_invalid");
        assert_eq!(envelope.field.as_deref(), Some("email"));
        assert_eq!(envelope.path.as_deref(), Some("input.email"));
        assert_eq!(envelope.reason.as_deref(), Some("invalid_format"));
    }

    #[test]
    fn recommended_action_routes_by_surface() {
        assert!(recommended_action_for_surface("user_dsl").contains("Modify the .lzi source"));
        assert!(recommended_action_for_surface("lib_internal").contains("runtime bug"));
        assert!(recommended_action_for_surface("codegen_bug").contains("codegen bug"));
        assert!(recommended_action_for_surface("adapter_runtime").contains("external adapter"));
    }

    #[test]
    fn estimate_tokens_returns_reasonable_count() {
        let bundle = DebugBundle {
            error: ErrorEnvelopeInput {
                code: "field_invalid".to_owned(),
                surface: "user_dsl".to_owned(),
                capsule: "minimal_demo".to_owned(),
                feature: "minimal_demo".to_owned(),
                kind: "command".to_owned(),
                op: "create_item".to_owned(),
                source: Some("minimal_command.lzi:5:1".to_owned()),
                message: None,
                field: Some("name".to_owned()),
                path: None,
                reason: Some("required".to_owned()),
            },
            recommended_action: recommended_action_for_surface("user_dsl").to_owned(),
            lzi_block: Some("command create_item\n  input\n    name: Text required".to_owned()),
            ir_snippet: Some(serde_json::json!({"feature":"minimal_demo","op":"create_item"})),
            estimated_tokens: 0,
        };

        assert!(estimate_tokens(&bundle) < 4000);
    }
}
