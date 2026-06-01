//! `lazuli plugin verify [--plugin <ns>] [--json]` — end-to-end plugin
//! wiring proof (spec 0022).
//!
//! Walks a project's `Lazurite.toml [plugins]` through the SAME
//! authoritative resolver real codegen uses (0019/0020's
//! `find_project_root` + `lazurite_manifest::load` + `build_alias_map`,
//! 0021's typed `PluginManifest` loader) and reports, per plugin, a
//! PASS/FAIL across an ordered link chain:
//!
//! - **L1 manifest** — `manifest.toml` found at the resolved plugin root
//!   and parsed into the typed 0021 shape with a `[plugin]` block.
//! - **L2 semantic** — every `@semantic.*` the plugin contributes resolves
//!   in the authoritative alias map. `n/a` for adapter-only plugins.
//! - **L3 contract** — delegates to the SHARED
//!   `lazuli_manifest::plugin_contract::classify_adapter_contract` (the
//!   same classifier `PLUGIN-CONTRACT-001` calls). `n/a` for semantic-only
//!   plugins. Tailed with the honest static-limit note.
//! - **L4 import** — the plugin's `go_module` resolves (from `go.mod` for
//!   local plugins, exactly as `read_plugin_go_module` does for codegen),
//!   so `main.go`'s `_ "<go_module>"` side-effect import WILL emit.
//! - **L5 env** — every var in the manifest's required `[env]` set is
//!   present in the app's env contract (`.env` / `.env.example` keys).
//!
//! A broken earlier link short-circuits the MEANING of deeper links, which
//! render `skipped`. Exit code is non-zero if ANY plugin has ANY FAIL.
//!
//! Honest static limit: L3 verifies the DECLARED contract + the wiring
//! graph only. Whether the Go `Adapter` type satisfies the interface's
//! method set is the plugin's runtime `var _ <Interface> = (*Adapter)(nil)`
//! assertion under `go build` — never claimed here.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use serde::Serialize;

use crate::lazurite_manifest::{self, Manifest, Plugin};
use crate::plugin_manifest::{
    self, PluginManifest, build_alias_map, load_plugin_manifest, resolve_plugin_root,
};

/// Status of a single wiring link.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkStatus {
    Pass,
    Fail,
    /// The link does not apply to this plugin (e.g. L3 on a semantic-only
    /// plugin, L2 on an adapter-only plugin).
    Na,
    /// An earlier link broke, so this link's meaning is undefined.
    Skipped,
}

impl LinkStatus {
    fn label(&self) -> &'static str {
        match self {
            LinkStatus::Pass => "PASS",
            LinkStatus::Fail => "FAIL",
            LinkStatus::Na => "n/a",
            LinkStatus::Skipped => "skipped",
        }
    }
}

/// One wiring link in a plugin's report.
#[derive(Debug, Clone, Serialize)]
pub struct Link {
    /// Stable link id (`L1 manifest`, `L2 semantic`, …).
    pub id: String,
    pub status: LinkStatus,
    /// Human detail — the broken-link reason + fix on FAIL, a short note
    /// otherwise.
    pub detail: String,
}

/// Overall per-plugin verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Overall {
    Pass,
    Fail,
}

/// One plugin's full wiring report.
#[derive(Debug, Clone, Serialize)]
pub struct PluginVerifyReport {
    pub plugin: String,
    pub overall: Overall,
    pub links: Vec<Link>,
}

/// The full `--json` document.
#[derive(Debug, Clone, Serialize)]
pub struct VerifyDocument {
    pub plugins: Vec<PluginVerifyReport>,
    pub ok: bool,
}

const HONEST_LIMIT: &str = lazuli_manifest::plugin_contract::HONEST_LIMIT_NOTE;

/// Entry point — resolve the project root, build the reports, render
/// (human or `--json`), and return the process exit code (non-zero on any
/// FAIL). `only` scopes to a single plugin ref; an unknown ref bails.
pub fn run_plugin_verify(input: &Path, only: Option<&str>, json: bool) -> Result<i32> {
    let Some(root) = lazurite_manifest::find_project_root(input) else {
        bail!("not a Lazuli project (no Lazurite.toml found)");
    };

    let manifest = match lazurite_manifest::load(&root)? {
        Some(m) => m,
        None => bail!("not a Lazuli project (no Lazurite.toml found)"),
    };

    if manifest.plugins.is_empty() {
        if json {
            let doc = VerifyDocument {
                plugins: vec![],
                ok: true,
            };
            println!("{}", serde_json::to_string_pretty(&doc)?);
        } else {
            println!("no plugins declared in [plugins]");
        }
        return Ok(0);
    }

    if let Some(ns) = only
        && !manifest.plugins.contains_key(ns)
    {
        bail!("plugin '{ns}' not declared in [plugins]");
    }

    let reports = build_reports(&manifest, &root, only);
    let ok = reports.iter().all(|r| r.overall == Overall::Pass);

    if json {
        let doc = VerifyDocument {
            plugins: reports,
            ok,
        };
        println!("{}", serde_json::to_string_pretty(&doc)?);
    } else {
        render_human(&reports, ok);
    }

    Ok(if ok { 0 } else { 1 })
}

