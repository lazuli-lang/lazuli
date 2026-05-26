//! File-name → platform / audience pairing for `.lzx` files.
//!
//! `*.web.lzx` files must declare `surface <experience> web`; the
//! `*.mobile.lzx` counterpart must declare `surface <experience>
//! mobile`. Plain `*.lzx` files must declare `experience <name>`
//! (abstract). Mismatches emit errors so the file location, the
//! header, and the projection axis cannot drift apart.

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, Url};

use crate::{leading_spaces, simple_canonical_diagnostic};

pub(crate) fn lzx_filename_diagnostics(uri: &Url, source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let Some(file_name) = uri
        .path_segments()
        .and_then(|mut segments| segments.next_back())
        .map(str::to_owned)
    else {
        return diagnostics;
    };

    let expected_platform = lzx_platform_from_file_name(&file_name);
    let surface_header = first_lzx_surface_header(source);

    match (expected_platform, surface_header) {
        (Some(platform), Some((line_index, line, header_platform))) => {
            if header_platform != platform {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::ERROR,
                    "lzx-file-header-mismatch",
                    &format!(
                        "`{file_name}` is a `{platform}` projection, so its header should use `surface <experience> {platform}`."
                    ),
                ));
            }
        }
        (Some(platform), None) => {
            diagnostics.push(simple_canonical_diagnostic(
                0,
                source.lines().next().unwrap_or(""),
                DiagnosticSeverity::ERROR,
                "lzx-file-header-mismatch",
                &format!(
                    "`{file_name}` is a `{platform}` projection, so it should declare `surface <experience> {platform}`."
                ),
            ));
        }
        (None, Some((line_index, line, _))) if file_name.ends_with(".lzx") => {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::ERROR,
                "lzx-file-header-mismatch",
                "abstract `.lzx` files declare `experience <name>`; concrete surfaces belong in `.web.lzx` or `.mobile.lzx` files.",
            ));
        }
        _ => {}
    }

    diagnostics
}

pub(crate) fn lzx_platform_from_file_name(file_name: &str) -> Option<&'static str> {
    if file_name.ends_with(".web.lzx") {
        Some("web")
    } else if file_name.ends_with(".mobile.lzx") {
        Some("mobile")
    } else {
        None
    }
}

pub(crate) fn first_lzx_surface_header(source: &str) -> Option<(usize, &str, &str)> {
    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        if leading_spaces(line) != 0 || !trimmed.starts_with("surface ") {
            continue;
        }
        let platform = trimmed.split_whitespace().nth(2)?;
        return Some((line_index, line, platform));
    }

    None
}
