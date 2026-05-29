//! tmLanguage keyword-rule generation from `lazuli_keywords::ALL`.
//!
//! # What this generates
//!
//! Only the **pure keyword-alternation repository rules** (`#kw-*`) of the
//! VS Code grammar — one repo rule per per-block `(Context, scope)` group that
//! the hand-written grammar previously expressed as an inline
//! `^\s+(a|b|c)\b` alternation with a single scope `name`.
//!
//! Each generated rule is a faithful projection of the registry: its match
//! is `^\s+(<literals, longest-first>)\b` and its `name` is the group's
//! TextMate scope leaf. Because a literal valid in N contexts is N rows in
//! the registry (context-as-data), grouping by `(Context, scope)` reproduces
//! the grammar's per-block scope leaves exactly.
//!
//! # What it does NOT generate (stays hand-written, structural)
//!
//! * block `begin`/`end` rules, entity-name captures, the top-level
//!   `patterns` include list;
//! * strings, comments, operators, punctuation, `#references`, `#types`,
//!   `#decorators`, `#modifiers`, `#constants` (cross-cutting / regex-shaped);
//! * curated *multi-group fallback* alternations that deliberately merge a
//!   block's statement keywords with section headers + cross-cutting
//!   modifiers and are shadowed by dedicated `begin/end` sub-blocks
//!   (`command`, `query`, `api`, `webhook`, `job`, `agent`, `notification`,
//!   `report`, `view*`, `extends*`, `experience`, `poller`, `rule`, `role`,
//!   `aggregate`, `auth`, `errors`, `channel`, `tenant_migration`,
//!   `resource`, value-catalog alternations). These are not single-group
//!   projections.
//!
//! # Freshness
//!
//! `gen-tmlanguage --check` regenerates the `#kw-*` section in memory and
//! asserts it is byte-identical to the committed grammar. CI / the
//! `editors/vscode` grammar test wires this so a hand-edit or a forgotten
//! regen fails loudly.

use std::collections::BTreeMap;
use std::path::PathBuf;

use lazuli_keywords::{ALL, Context};
use serde_json::{Map, Value};

/// Path to the committed grammar, relative to the workspace root.
const GRAMMAR_REL: &str = "editors/vscode/syntaxes/lazuli.tmLanguage.json";

/// A generatable group: the registry `(Context, scope)` pair whose literals
/// the grammar expresses as a single-scope keyword alternation, plus the
/// stable `#kw-*` repository key the rule is written under.
///
/// This allowlist is the seam between "pure single-group projection"
/// (generated) and "curated structural alternation" (hand-written). Adding a
/// keyword row to one of these `(Context, scope)` groups in the registry
/// automatically widens the generated alternation — that is the drift-proof
/// guarantee. The registry is reconciled so each group's literal set equals
/// the alternation it replaced (see the H2 backfill block in `registry.rs`).
struct Group {
    key: &'static str,
    context: Context,
    scope: &'static str,
}

