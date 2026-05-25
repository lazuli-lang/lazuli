//! Parser for `profiles.lzi` — per-environment overrides over the base
//! `app.lzi` + `registry.lzi`.
//!
//! Each `profile <name>` block carries optional `urls` (per-target URL
//! overrides), `bindings` (re-target a feature slot to a different
//! registry source for that environment), `integrations` (swap the
//! `environment` or `adapter` of a registry integration), and `deploy`
//! (topology / migration / rollback overrides). The IR layer
//! (`AppProfile`) is intentionally additive — the runtime overlays
//! profile values on top of the base app at build time; we do not merge
//! here.
//!
//! Profile names are arbitrary identifiers; doctor cross-checks them
//! against `app.environments` so unused profiles surface.
//!
//! See: `lazuli_ir::nodes::app_manifest::AppProfile`,
//!      `lazuli_syntax::ast::feature::PackageSkeleton`.

use lazuli_ir::{AppProfile, AppProfileDeploy, AppProfileIntegration, AppProfileUrl};

use super::parsers::{
    adapter_source_provenance, is_identifier, leading_spaces, parse_app_binding, profile_child,
    unquote,
};

pub fn parse_app_profiles(source: &str) -> Vec<AppProfile> {
    let lines: Vec<_> = source.lines().collect();
    let mut profiles = Vec::new();
    let mut index = 0;

    while index < lines.len() {
        let trimmed = lines[index].trim_start();
        if leading_spaces(lines[index]) != 0 || !trimmed.starts_with("profile ") {
            index += 1;
            continue;
        }

        let Some(name) = trimmed.split_whitespace().nth(1) else {
            index += 1;
            continue;
        };
        if !is_identifier(name) {
            index += 1;
            continue;
        }

        let start = index;
        index += 1;
        while index < lines.len() {
            let next_trimmed = lines[index].trim_start();
            if leading_spaces(lines[index]) == 0
                && !next_trimmed.is_empty()
                && !next_trimmed.starts_with('#')
            {
                break;
            }
            index += 1;
        }

        profiles.push(parse_app_profile_block(name, &lines[start + 1..index]));
    }

    profiles
}

fn parse_app_profile_block(name: &str, lines: &[&str]) -> AppProfile {
    let mut profile = AppProfile {
        name: name.to_owned(),
        urls: Vec::new(),
        bindings: Vec::new(),
        integrations: Vec::new(),
        deploy: None,
    };
    let mut current_child: Option<&str> = None;

    for line in lines {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        match leading_spaces(line) {
            2 => current_child = profile_child(trimmed),
            4 => match current_child {
                Some("urls") => {
                    let parts: Vec<_> = trimmed.split_whitespace().collect();
                    if parts.len() == 2 && is_identifier(parts[0]) {
                        profile.urls.push(AppProfileUrl {
                            target: parts[0].to_owned(),
                            url: unquote(parts[1]).to_owned(),
                        });
                    }
                }
                Some("bindings") => {
                    if let Some(binding) = parse_app_binding(trimmed) {
                        profile.bindings.push(binding);
                    }
                }
                Some("integrations") => {
                    let parts: Vec<_> = trimmed.split_whitespace().collect();
                    match parts.as_slice() {
                        [name, "environment", environment]
                            if is_identifier(name) && is_identifier(environment) =>
                        {
                            upsert_profile_integration(&mut profile, name).environment =
                                Some((*environment).to_owned());
                        }
                        [name, "adapter", adapter] if is_identifier(name) => {
                            let integration = upsert_profile_integration(&mut profile, name);
                            integration.adapter = Some((*adapter).to_owned());
                            integration.adapter_provenance =
                                adapter_source_provenance(adapter).map(str::to_owned);
                        }
                        _ => {}
                    }
                }
                Some("deploy") => {
                    let deploy = profile.deploy.get_or_insert_with(AppProfileDeploy::default);
                    let parts: Vec<_> = trimmed.split_whitespace().collect();
                    match parts.as_slice() {
                        ["topology", value] => deploy.topology = Some((*value).to_owned()),
                        ["migrations", value] => deploy.migrations = Some((*value).to_owned()),
                        ["migration_lock", value] => {
                            deploy.migration_lock = Some((*value).to_owned());
                        }
                        ["destructive_migrations", value] => {
                            deploy.destructive_migrations = Some((*value).to_owned());
                        }
                        ["rollback", value] => deploy.rollback = Some((*value).to_owned()),
                        _ => {}
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }

    profile
}

fn upsert_profile_integration<'a>(
    profile: &'a mut AppProfile,
    name: &str,
) -> &'a mut AppProfileIntegration {
    if let Some(index) = profile
        .integrations
        .iter()
        .position(|integration| integration.name == name)
    {
        return &mut profile.integrations[index];
    }

    profile.integrations.push(AppProfileIntegration {
        name: name.to_owned(),
        environment: None,
        adapter: None,
        adapter_provenance: None,
    });
    let index = profile.integrations.len() - 1;
    &mut profile.integrations[index]
}
