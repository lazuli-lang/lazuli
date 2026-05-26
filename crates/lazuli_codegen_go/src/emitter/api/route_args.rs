//! API path → typed args struct.
//!
//! `api <name>` has no typed IR slot for path parameters; we infer the
//! arg shape from the route segments (`:id`, `{slug}`) and synthesize a
//! Go struct so the contract value can be `lazuli.Api[Args, Output]`.

use super::super::casing::pascal_case;
use super::super::printer::GoPrinter;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ApiArg {
    pub(super) name: String,
    pub(super) go_type: String,
}

pub(super) fn emit_args_struct(p: &mut GoPrinter, name: &str, args: &[ApiArg]) {
    if args.is_empty() {
        p.line(&format!("type {name} struct{{}}"));
        return;
    }

    p.line(&format!("type {name} struct {{"));
    p.indent();
    let rows: Vec<(String, String, String)> = args
        .iter()
        .map(|arg| {
            (
                pascal_case(&arg.name),
                arg.go_type.clone(),
                format!("`json:\"{}\"`", arg.name),
            )
        })
        .collect();
    let row_refs: Vec<(&str, &str, &str)> = rows
        .iter()
        .map(|(name, ty, tag)| (name.as_str(), ty.as_str(), tag.as_str()))
        .collect();
    p.aligned_struct_rows(&row_refs);
    p.dedent();
    p.line("}");
}

pub(super) fn route_args(path: &str) -> Vec<ApiArg> {
    let mut args = Vec::new();
    let mut seen = Vec::<String>::new();

    for name in path_params(path) {
        if name.is_empty() || seen.iter().any(|existing| existing == &name) {
            continue;
        }
        let go_type = infer_route_arg_type(&name).to_owned();
        seen.push(name.clone());
        args.push(ApiArg { name, go_type });
    }

    args
}

fn path_params(path: &str) -> Vec<String> {
    let mut out = Vec::new();
    for segment in path.split('/') {
        if let Some(raw) = segment.strip_prefix(':') {
            let name = raw
                .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
                .next()
                .unwrap_or("")
                .trim();
            if !name.is_empty() {
                out.push(name.to_owned());
            }
            continue;
        }
        out.extend(brace_params(segment));
    }
    out
}

fn brace_params(path: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = path;
    while let Some(start) = rest.find('{') {
        let after_start = &rest[start + 1..];
        let Some(end) = after_start.find('}') else {
            break;
        };
        out.push(after_start[..end].trim().to_owned());
        rest = &after_start[end + 1..];
    }
    out
}

fn infer_route_arg_type(name: &str) -> &'static str {
    let lower = name.to_ascii_lowercase();
    if lower == "id" || lower.ends_with("_id") {
        "lazuli.ID"
    } else {
        "string"
    }
}
