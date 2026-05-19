//! Doctor bridge for LAZ-87 lifecycle-gate analyzer diagnostics.

use lazuli_analyzer::checks::lifecycle_gate::{
    LifecycleGateInput, LifecycleGateOrigin, LifecycleGateResume, LifecycleGateResumeArm,
    LifecycleGateResumeSource, LifecycleGateSeverity, LifecycleGateView, RequiresLifecycle,
    check_input,
};

use super::{DoctorAppManifest, DoctorDiagnostic, DoctorFile, DoctorSeverity, Tier3FeatureFacts};

pub(super) fn diagnostics(
    files: &[DoctorFile],
    _app: Option<&DoctorAppManifest>,
    facts: &[Tier3FeatureFacts],
) -> Vec<DoctorDiagnostic> {
    let features: Vec<lazuli_ir::Feature> = facts
        .iter()
        .map(|fact| {
            let mut feature = super::make_synthetic_feature_for_error_vocab(fact);
            feature.queries = fact.queries.clone();
            feature.resources = fact.resources.clone();
            feature
        })
        .collect();

    let mut out = Vec::new();
    for file in files.iter().filter(|f| is_lzx_file(f)) {
        let input = scan_lzx(file);
        if input.views.iter().all(|v| v.requires.is_none()) && input.resumes.is_empty() {
            continue;
        }
        for finding in check_input(&input, &features) {
            let path = match finding.origin {
                LifecycleGateOrigin::App => _app
                    .map(|a| a.path.clone())
                    .unwrap_or_else(|| file.path.clone()),
                LifecycleGateOrigin::Lzx => file.path.clone(),
            };
            let line = super::span_line(files, &path, finding.span, 1);
            out.push(DoctorDiagnostic {
                path,
                line,
                column: 1,
                severity: match finding.severity {
                    LifecycleGateSeverity::Error => DoctorSeverity::Error,
                    LifecycleGateSeverity::Warning => DoctorSeverity::Warning,
                    LifecycleGateSeverity::Info => DoctorSeverity::Info,
                },
                code: finding.code.to_owned(),
                message: finding.message,
            });
        }
    }
    out
}

fn is_lzx_file(file: &DoctorFile) -> bool {
    file.path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("lzx"))
        .unwrap_or(false)
}

#[derive(Clone)]
struct LogicalLine<'a> {
    indent: usize,
    text: &'a str,
    span: lazuli_ir::SpanRef,
}

fn scan_lzx(file: &DoctorFile) -> LifecycleGateInput {
    let mut input = LifecycleGateInput::default();
    let mut current_feature: Option<String> = None;
    let mut last_feature: Option<String> = None;
    let mut experience: Option<(String, usize)> = None;
    let mut view: Option<(usize, usize)> = None;
    let mut resume: Option<(usize, usize)> = None;

    for line in logical_lines(&file.source) {
        if line.text.is_empty() {
            continue;
        }
        if let Some((_, indent)) = resume {
            if line.indent <= indent {
                resume = None;
            }
        }
        if let Some((_, indent)) = view {
            if line.indent <= indent {
                view = None;
            }
        }
        if let Some((_, indent)) = experience.as_ref() {
            if line.indent <= *indent {
                experience = None;
            }
        }

        if line.indent == 0 {
            if let Some(name) = line.text.strip_prefix("feature ") {
                current_feature = Some(name.trim().to_owned());
                last_feature = current_feature.clone();
                continue;
            }
        }

        if let Some(name) = line.text.strip_prefix("experience ") {
            let name = name.split_whitespace().next().unwrap_or_default();
            let feature = current_feature.clone().unwrap_or_else(|| name.to_owned());
            last_feature = Some(feature.clone());
            experience = Some((feature, line.indent));
            continue;
        }

        if let Some(name) = line.text.strip_prefix("resume ") {
            let feature = current_feature
                .clone()
                .or_else(|| last_feature.clone())
                .unwrap_or_else(|| "app".to_owned());
            input.resumes.push(LifecycleGateResume {
                feature,
                name: name
                    .split_whitespace()
                    .next()
                    .unwrap_or_default()
                    .to_owned(),
                source: None,
                arms: Vec::new(),
                span: Some(line.span),
            });
            resume = Some((input.resumes.len() - 1, line.indent));
            continue;
        }

        if let Some((idx, _)) = resume {
            if let Some(source) = parse_source(&input.resumes[idx].feature, &line) {
                input.resumes[idx].source = Some(source);
                continue;
            }
            if let Some(arm) = parse_arm(&line) {
                input.resumes[idx].arms.push(arm);
                continue;
            }
        }

        if let Some((feature, indent)) = experience.as_ref() {
            if line.indent == indent + 2 && line.text.starts_with("view ") {
                let name = line
                    .text
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or_default()
                    .to_owned();
                input.views.push(LifecycleGateView {
                    feature: feature.clone(),
                    name,
                    policy_present: false,
                    requires: None,
                    on_lifecycle_pending: None,
                    span: Some(line.span),
                });
                view = Some((input.views.len() - 1, line.indent));
                continue;
            }
        }

        if let Some((idx, _)) = view {
            if line.text.starts_with("policy ") {
                input.views[idx].policy_present = true;
            } else if let Some(req) = parse_requires(&line) {
                input.views[idx].requires = Some(req);
            } else if let Some(pending) = line.text.strip_prefix("on_lifecycle_pending ") {
                input.views[idx].on_lifecycle_pending = Some(pending.trim().to_owned());
            }
        }
    }

    input
}

