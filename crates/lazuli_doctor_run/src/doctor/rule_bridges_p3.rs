/// PATTERN-DRAFT-STALE-001 - a `//lazuli:pattern <id> draft`
/// annotation has been on main for more than 7 days.
///
/// The check bounds the draft escape hatch for in-progress codegen
/// patterns. It focuses on the Rust-side pattern catalog because
/// generated `dist/` output is regen-only and often absent in source
/// checkouts. If git/blame data is unavailable, the check is a no-op.
pub(crate) fn check_pattern_draft_stale_001(project_root: &Path) -> Vec<DoctorDiagnostic> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    check_pattern_draft_stale_001_at(project_root, now)
}

pub(crate) fn check_pattern_draft_stale_001_at(
    project_root: &Path,
    now: u64,
) -> Vec<DoctorDiagnostic> {
    let pattern_file = project_root.join("crates/lazuli_codegen_go/src/emitter/patterns.rs");
    let Ok(source) = fs::read_to_string(&pattern_file) else {
        return Vec::new();
    };

    let mut diagnostics = Vec::new();
    let seven_days_seconds: u64 = 7 * 24 * 60 * 60;
    for (lineno, line) in source.lines().enumerate() {
        if !is_pattern_draft_line(line) {
            continue;
        }

        let Some(author_time) = git_blame_author_time(project_root, &pattern_file, lineno + 1)
        else {
            continue;
        };
        let age = now.saturating_sub(author_time);
        if age <= seven_days_seconds {
            continue;
        }

        diagnostics.push(DoctorDiagnostic {
            path: pattern_file.clone(),
            line: lineno + 1,
            column: 1,
            severity: DoctorSeverity::Error,
            code: "PATTERN-DRAFT-STALE-001".to_owned(),
            message: format!(
                "pattern annotation marked `draft` on main for {} days (> 7). Promote to a numbered version (v1, v2, ...) or remove. See docs/proposals/bucket-ai-debug-loop-cycle.md §6.3.",
                age / (24 * 60 * 60)
            ),
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    }

    diagnostics
}

/// AUTH-SESSION-CALLSITE-001 — direct `auth.IssueSession` call inside a
/// user-authored handler for a feature whose session resource has extra columns.
///
/// The v1 shim emits a per-resource `Issue<Resource>` wrapper that threads
/// the tenant-pin columns automatically; callers must use that wrapper, not
/// the base `auth.IssueSession` function, so the extra columns are always
/// supplied.
pub(crate) fn check_auth_session_callsite_001(
    auth_facts: &[AuthFacts],
    project_root: &Path,
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    let features_root = project_root.join("features");
    if !features_root.exists() {
        return diagnostics;
    }
    for fact in auth_facts {
        let sessions = match fact.auth.sessions.as_ref() {
            Some(s) if !s.extra_columns.is_empty() => s,
            _ => continue,
        };
        let feature_dir = features_root.join(&fact.feature);
        if !feature_dir.exists() {
            continue;
        }
        let resource_name = sessions.resource.name.as_str();
        collect_issue_session_callsites(&feature_dir, resource_name, &mut diagnostics);
    }
    diagnostics
}

pub(crate) fn collect_issue_session_callsites(
    dir: &Path,
    resource_name: &str,
    diagnostics: &mut Vec<DoctorDiagnostic>,
) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_issue_session_callsites(&path, resource_name, diagnostics);
            continue;
        }
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        if !file_name.ends_with(".go")
            || file_name.ends_with(".gen.go")
            || file_name.ends_with("_test.go")
        {
            continue;
        }
        let Ok(source) = fs::read_to_string(&path) else {
            continue;
        };
        for (idx, line) in source.lines().enumerate() {
            if line.contains("auth.IssueSession(") {
                diagnostics.push(DoctorDiagnostic {
                    path: path.clone(),
                    line: idx + 1,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "AUTH-SESSION-CALLSITE-001".to_owned(),
                    message: format!(
                        "direct call to `auth.IssueSession` in a feature with extra session columns; use `auth.Issue{resource_name}` instead so the tenant-pin column is always supplied.",
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }
        }
    }
}

pub(crate) fn is_pattern_draft_line(line: &str) -> bool {
    if !line.contains("draft") {
        return false;
    }
    (line.contains("PATTERN_") && line.contains("\"draft\"")) || line.contains("//lazuli:pattern")
}

pub(crate) fn git_blame_author_time(project_root: &Path, path: &Path, line: usize) -> Option<u64> {
    let blame_path = path.strip_prefix(project_root).unwrap_or(path);
    let output = std::process::Command::new("git")
        .args([
            "blame",
            "-L",
            &format!("{line},{line}"),
            "--porcelain",
            "--",
        ])
        .arg(blame_path)
        .current_dir(project_root)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let blame_text = String::from_utf8_lossy(&output.stdout);
    blame_text.lines().find_map(|line| {
        line.strip_prefix("author-time ")
            .and_then(|rest| rest.trim().parse::<u64>().ok())
    })
}

pub(crate) fn collect_codegen_wrap_001(
    bucket_root: &Path,
    current: &Path,
    diagnostics: &mut Vec<DoctorDiagnostic>,
) {
    let Ok(entries) = fs::read_dir(current) else {
        return;
    };

    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            collect_codegen_wrap_001(bucket_root, &path, diagnostics);
            continue;
        }
        if !is_bucket_go_source(bucket_root, &path) {
            continue;
        }

        let Ok(source) = fs::read_to_string(&path) else {
            continue;
        };
        for (lineno, line) in source.lines().enumerate() {
            for typed in [
                "FieldError",
                "PolicyError",
                "TenantError",
                "AdapterError",
                "LibBugError",
            ] {
                let needle = format!("lazuli.{typed}{{");
                if line.contains(&needle) {
                    diagnostics.push(DoctorDiagnostic {
                        path: path.clone(),
                        line: lineno + 1,
                        column: line.find(&needle).map(|col| col + 1).unwrap_or(1),
                        severity: DoctorSeverity::Error,
                        code: "CODEGEN-WRAP-001".to_owned(),
                        message: format!(
                            "typed error `{typed}` constructed in bucket source. The one-wrap boundary requires typed errors only at the codegen-emitted handler boundary. Return a bare sentinel from this bucket; codegen will wrap it. See bucket-ai-debug-loop-cycle.md §7.2."
                        ),
                        category: None,
                        feature_name: None,
                        construct: None,
                        fix: None,
                        group: None,
                    });
                }
            }
        }
    }
}

pub(crate) fn is_bucket_go_source(bucket_root: &Path, path: &Path) -> bool {
    if path.parent() == Some(bucket_root) {
        return false;
    }
    if path.extension().and_then(|ext| ext.to_str()) != Some("go") {
        return false;
    }
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    if name.ends_with("_test.go") {
        return false;
    }
    let rendered = path.to_string_lossy();
    !rendered.contains(".gen.")
}
