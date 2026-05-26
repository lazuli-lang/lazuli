//! `experience` / `surface` header shape + audience scoping checks.
//!
//! Walks the source once accumulating the open surface header (line +
//! `uses experience` presence + platform) and emits diagnostics for:
//!
//! * partial overrides (`+=` / `-=` are forbidden)
//! * `opens <target>` without an explicit argument binding
//! * platform `submit <verb>` instead of a typed command reference
//! * surface header shape (`surface <experience> <platform>`)
//! * concrete views outside `audience` blocks
//! * mobile views using web primitives (`Table` / `SidePanel`)
//! * view extensions whose `block` lives outside a `slot`

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use crate::{is_identifier, leading_spaces, simple_canonical_diagnostic};

pub(crate) fn lzx_contract_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut current_surface: Option<(usize, String, bool, Option<String>)> = None;
    let mut in_audience = false;
    let mut in_view_extension = false;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if trimmed.contains("+=") || trimmed.contains("-=") {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::ERROR,
                "lzx-no-partial-override",
                "`.lzx` forbids partial overrides such as `+=`/`-=`. Redeclare the whole view for this audience/tenant so the block remains a local truth.",
            ));
        }

        if leading_spaces(line) == 4
            && let Some(target) = trimmed.strip_prefix("opens ")
            && !target.contains('(')
        {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "lzx-open-binding",
                "view navigation should bind route arguments explicitly, e.g. `opens detail(id: row.id)`, so generators do not infer row identity.",
            ));
        }

        if leading_spaces(line) == 6
            && let Some(target) = trimmed.strip_prefix("submit ")
            && is_identifier(target.trim())
        {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "lzx-submit-target",
                "platform form submits should use an explicit command reference such as `command.create` or `customer.command.capture_lead`, not a bare verb.",
            ));
        }

        if leading_spaces(line) == 0 {
            in_view_extension = false;

            if let Some((surface_line, surface_header, has_uses_experience, _)) =
                current_surface.take()
                && !has_uses_experience
            {
                diagnostics.push(simple_canonical_diagnostic(
                    surface_line,
                    &surface_header,
                    DiagnosticSeverity::ERROR,
                    "lzx-surface-dependency",
                    "concrete `.lzx` surfaces must declare `uses experience <name>`; platform views project the abstract experience instead of calling `.lzi` directly.",
                ));
            }

            in_audience = false;

            if trimmed.starts_with("surface ") {
                let parts: Vec<_> = trimmed.split_whitespace().collect();
                if parts.len() == 2 && matches!(parts.get(1), Some(&"web" | &"mobile")) {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        line,
                        DiagnosticSeverity::ERROR,
                        "lzx-surface-header",
                        "put the experience name before the platform: `surface <experience> web` or `surface <experience> mobile`.",
                    ));
                } else if parts.len() < 3 {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        line,
                        DiagnosticSeverity::ERROR,
                        "lzx-surface-header",
                        "concrete `.lzx` surfaces use `surface <experience> <platform>`, with protected platforms `web` or `mobile`.",
                    ));
                } else {
                    if matches!(parts.get(1), Some(&"web" | &"mobile")) {
                        diagnostics.push(simple_canonical_diagnostic(
                            line_index,
                            line,
                            DiagnosticSeverity::ERROR,
                            "lzx-surface-header",
                            "put the experience name before the platform: `surface <experience> web` or `surface <experience> mobile`.",
                        ));
                    }
                    if !matches!(parts.get(2), Some(&"web" | &"mobile")) {
                        diagnostics.push(simple_canonical_diagnostic(
                            line_index,
                            line,
                            DiagnosticSeverity::ERROR,
                            "lzx-platform",
                            "canonical `.lzx` platform suffixes are protected: use `web` or `mobile` in the surface header; product axes belong in `audience`/`tenant` blocks.",
                        ));
                    }
                }
                current_surface = Some((
                    line_index,
                    line.to_owned(),
                    false,
                    parts.get(2).map(|platform| (*platform).to_owned()),
                ));
            }

            continue;
        }

        if leading_spaces(line) == 2 {
            in_view_extension = trimmed.starts_with("extends @anchor.");
        } else if leading_spaces(line) < 2 {
            in_view_extension = false;
        }

        if in_view_extension && leading_spaces(line) == 4 && trimmed.starts_with("block ") {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "lzx-extension-slot",
                "view extensions should place blocks under an explicit slot, e.g. `slot aside` then `block @client.tag_editor`, so composition order and placement are deterministic.",
            ));
        }

        if let Some((_, _, has_uses_experience, platform)) = current_surface.as_mut() {
            if leading_spaces(line) == 2 {
                if trimmed.starts_with("uses experience ") {
                    *has_uses_experience = true;
                    in_audience = false;
                    continue;
                }

                if trimmed.starts_with("audience ") {
                    in_audience = true;
                    continue;
                }

                if trimmed.starts_with("view ") {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        line,
                        DiagnosticSeverity::ERROR,
                        "lzx-audience-required",
                        "concrete platform views live under `audience ...` blocks. Product axes are source syntax, not filename-only convention.",
                    ));
                }
            } else if leading_spaces(line) <= 2 {
                in_audience = false;
            } else if trimmed.starts_with("view ") && !in_audience {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::ERROR,
                    "lzx-audience-required",
                    "concrete platform views live under `audience ...` blocks.",
                ));
            } else if in_audience
                && leading_spaces(line) == 4
                && platform.as_deref() == Some("mobile")
                && let Some(view_type) = trimmed.split_whitespace().nth(2)
                && matches!(view_type, "Table" | "SidePanel")
            {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::WARNING,
                    "lzx-mobile-primitive",
                    "mobile projections should use mobile-native primitives such as `List`, `Screen`, or `Sheet` instead of web-oriented `Table`/`SidePanel`.",
                ));
            }
        }
    }

    if let Some((surface_line, surface_header, has_uses_experience, _)) = current_surface
        && !has_uses_experience
    {
        diagnostics.push(simple_canonical_diagnostic(
            surface_line,
            &surface_header,
            DiagnosticSeverity::ERROR,
            "lzx-surface-dependency",
            "concrete `.lzx` surfaces must declare `uses experience <name>`; platform views project the abstract experience instead of calling `.lzi` directly.",
        ));
    }

    diagnostics
}
