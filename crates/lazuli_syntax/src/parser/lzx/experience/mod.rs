//! `.lzx` **experience** dialect — `experience`, `surface` (app-side),
//! `audience`, and their resume / view extension grammar.
//!
//! ## What lives here
//!
//! - **`parse_lzx_experience`**: top-level `experience <name>` block,
//!   collecting `imports`, `view`, `resume`, and `extends @anchor.*`
//!   children. Called from `parse_lzx_document` in `app.rs`.
//! - **`parse_lzx_resume_router`** + **`parse_lzx_resume_arm`**: the
//!   `resume <name>` block (with `source query.lookup` and arms like
//!   `<state> -> view <name>`).
//! - **`parse_lzx_experience_view`**: `view <name>` inside an experience
//!   (anchor, source, submit, blocks, actions, tests, policy guard,
//!   lifecycle gate, on_lifecycle_pending).
//! - **`parse_lzx_view_extension`** + **`parse_lzx_extension_slot`**:
//!   `extends @anchor.<name>` blocks.
//! - **`parse_lzx_surface`**: the app-side `surface <experience> web|mobile`
//!   declaration that maps an experience onto a platform.
//! - **`parse_lzx_audience`** + **`parse_lzx_platform_view`**: the
//!   audience-scoped concrete views inside a surface.
//!
//! Shared guard helpers (`parse_lzx_view_guard`, lifecycle-gate parsing,
//! `attach_lzx_*`) come from the sibling `app` module via `use super::app::...`.

use crate::ast::{
    LzxAction, LzxExperience, LzxExperienceView, LzxPlatform, LzxRequiresLifecycle, LzxSurface,
    LzxViewTestAssertion, Span,
};

use super::super::common::{SourceLine, is_lzx_bare_ident, is_trivia, line_error, split_lzx_list};
use super::super::error::ParseError;

use super::app::{
    attach_lzx_on_lifecycle_pending, attach_lzx_requires_lifecycle, parse_lzx_on_lifecycle_pending,
    parse_lzx_requires_lifecycle, parse_lzx_view_guard,
};

mod resume;
use resume::parse_lzx_resume_router;

mod view_extension;
use view_extension::parse_lzx_view_extension;

mod audience;
use audience::parse_lzx_audience;