/// The blocks whose inline alternation is replaced by a `#kw-*` include.
/// Order here is the emission order of the `#kw-*` keys (stable).
///
/// Excluded on purpose (kept hand-written): the curated multi-group fallback
/// alternations (`command`, `query`, `api`, `webhook`, `job`, `agent`,
/// `notification`, `report`, `view*`, `extends*`, `experience`, `poller`,
/// `rule`, `role`, `aggregate`, `auth`, `errors`, `channel`,
/// `tenant_migration`, `resource`), and `locale` (two distinct alternations
/// share one `(Context, scope)` group → not a single-group projection).
const GROUPS: &[Group] = &[
    Group {
        key: "kw-cookie",
        context: Context::Cookie,
        scope: "entity.name.function.statement.cookie.lazuli",
    },
    Group {
        key: "kw-headers",
        context: Context::Headers,
        scope: "entity.name.function.statement.headers.lazuli",
    },
    Group {
        key: "kw-proxy",
        context: Context::Proxy,
        scope: "entity.name.function.statement.proxy.lazuli",
    },
    Group {
        key: "kw-limits",
        context: Context::Limits,
        scope: "entity.name.function.statement.limits.lazuli",
    },
    Group {
        key: "kw-encryption",
        context: Context::Encryption,
        scope: "entity.name.function.statement.encryption.lazuli",
    },
    Group {
        key: "kw-logging",
        context: Context::Logging,
        scope: "entity.name.function.statement.logging.lazuli",
    },
    Group {
        key: "kw-tracing",
        context: Context::Tracing,
        scope: "entity.name.function.statement.tracing.lazuli",
    },
    Group {
        key: "kw-runtime",
        context: Context::Runtime,
        scope: "entity.name.function.statement.runtime.lazuli",
    },
    Group {
        key: "kw-deploy",
        context: Context::Deploy,
        scope: "entity.name.function.statement.deploy.lazuli",
    },
    Group {
        key: "kw-services",
        context: Context::Services,
        scope: "entity.name.function.statement.services.lazuli",
    },
    Group {
        key: "kw-communication",
        context: Context::Communication,
        scope: "entity.name.function.statement.communication.lazuli",
    },
    Group {
        key: "kw-env",
        context: Context::Env,
        scope: "entity.name.function.statement.env.lazuli",
    },
    Group {
        key: "kw-integrations",
        context: Context::Integrations,
        scope: "entity.name.function.statement.integration.lazuli",
    },
    Group {
        key: "kw-packs",
        context: Context::Packs,
        scope: "entity.name.function.statement.packs.lazuli",
    },
    Group {
        key: "kw-cache",
        context: Context::Cache,
        scope: "entity.name.function.statement.cache.lazuli",
    },
    Group {
        key: "kw-translation",
        context: Context::Translation,
        scope: "entity.name.function.statement.translation.lazuli",
    },
    Group {
        key: "kw-tests",
        context: Context::Tests,
        scope: "entity.name.function.statement.tests.lazuli",
    },
    Group {
        key: "kw-defaults",
        context: Context::Defaults,
        scope: "entity.name.function.statement.defaults.lazuli",
    },
    Group {
        key: "kw-non-goals",
        context: Context::FeatureHeader,
        scope: "entity.name.function.statement.non-goals.lazuli",
    },
    Group {
        key: "kw-audit",
        context: Context::Audit,
        scope: "entity.name.function.statement.audit.lazuli",
    },
    Group {
        key: "kw-approval",
        context: Context::Approval,
        scope: "entity.name.function.statement.approval.lazuli",
    },
    Group {
        key: "kw-deprecated",
        context: Context::CommandBody,
        scope: "entity.name.function.statement.deprecated.lazuli",
    },
    Group {
        key: "kw-replay",
        context: Context::Webhook,
        scope: "entity.name.function.statement.replay.lazuli",
    },
    Group {
        key: "kw-digest",
        context: Context::Notification,
        scope: "entity.name.function.statement.digest.lazuli",
    },
    Group {
        key: "kw-throttle",
        context: Context::Notification,
        scope: "entity.name.function.statement.throttle.lazuli",
    },
    Group {
        key: "kw-plan-section",
        context: Context::Plan,
        scope: "keyword.control.section.lazuli",
    },
    Group {
        key: "kw-secret-rotation",
        context: Context::SecretRotation,
        scope: "keyword.control.statement.lazuli",
    },
];

/// Marker written into every generated `#kw-*` rule so a reader (and the
/// `--check` gate) knows it is machine-owned.
const GEN_COMMENT: &str =
    "GENERATED by `cargo xtask gen-tmlanguage` from lazuli_keywords::ALL — do not edit by hand.";

