//! Playwright helper-import resolver.
//!
//! Carved out of `main.rs` as part of Wave R6-5 (Rails-style refactor).
//! Resolves the user-written `e2e/helpers/{api,session,onboarding-progress}.ts`
//! files for the Vite frontend into the relative-import strings the
//! Playwright fixture emitter needs. Gracefully degrades to
//! `PlaywrightFixtureConfig::without_helpers()` whenever a helper is
//! missing or the relative-import calculation fails — Playwright fixtures
//! still emit, they just don't pre-import unavailable helpers.

use std::fs;
use std::path::Path;

use crate::lazurite_manifest;

pub(crate) fn playwright_fixture_config(
    project_root: &Path,
    manifest: Option<&lazurite_manifest::Manifest>,
) -> lazuli_codegen_ts::playwright::PlaywrightFixtureConfig {
    let Some(frontend) = manifest
        .and_then(|manifest| {
            manifest.frontends.values().find(|frontend| {
                matches!(
                    frontend.target,
                    lazurite_manifest::FrontendTarget::TanstackVite
                )
            })
        })
        .and_then(|frontend| frontend.source.as_deref())
    else {
        return lazuli_codegen_ts::playwright::PlaywrightFixtureConfig::without_helpers();
    };

    let helper_dir = project_root.join(frontend).join("e2e").join("helpers");
    let api = helper_dir.join("api.ts");
    let session = helper_dir.join("session.ts");
    if !api.is_file() || !session.is_file() {
        return lazuli_codegen_ts::playwright::PlaywrightFixtureConfig::without_helpers();
    }

    let from_dir = project_root.join("dist").join("ts-web").join("tests");
    let Some(api_import) = relative_ts_import(&from_dir, &api) else {
        return lazuli_codegen_ts::playwright::PlaywrightFixtureConfig::without_helpers();
    };
    let Some(session_import) = relative_ts_import(&from_dir, &session) else {
        return lazuli_codegen_ts::playwright::PlaywrightFixtureConfig::without_helpers();
    };

    let onboarding = helper_dir.join("onboarding-progress.ts");
    let (lifecycle_import, lifecycle_seeders) = if onboarding.is_file() {
        let contents = fs::read_to_string(&onboarding).unwrap_or_default();
        let import = relative_ts_import(&from_dir, &onboarding);
        let seeders = ["host", "traveler", "operator"]
            .into_iter()
            .filter_map(|role| {
                let function_name = format!("progress{}To", playwright_fixture_pascal_case(role));
                if contents.contains(&format!("function {function_name}"))
                    || contents.contains(&format!("function* {function_name}"))
                {
                    Some(lazuli_codegen_ts::playwright::LifecycleSeeder {
                        role: role.to_owned(),
                        function_name,
                    })
                } else {
                    None
                }
            })
            .collect();
        (import, seeders)
    } else {
        (None, Vec::new())
    };

    lazuli_codegen_ts::playwright::PlaywrightFixtureConfig {
        helpers: Some(
            lazuli_codegen_ts::playwright::PlaywrightFixtureHelperImports {
                api_import,
                session_import,
                lifecycle_import,
                lifecycle_seeders,
            },
        ),
    }
}

fn relative_ts_import(from_dir: &Path, target_file: &Path) -> Option<String> {
    let from = normalized_components(from_dir);
    let target = normalized_components(target_file);
    let mut common = 0usize;
    while common < from.len() && common < target.len() && from[common] == target[common] {
        common += 1;
    }
    if common == 0 {
        return None;
    }

    let mut parts = Vec::new();
    for _ in common..from.len() {
        parts.push("..".to_owned());
    }
    parts.extend(target[common..].iter().cloned());
    let mut import = parts.join("/");
    if let Some(stripped) = import.strip_suffix(".ts") {
        import = stripped.to_owned();
    } else if let Some(stripped) = import.strip_suffix(".tsx") {
        import = stripped.to_owned();
    }
    if !import.starts_with('.') {
        import = format!("./{import}");
    }
    Some(import)
}

fn normalized_components(path: &Path) -> Vec<String> {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy().replace('\\', "/"))
        .collect()
}

fn playwright_fixture_pascal_case(value: &str) -> String {
    let mut out = String::new();
    for word in value.split(['_', '-', ' ']) {
        if word.is_empty() {
            continue;
        }
        let mut chars = word.chars();
        if let Some(first) = chars.next() {
            out.extend(first.to_uppercase());
            out.push_str(&chars.as_str().to_ascii_lowercase());
        }
    }
    out
}