pub(super) fn parse_lzx_experience(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(LzxExperience, usize), ParseError> {
    let header = &lines[start];
    let parts: Vec<_> = header.text.trim_start().split_whitespace().collect();
    if parts.len() != 2 {
        return Err(line_error(header, "`experience` uses `experience <name>`"));
    }

    let mut imports = Vec::new();
    let mut views = Vec::new();
    let mut resume_routers = Vec::new();
    let mut extensions = Vec::new();
    let mut index = start + 1;

    while index < lines.len() {
        let line = &lines[index];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            index += 1;
            continue;
        }

        if line.indent == 0 {
            break;
        }

        if line.indent != 2 {
            return Err(line_error(
                line,
                "experience children use two-space indentation",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("imports ") {
            imports.extend(split_lzx_list(rest));
            index += 1;
        } else if trimmed.starts_with("view ") {
            let (view, next) = parse_lzx_experience_view(lines, index)?;
            views.push(view);
            index = next;
        } else if trimmed.starts_with("resume ") {
            let (resume, next) = parse_lzx_resume_router(lines, index)?;
            resume_routers.push(resume);
            index = next;
        } else if trimmed.starts_with("extends @anchor.") {
            let (extension, next) = parse_lzx_view_extension(lines, index)?;
            extensions.push(extension);
            index = next;
        } else {
            return Err(line_error(
                line,
                "experience children are `imports`, `view`, `resume`, or `extends @anchor.*` declarations",
            ));
        }
    }

    Ok((
        LzxExperience {
            name: parts[1].to_owned(),
            imports,
            views,
            resume_routers,
            extensions,
            span: Span::new(header.start, lines[index.saturating_sub(1)].end),
        },
        index,
    ))
}

fn parse_lzx_experience_view(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(LzxExperienceView, usize), ParseError> {
    let header = &lines[start];
    let parts: Vec<_> = header.text.trim_start().split_whitespace().collect();
    if parts.len() != 2 && !(parts.len() == 4 && parts[2] == "id") {
        return Err(line_error(
            header,
            "experience views use `view <name>` or `view <name> id @anchor.<name>`",
        ));
    }

    let mut anchor = (parts.len() == 4).then(|| parts[3].to_owned());
    let mut source = None;
    let mut submit = None;
    let mut routes = Vec::new();
    let mut extensible_by = Vec::new();
    let mut blocks = Vec::new();
    let mut actions = Vec::new();
    let mut opens = Vec::new();
    let mut tests = Vec::new();
    let mut guard = None;
    let mut pending_requires_lifecycle: Option<(usize, LzxRequiresLifecycle)> = None;
    let mut pending_on_lifecycle_pending: Option<(usize, String)> = None;
    let mut index = start + 1;

    while index < lines.len() {
        let line = &lines[index];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            index += 1;
            continue;
        }

        if line.indent <= 2 {
            break;
        }

        if line.indent != 4 {
            return Err(line_error(line, "view children use four-space indentation"));
        }

        if let Some(rest) = trimmed.strip_prefix("route ") {
            routes.push(rest.trim().to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("anchor ") {
            anchor = Some(rest.trim().to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("source ") {
            source = Some(rest.trim().to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("submit ") {
            submit = Some(rest.trim().to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("extensible_by ") {
            extensible_by = split_lzx_list(rest);
        } else if let Some(rest) = trimmed.strip_prefix("block ") {
            blocks.push(rest.trim().to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("action ") {
            let Some((name, target)) = rest.split_once(" -> ") else {
                return Err(line_error(line, "actions use `action <name> -> <target>`"));
            };
            actions.push(LzxAction {
                name: name.trim().to_owned(),
                target: target.trim().to_owned(),
                span: Span::new(line.start, line.end),
            });
        } else if let Some(rest) = trimmed.strip_prefix("opens ") {
            opens.push(rest.trim().to_owned());
        } else if trimmed.starts_with("policy ") {
            if guard.is_some() {
                return Err(line_error(line, "view declares `policy` at most once"));
            }
            let (mut parsed, next) = parse_lzx_view_guard(lines, index, 4)?;
            if let Some((pending_index, pending)) = pending_requires_lifecycle.take() {
                attach_lzx_requires_lifecycle(&lines[pending_index], &mut parsed, pending)?;
            }
            if let Some((pending_index, pending)) = pending_on_lifecycle_pending.take() {
                let pending_line = &lines[pending_index];
                attach_lzx_on_lifecycle_pending(
                    pending_line,
                    &mut parsed,
                    pending,
                    pending_line.end,
                )?;
            }
            guard = Some(parsed);
            index = next;
            continue;
        } else if let Some(rest) = trimmed.strip_prefix("requires_lifecycle ") {
            let parsed = parse_lzx_requires_lifecycle(line, rest.trim())?;
            if let Some(guard) = guard.as_mut() {
                attach_lzx_requires_lifecycle(line, guard, parsed)?;
            } else {
                if pending_requires_lifecycle.is_some() {
                    return Err(line_error(
                        line,
                        "view declares `requires_lifecycle` at most once",
                    ));
                }
                pending_requires_lifecycle = Some((index, parsed));
            }
        } else if let Some(rest) = trimmed.strip_prefix("on_lifecycle_pending ") {
            let parsed = parse_lzx_on_lifecycle_pending(line, rest.trim())?;
            if let Some(guard) = guard.as_mut() {
                attach_lzx_on_lifecycle_pending(line, guard, parsed, line.end)?;
            } else {
                if pending_on_lifecycle_pending.is_some() {
                    return Err(line_error(
                        line,
                        "view declares `on_lifecycle_pending` at most once",
                    ));
                }
                pending_on_lifecycle_pending = Some((index, parsed));
            }
        } else if trimmed == "tests" {
            index += 1;
            while index < lines.len() {
                let test_line = &lines[index];
                let test_trimmed = test_line.text.trim_start();
                if is_trivia(test_trimmed) {
                    index += 1;
                    continue;
                }
                if test_line.indent <= 4 {
                    break;
                }
                if test_line.indent != 6 {
                    return Err(line_error(
                        test_line,
                        "test assertions inside experience views use six-space indentation",
                    ));
                }
                // Wave 4 — view tests are an extensibility vocabulary,
                // not policy/predicate. Closed catalog: `accepted by
                // <feature>` / `rejected by <feature>`. Anything else is
                // a hard parse error (no silent acceptance).
                let assertion = if let Some(rest) = test_trimmed.strip_prefix("accepted by ") {
                    let feature = rest.trim().to_owned();
                    if feature.is_empty() {
                        return Err(line_error(
                            test_line,
                            "view test `accepted by` requires a feature name",
                        ));
                    }
                    LzxViewTestAssertion::AcceptedBy {
                        feature,
                        span: Span::new(test_line.start, test_line.end),
                    }
                } else if let Some(rest) = test_trimmed.strip_prefix("rejected by ") {
                    let feature = rest.trim().to_owned();
                    if feature.is_empty() {
                        return Err(line_error(
                            test_line,
                            "view test `rejected by` requires a feature name",
                        ));
                    }
                    LzxViewTestAssertion::RejectedBy {
                        feature,
                        span: Span::new(test_line.start, test_line.end),
                    }
                } else {
                    return Err(line_error(
                        test_line,
                        "view test assertion must start with `accepted by` or `rejected by` \
                             (extensibility vocabulary only — policy / predicate testing lives \
                             on commands, rules, and transitions)",
                    ));
                };
                tests.push(assertion);
                index += 1;
            }
            continue;
        } else {
            return Err(line_error(
                line,
                "view children are `route`, `anchor`, `source`, `submit`, `extensible_by`, `block`, `action`, `opens`, `policy`, `requires_lifecycle`, `on_lifecycle_pending`, or `tests`",
            ));
        }
        index += 1;
    }

    if let Some((pending_index, _)) = pending_requires_lifecycle.as_ref() {
        return Err(line_error(
            &lines[*pending_index],
            "`requires_lifecycle` currently requires a sibling `policy` guard",
        ));
    }
    if let Some((pending_index, _)) = pending_on_lifecycle_pending.as_ref() {
        return Err(line_error(
            &lines[*pending_index],
            "`on_lifecycle_pending` currently requires a sibling `policy` guard",
        ));
    }

    Ok((
        LzxExperienceView {
            name: parts[1].to_owned(),
            anchor,
            routes,
            extensible_by,
            source,
            submit,
            blocks,
            actions,
            opens,
            tests,
            guard,
            span: Span::new(header.start, lines[index.saturating_sub(1)].end),
        },
        index,
    ))
}

pub(super) fn parse_lzx_surface(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(LzxSurface, usize), ParseError> {
    let header = &lines[start];
    let parts: Vec<_> = header.text.trim_start().split_whitespace().collect();
    if parts.len() != 3 {
        return Err(line_error(
            header,
            "surfaces use `surface <experience> web|mobile`",
        ));
    }

    let platform = match parts[2] {
        "web" => LzxPlatform::Web,
        "mobile" => LzxPlatform::Mobile,
        _ => {
            return Err(line_error(
                header,
                "surface platform must be `web` or `mobile`",
            ));
        }
    };

    let mut uses_experience = None;
    let mut audiences = Vec::new();
    let mut index = start + 1;

    while index < lines.len() {
        let line = &lines[index];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            index += 1;
            continue;
        }

        if line.indent == 0 {
            break;
        }

        if line.indent != 2 {
            return Err(line_error(
                line,
                "surface children use two-space indentation",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("uses experience ") {
            uses_experience = Some(rest.trim().to_owned());
            index += 1;
        } else if trimmed.starts_with("audience ") {
            let (audience, next) = parse_lzx_audience(lines, index)?;
            audiences.push(audience);
            index = next;
        } else if trimmed.starts_with("view ") {
            return Err(line_error(
                line,
                "concrete platform views live under `audience ...` blocks",
            ));
        } else {
            return Err(line_error(
                line,
                "surface children are `uses experience` or `audience` declarations",
            ));
        }
    }

    Ok((
        LzxSurface {
            experience: parts[1].to_owned(),
            platform,
            uses_experience,
            audiences,
            span: Span::new(header.start, lines[index.saturating_sub(1)].end),
        },
        index,
    ))
}