/// Build the `#kw-*` repository rules as an ordered JSON object (key → rule).
fn build_kw_rules() -> Map<String, Value> {
    // Group registry literals by (context, scope).
    let mut by_group: BTreeMap<(usize, &'static str), Vec<&'static str>> = BTreeMap::new();
    for (ctx_idx, g) in GROUPS.iter().enumerate() {
        let mut lits: Vec<&'static str> = ALL
            .iter()
            .filter(|c| c.context == g.context && c.scope == g.scope && c.sigil.is_none())
            .map(|c| c.literal)
            .collect();
        lits.sort_unstable();
        lits.dedup();
        by_group.insert((ctx_idx, g.key), lits);
    }

    let mut out = Map::new();
    for (idx, g) in GROUPS.iter().enumerate() {
        let lits = &by_group[&(idx, g.key)];
        let alternation = order_longest_first(lits);
        let escaped: Vec<String> = alternation.iter().map(|s| escape_literal(s)).collect();
        let pattern = format!("^\\s+({})\\b", escaped.join("|"));
        // Build the rule object explicitly (no `json!` macro — its expansion
        // calls `.unwrap()`, which the workspace clippy config disallows).
        // serde_json::Value::Object is a BTreeMap, so the three keys serialize
        // in stable alphabetical order (`comment`, `match`, `name`).
        let mut rule = Map::new();
        rule.insert("comment".to_string(), Value::String(GEN_COMMENT.to_string()));
        rule.insert("name".to_string(), Value::String(g.scope.to_string()));
        rule.insert("match".to_string(), Value::String(pattern));
        out.insert(g.key.to_string(), Value::Object(rule));
    }
    out
}

/// Longest-literal-first ordering so overlapping prefixes (e.g. `timestamps`
/// vs `timestamp`) never short-circuit a longer match. Ties broken
/// alphabetically for determinism.
fn order_longest_first(lits: &[&str]) -> Vec<String> {
    let mut v: Vec<String> = lits.iter().map(|s| s.to_string()).collect();
    v.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
    v
}

/// Escape a literal for safe embedding inside a JSON tmLanguage regex
/// alternation. `serde_json` handles the JSON-string escaping of backslashes;
/// here we only escape regex metacharacters that can appear in a keyword
/// literal (`.` in dotted forms like `view.board`). Identifier characters and
/// `_` need no escaping.
fn escape_literal(lit: &str) -> String {
    let mut s = String::with_capacity(lit.len());
    for ch in lit.chars() {
        match ch {
            '.' | '+' | '*' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '|' | '^' | '$' | '\\' => {
                s.push('\\');
                s.push(ch);
            }
            _ => s.push(ch),
        }
    }
    s
}

/// Render the `#kw-*` rules object exactly the way it is serialized inside the
/// committed grammar so the `--check` comparison is byte-exact: 2-space
/// indent, keys in `GROUPS` order, each rule indented to the grammar's
/// `repository`-child depth (4 spaces) — matching `serde_json` pretty output
/// re-indented by 4.
fn render_kw_section(rules: &Map<String, Value>) -> Result<String, String> {
    // Serialize each rule with serde_json's 2-space pretty printer, then
    // re-indent so it nests under `"repository": {` at the file's depth.
    // Every generated rule is followed by either another rule or the
    // `_kw_generated_end` sentinel — so each line group always ends with a
    // trailing comma (the section is spliced *between* two sentinel lines).
    let mut lines = Vec::new();
    for (key, rule) in rules.iter() {
        let body = serde_json::to_string_pretty(rule)
            .map_err(|e| format!("serializing #{key} rule: {e}"))?;
        // Re-indent every line by 4 spaces (repository-child depth).
        let mut reindented = String::new();
        for line in body.lines() {
            reindented.push_str("    ");
            reindented.push_str(line);
            reindented.push('\n');
        }
        let trimmed = reindented.trim_end_matches('\n');
        lines.push(format!("    \"{}\": {},", key, trimmed.trim_start()));
    }
    Ok(lines.join("\n"))
}

fn workspace_root() -> PathBuf {
    // CARGO_MANIFEST_DIR is .../tools/xtask; climb two levels to the root.
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop(); // xtask
    p.pop(); // tools
    p
}

/// Entry point for the `gen-tmlanguage` subcommand.
pub fn run(check: bool) -> Result<(), String> {
    let grammar_path = workspace_root().join(GRAMMAR_REL);
    let raw = std::fs::read_to_string(&grammar_path)
        .map_err(|e| format!("reading {}: {e}", grammar_path.display()))?;

    let rules = build_kw_rules();
    let fresh_section = render_kw_section(&rules)?;

    let committed_section = extract_kw_section(&raw)?;

    if check {
        if committed_section.trim_end() == fresh_section.trim_end() {
            println!(
                "gen-tmlanguage --check: #kw-* section is fresh ({} rules).",
                rules.len()
            );
            Ok(())
        } else {
            Err(format!(
                "stale #kw-* section in {} — run `cargo xtask gen-tmlanguage`.\n\
                 --- committed ---\n{}\n--- fresh ---\n{}",
                GRAMMAR_REL, committed_section, fresh_section
            ))
        }
    } else {
        if committed_section.trim_end() == fresh_section.trim_end() {
            println!(
                "gen-tmlanguage: #kw-* section already fresh ({} rules), no write.",
                rules.len()
            );
            return Ok(());
        }
        let updated = splice_kw_section(&raw, &fresh_section)?;
        std::fs::write(&grammar_path, updated)
            .map_err(|e| format!("writing {}: {e}", grammar_path.display()))?;
        println!(
            "gen-tmlanguage: wrote {} #kw-* rules to {}.",
            rules.len(),
            GRAMMAR_REL
        );
        Ok(())
    }
}

/// Sentinel keys bounding the generated region inside `repository`.
const BEGIN_MARK: &str = "    \"_kw_generated_begin\":";
const END_MARK: &str = "    \"_kw_generated_end\":";

/// Extract the committed `#kw-*` section: everything strictly between the
/// begin/end sentinel marker lines.
fn extract_kw_section(raw: &str) -> Result<String, String> {
    let begin = raw
        .find(BEGIN_MARK)
        .ok_or_else(|| format!("sentinel `{}` not found in grammar", BEGIN_MARK.trim()))?;
    let after_begin = raw[begin..]
        .find('\n')
        .map(|n| begin + n + 1)
        .ok_or("malformed begin sentinel")?;
    let end = raw
        .find(END_MARK)
        .ok_or_else(|| format!("sentinel `{}` not found in grammar", END_MARK.trim()))?;
    Ok(raw[after_begin..end].trim_end_matches('\n').to_string())
}

/// Replace the committed `#kw-*` section between the sentinels with `fresh`.
fn splice_kw_section(raw: &str, fresh: &str) -> Result<String, String> {
    let begin = raw
        .find(BEGIN_MARK)
        .ok_or_else(|| format!("sentinel `{}` not found in grammar", BEGIN_MARK.trim()))?;
    let after_begin = raw[begin..]
        .find('\n')
        .map(|n| begin + n + 1)
        .ok_or("malformed begin sentinel")?;
    let end = raw
        .find(END_MARK)
        .ok_or_else(|| format!("sentinel `{}` not found in grammar", END_MARK.trim()))?;
    let mut out = String::with_capacity(raw.len() + fresh.len());
    out.push_str(&raw[..after_begin]);
    out.push_str(fresh);
    out.push('\n');
    out.push_str(&raw[end..]);
    Ok(out)
}

/// Dump every `(surface, context, scope, sigil)` group → its literals as
/// JSON, for mapping the registry to the hand-written tmLanguage alternations.
pub fn dump_groups() {
    let mut groups: BTreeMap<String, Vec<&'static str>> = BTreeMap::new();
    for c in ALL {
        let key = format!(
            "{:?} | {:?} | {} | {:?}",
            c.surface, c.context, c.scope, c.sigil
        );
        groups.entry(key).or_default().push(c.literal);
    }
    let mut out = serde_json::Map::new();
    for (k, mut lits) in groups {
        lits.sort_unstable();
        lits.dedup();
        out.insert(
            k,
            serde_json::Value::Array(lits.into_iter().map(|s| s.into()).collect()),
        );
    }
    match serde_json::to_string_pretty(&serde_json::Value::Object(out)) {
        Ok(s) => println!("{s}"),
        Err(e) => eprintln!("dump-groups: serialization failed: {e}"),
    }
}