fn logical_lines(source: &str) -> Vec<LogicalLine<'_>> {
    let mut out = Vec::new();
    let mut offset = 0;
    for raw in source.split_inclusive('\n') {
        let line = raw.trim_end_matches(['\r', '\n']);
        let base_indent = line.chars().take_while(|c| *c == ' ').count();
        let mut text = &line[base_indent..];
        let mut indent = base_indent;
        if let Some(rest) = text.strip_prefix('#') {
            let spaces = rest.chars().take_while(|c| *c == ' ').count();
            indent += spaces.saturating_sub(1);
            text = &rest[spaces..];
        }
        out.push(LogicalLine {
            indent,
            text: text.trim_end(),
            span: lazuli_ir::SpanRef {
                start: offset + base_indent,
                end: offset + line.len(),
            },
        });
        offset += raw.len();
    }
    out
}

fn parse_requires(line: &LogicalLine<'_>) -> Option<RequiresLifecycle> {
    let rest = line.text.strip_prefix("requires_lifecycle ")?;
    let (resource, state) = rest.split_once('=')?;
    Some(RequiresLifecycle {
        resource: resource.trim().to_owned(),
        state: state.trim().to_owned(),
        span: Some(line.span),
    })
}

fn parse_source(feature: &str, line: &LogicalLine<'_>) -> Option<LifecycleGateResumeSource> {
    let rest = line.text.strip_prefix("source ")?;
    let parts: Vec<_> = rest.split_whitespace().collect();
    let (head, query) = match parts.as_slice() {
        [head, query] => (*head, *query),
        _ => return None,
    };
    let dotted: Vec<_> = head.split('.').collect();
    let (source_feature, kind, query) = match dotted.as_slice() {
        ["query", kind] => (None, Some((*kind).to_owned()), query.to_owned()),
        [source_feature, "query", kind] => (
            Some((*source_feature).to_owned()),
            Some((*kind).to_owned()),
            query.to_owned(),
        ),
        [source_feature, "query", kind, name] => (
            Some((*source_feature).to_owned()),
            Some((*kind).to_owned()),
            (*name).to_owned(),
        ),
        _ => (Some(feature.to_owned()), None, query.to_owned()),
    };
    Some(LifecycleGateResumeSource {
        feature: source_feature,
        kind,
        query,
        text: rest.to_owned(),
        span: Some(line.span),
    })
}

fn parse_arm(line: &LogicalLine<'_>) -> Option<LifecycleGateResumeArm> {
    let (state, target) = line
        .text
        .split_once("->")
        .or_else(|| line.text.split_once('\u{2192}'))?;
    let target = target.trim();
    let target_view = target.strip_prefix("view ").unwrap_or(target).trim();
    Some(LifecycleGateResumeArm {
        state: state.trim().to_owned(),
        target_view: target_view.to_owned(),
        span: Some(line.span),
    })
}