/// Pure core — build the per-plugin reports for a loaded manifest. Shared
/// by `run_plugin_verify` and the CLI tests (which assert on the structs
/// without `process::exit`).
pub fn build_reports(
    manifest: &Manifest,
    root: &Path,
    only: Option<&str>,
) -> Vec<PluginVerifyReport> {
    // Authoritative alias map (same `(manifest, root)` codegen feeds).
    let alias_map = build_alias_map(Some(manifest), root).unwrap_or_default();
    // App env contract — the union of `.env` / `.env.example` keys at the
    // project root (the only statically-readable env surface v1 has).
    let env_contract = read_app_env_contract(root);

    let registry = lazuli_manifest::plugin_contract::RegistryView::empty();

    manifest
        .plugins
        .iter()
        .filter(|(plugin_ref, _)| only.is_none_or(|ns| ns == plugin_ref.as_str()))
        .map(|(plugin_ref, plugin)| {
            build_one_report(
                manifest,
                root,
                plugin_ref,
                plugin,
                &alias_map,
                &env_contract,
                &registry,
            )
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn build_one_report(
    manifest: &Manifest,
    root: &Path,
    plugin_ref: &str,
    plugin: &Plugin,
    alias_map: &std::collections::BTreeMap<String, plugin_manifest::ResolvedPluginSemantic>,
    env_contract: &EnvContract,
    registry: &lazuli_manifest::plugin_contract::RegistryView,
) -> PluginVerifyReport {
    let mut links: Vec<Link> = Vec::new();

    // ── L1 manifest ──────────────────────────────────────────────────
    let plugin_root = resolve_plugin_root(manifest, root, plugin_ref);
    let typed: Option<PluginManifest> = match &plugin_root {
        Some(pr) => match load_plugin_manifest(pr) {
            Ok(Some(m)) if m.plugin.is_some() => {
                links.push(link(
                    "L1 manifest",
                    LinkStatus::Pass,
                    format!("manifest.toml parsed at {}", pr.display()),
                ));
                Some(m)
            }
            Ok(Some(m)) => {
                links.push(link(
                    "L1 manifest",
                    LinkStatus::Fail,
                    format!(
                        "manifest.toml at {} parses but lacks a [plugin] block (legacy flat schema) — add [plugin] with name + namespace + go_module",
                        pr.join(plugin_manifest::PLUGIN_MANIFEST_FILENAME).display()
                    ),
                ));
                return finish(plugin_ref, links, Some(m), true);
            }
            Ok(None) => {
                links.push(link(
                    "L1 manifest",
                    LinkStatus::Fail,
                    format!(
                        "no manifest.toml at {} — every plugin must ship one with a [plugin] block",
                        pr.join(plugin_manifest::PLUGIN_MANIFEST_FILENAME).display()
                    ),
                ));
                return finish(plugin_ref, links, None, true);
            }
            Err(err) => {
                links.push(link(
                    "L1 manifest",
                    LinkStatus::Fail,
                    format!("manifest.toml at {} failed to parse: {err}", pr.display()),
                ));
                return finish(plugin_ref, links, None, true);
            }
        },
        None => {
            // Remote plugin with no local override — manifest not on disk.
            links.push(link(
                "L1 manifest",
                LinkStatus::Fail,
                format!(
                    "could not resolve a local root for `{plugin_ref}` (remote plugin without a dev.plugin_paths override) — verify needs the plugin on disk"
                ),
            ));
            return finish(plugin_ref, links, None, true);
        }
    };
    let typed = typed.expect("L1 Pass implies a typed manifest");
    let plugin_root = plugin_root.expect("L1 Pass implies a resolved root");

    // ── L2 semantic ──────────────────────────────────────────────────
    let declared_aliases: Vec<String> = typed
        .semantic_types
        .iter()
        .map(|s| s.alias.clone())
        .collect();
    if declared_aliases.is_empty() {
        links.push(link(
            "L2 semantic",
            LinkStatus::Na,
            "plugin contributes no @semantic.* types".to_string(),
        ));
    } else {
        let unresolved: Vec<&String> = declared_aliases
            .iter()
            .filter(|a| !alias_map.contains_key(*a))
            .collect();
        if unresolved.is_empty() {
            links.push(link(
                "L2 semantic",
                LinkStatus::Pass,
                format!(
                    "{} semantic alias(es) resolve in the authoritative alias map",
                    declared_aliases.len()
                ),
            ));
        } else {
            links.push(link(
                "L2 semantic",
                LinkStatus::Fail,
                format!(
                    "semantic alias(es) {} declared in manifest.toml do not resolve in the authoritative alias map — check namespace/carrier/conflict",
                    unresolved
                        .iter()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            ));
        }
    }

    // ── L3 contract ──────────────────────────────────────────────────
    let contract = lazuli_manifest::plugin_contract::classify_adapter_contract(
        &typed, plugin_ref, registry,
    );
    use lazuli_manifest::plugin_contract::ContractStatus;
    match contract {
        ContractStatus::NotAnAdapter => {
            links.push(link(
                "L3 contract",
                LinkStatus::Na,
                format!("plugin declares no implements/[binds] interface. {HONEST_LIMIT}"),
            ));
        }
        ContractStatus::Ok => {
            let ifaces = lazuli_manifest::plugin_contract::declared_interfaces(&typed);
            links.push(link(
                "L3 contract",
                LinkStatus::Pass,
                format!(
                    "declared interface(s) [{}] are known bucket interfaces. {HONEST_LIMIT}",
                    ifaces.join(", ")
                ),
            ));
        }
        ContractStatus::UnknownInterface { declared, nearest } => {
            links.push(link(
                "L3 contract",
                LinkStatus::Fail,
                format!(
                    "implements '{declared}' is not a known bucket interface (did you mean '{nearest}'?). {HONEST_LIMIT}"
                ),
            ));
        }
        ContractStatus::UnboundCapability {
            capability,
            bound_to,
        } => {
            links.push(link(
                "L3 contract",
                LinkStatus::Fail,
                format!(
                    "declares capability '{capability}' but the app registry binds it to '{bound_to}' — bind it to this plugin or remove the declaration. {HONEST_LIMIT}"
                ),
            ));
        }
    }

    // ── L4 import ─────────────────────────────────────────────────────
    let go_module = resolve_go_module(root, plugin, &plugin_root);
    match go_module {
        Some(module) => {
            links.push(link(
                "L4 import",
                LinkStatus::Pass,
                format!("go_module '{module}' resolves — the `_ \"{module}\"` side-effect import will emit in main.go"),
            ));
        }
        None => {
            links.push(link(
                "L4 import",
                LinkStatus::Fail,
                format!(
                    "go_module unresolved — no 'module ...' directive in {}/go.mod; the side-effect import will NOT emit and the adapter will be ErrAdapterMissing at runtime",
                    plugin_root.display()
                ),
            ));
        }
    }

    // ── L5 env ────────────────────────────────────────────────────────
    let required_env: Vec<String> = typed
        .env
        .as_ref()
        .map(|e| {
            let mut v = e.required.clone();
            v.extend(e.required_for_auth.clone());
            v
        })
        .unwrap_or_default();
    if required_env.is_empty() {
        links.push(link(
            "L5 env",
            LinkStatus::Na,
            "manifest declares no required [env] vars".to_string(),
        ));
    } else if !env_contract.readable {
        // No `.env` / `.env.example` to read — can't prove presence; don't
        // false-FAIL. Report n/a with a note (consistent with the
        // PLUGIN-UNUSED warning stance: absence of evidence is not a FAIL).
        links.push(link(
            "L5 env",
            LinkStatus::Na,
            format!(
                "no .env/.env.example at the project root to check {} required var(s) against — add one to verify env wiring",
                required_env.len()
            ),
        ));
    } else {
        let missing: Vec<&String> = required_env
            .iter()
            .filter(|v| !env_contract.keys.contains(*v))
            .collect();
        if missing.is_empty() {
            links.push(link(
                "L5 env",
                LinkStatus::Pass,
                format!(
                    "all {} required env var(s) present in the app env contract",
                    required_env.len()
                ),
            ));
        } else {
            links.push(link(
                "L5 env",
                LinkStatus::Fail,
                format!(
                    "required env var(s) missing from the app env contract (.env/.env.example): {}",
                    missing
                        .iter()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            ));
        }
    }

    let overall = if links.iter().any(|l| l.status == LinkStatus::Fail) {
        Overall::Fail
    } else {
        Overall::Pass
    };
    PluginVerifyReport {
        plugin: plugin_ref.to_string(),
        overall,
        links,
    }
}

/// Finish a report whose L1 broke — deeper links are `skipped` (their
/// meaning is undefined without a parsed manifest).
fn finish(
    plugin_ref: &str,
    mut links: Vec<Link>,
    _typed: Option<PluginManifest>,
    l1_failed: bool,
) -> PluginVerifyReport {
    if l1_failed {
        for (id, why) in [
            ("L2 semantic", "manifest did not parse"),
            ("L3 contract", "manifest did not parse"),
            ("L4 import", "manifest did not parse"),
            ("L5 env", "manifest did not parse"),
        ] {
            links.push(link(id, LinkStatus::Skipped, why.to_string()));
        }
    }
    let overall = if links.iter().any(|l| l.status == LinkStatus::Fail) {
        Overall::Fail
    } else {
        Overall::Pass
    };
    PluginVerifyReport {
        plugin: plugin_ref.to_string(),
        overall,
        links,
    }
}

fn link(id: &str, status: LinkStatus, detail: String) -> Link {
    Link {
        id: id.to_string(),
        status,
        detail,
    }
}

/// Resolve the plugin's effective Go module path the way codegen does:
/// Remote → the declared module; Local → the first-line `module` directive
/// in `<plugin_root>/go.mod`. Mirrors
/// `lazurite_codegen::read_plugin_go_module` so L4 predicts the real
/// side-effect-import emission.
fn resolve_go_module(_root: &Path, plugin: &Plugin, plugin_root: &Path) -> Option<String> {
    match plugin {
        Plugin::Remote { module, .. } => Some(module.clone()),
        Plugin::Local { .. } => read_go_mod_module(plugin_root),
    }
}

fn read_go_mod_module(plugin_root: &Path) -> Option<String> {
    let go_mod = plugin_root.join("go.mod");
    let contents = std::fs::read_to_string(&go_mod).ok()?;
    for line in contents.lines().take(40) {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("module ") {
            let module = rest.split("//").next()?.trim();
            if !module.is_empty() {
                return Some(module.to_owned());
            }
        }
    }
    None
}

/// The app's statically-readable env contract: the union of variable keys
/// declared in `.env` and `.env.example` at the project root.
struct EnvContract {
    keys: BTreeSet<String>,
    /// True when at least one env file was found+read.
    readable: bool,
}

fn read_app_env_contract(root: &Path) -> EnvContract {
    let mut keys = BTreeSet::new();
    let mut readable = false;
    for name in [".env", ".env.example", ".env.sample"] {
        let Ok(contents) = std::fs::read_to_string(root.join(name)) else {
            continue;
        };
        readable = true;
        for line in contents.lines() {
            let t = line.trim();
            if t.is_empty() || t.starts_with('#') {
                continue;
            }
            let key = t
                .trim_start_matches("export ")
                .split('=')
                .next()
                .unwrap_or("")
                .trim();
            if !key.is_empty() {
                keys.insert(key.to_string());
            }
        }
    }
    EnvContract { keys, readable }
}

fn render_human(reports: &[PluginVerifyReport], ok: bool) {
    println!("lazuli plugin verify — {} plugin(s)\n", reports.len());
    for r in reports {
        let badge = match r.overall {
            Overall::Pass => "PASS",
            Overall::Fail => "FAIL",
        };
        println!("[{badge}] {}", r.plugin);
        for l in &r.links {
            println!("    {:<14} {:<8} {}", l.id, l.status.label(), l.detail);
        }
        println!();
    }
    if ok {
        println!("all plugins wired (declared contract + wiring graph verified).");
    } else {
        println!(
            "one or more plugins FAILED — fix the broken link(s) above. Note: L3 verifies the DECLARED contract only; method-set conformance is the runtime `var _ <Interface> = (*Adapter)(nil)` assertion under `go build`."
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_contract_parses_keys_and_skips_comments() {
        let dir = std::env::temp_dir().join(format!(
            "lazuli-verify-env-{}-{}",
            std::process::id(),
            "a"
        ));
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(
            dir.join(".env"),
            "# comment\nFOO=1\nexport BAR=2\n\nBAZ=\n",
        )
        .unwrap();
        let c = read_app_env_contract(&dir);
        assert!(c.readable);
        assert!(c.keys.contains("FOO"));
        assert!(c.keys.contains("BAR"));
        assert!(c.keys.contains("BAZ"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn env_contract_absent_is_not_readable() {
        let dir = std::env::temp_dir().join(format!(
            "lazuli-verify-noenv-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        let c = read_app_env_contract(&dir);
        assert!(!c.readable);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
