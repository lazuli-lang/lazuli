//! tmLanguage keyword-rule generation from `lazuli_keywords::ALL`.
//!
//! # What this generates
//!
//! The **keyword-alternation repository rules** (`#kw-*`) of the VS Code
//! grammar — one repo rule per `(Context, scope, sigil-shape)` group in the
//! [`GROUPS`] allowlist. Each rule's match is `^\s+(<literals, longest-first>)\b`
//! and its `name` is the group's TextMate scope leaf. Because a literal valid
//! in N contexts is N rows in the registry (context-as-data), grouping by
//! `(Context, scope)` reproduces the grammar's per-block scope leaves exactly.
//!
//! Two tiers (see [`GROUPS`]):
//!
//! 1. **Wired** rules an `include` in a hand-written `begin/end` block points
//!    at (`kw-cookie`, `kw-audit`, …) — editing the registry widens the live
//!    highlight.
//! 2. **Fallback-coverage** rules (WT-3) that back the curated multi-group
//!    fallback alternations (`command`/`query`/`view*`/`agent`/… bodies). These
//!    are GENERATED so every registry keyword is a literal substring of the
//!    grammar (the `keyword_surface_parity` highlight half is drift-proof by
//!    construction — this retired the 44-entry `HIGHLIGHT_SURFACE_GAP`) but are
//!    intentionally left UN-`include`d, so they change the generation source,
//!    not a rendered token (snapshots stay byte-for-byte green).
//!
//! # What it does NOT generate (stays hand-written, structural)
//!
//! * block `begin`/`end` rules, entity-name captures, the top-level
//!   `patterns` include list, and the `{ "include": "#kw-*" }` references
//!   (which block points at which generated rule is a hand-written decision);
//! * strings, comments, operators, punctuation, `#references`, `#types`,
//!   `#decorators`, `#modifiers`, `#constants` (cross-cutting / regex-shaped);
//! * the `locale_negotiate` sub-block alternations (`source`/`strategy` are
//!   closed-catalog rules, not a single-group keyword projection) and the
//!   value-catalog alternations. (The app-level `locale` block statement
//!   alternation — `default`/`supported`/`fallback` — IS generated as
//!   `#kw-locale`.)
//!
//! # Freshness
//!
//! `gen-tmlanguage --check` regenerates the `#kw-*` section in memory and
//! asserts it is byte-identical to the committed grammar. CI / the
//! `editors/vscode` grammar test wires this so a hand-edit or a forgotten
//! regen fails loudly.

use std::collections::BTreeMap;
use std::path::PathBuf;

use lazuli_keywords::{ALL, Context, Sigil};
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
    /// Which sigil-shape of row this group projects. The registry stores a
    /// dotted-kind declaration (`query.view`, `event.trace`) with
    /// `Sigil::DottedKind` and a bare keyword with `None`; both can share the
    /// same `(context, scope)`, so the selector lets one `(context, scope)`
    /// pair split into a bare alternation and a dotted alternation under
    /// separate `#kw-*` keys. (`@`-decorators are never projected here — they
    /// are matched by the cross-cutting `#decorators` rule.)
    sigil: SigilSel,
}

/// Which sigil-shape of registry row a [`Group`] projects.
#[derive(Clone, Copy, PartialEq, Eq)]
enum SigilSel {
    /// Bare keyword literals (`c.sigil == None`) — the default for every
    /// per-block statement/section alternation.
    Bare,
    /// Dotted-kind declarations (`c.sigil == Some(Sigil::DottedKind)`), e.g.
    /// `query.view` / `event.trace`.
    Dotted,
}

impl SigilSel {
    /// Whether a registry row's sigil matches this selector.
    fn matches(self, sigil: Option<Sigil>) -> bool {
        match self {
            SigilSel::Bare => sigil.is_none(),
            SigilSel::Dotted => sigil == Some(Sigil::DottedKind),
        }
    }
}

/// Shorthand for a bare-keyword group (the common case).
const fn bare(key: &'static str, context: Context, scope: &'static str) -> Group {
    Group {
        key,
        context,
        scope,
        sigil: SigilSel::Bare,
    }
}

/// Shorthand for a dotted-kind group (`query.*` / `event.trace`).
const fn dotted_group(key: &'static str, context: Context, scope: &'static str) -> Group {
    Group {
        key,
        context,
        scope,
        sigil: SigilSel::Dotted,
    }
}

/// The `(Context, scope)` groups whose alternation `gen-tmlanguage` projects
/// into `#kw-*` repository rules. The `#kw-*` keys are emitted in
/// alphabetical order (the rule map is a `BTreeMap`), so this array's order
/// only disambiguates same-key collisions — declare them in any sensible
/// grouping.
///
/// # Two tiers of generated group
///
/// 1. **Wired alternations** (the original H2 set) — the per-block statement
///    leaves whose `#kw-*` rule is `include`d by a hand-written block
///    `begin/end` (`kw-cookie`, `kw-audit`, `kw-deprecated`, …). Editing the
///    registry widens the live highlight.
///
/// 2. **Fallback-coverage alternations** (WT-3) — the groups that back the
///    curated *multi-group fallback* alternations (`command`/`query`/`view*`/
///    `agent`/… bodies) which `gen-tmlanguage` deliberately does NOT wire as a
///    single `begin/end` include. These `#kw-*` rules are GENERATED (so every
///    registry keyword is a literal substring of the grammar — the
///    `keyword_surface_parity` highlight half is satisfied by construction and
///    drift-proof) but are intentionally left UN-`include`d: the editor color
///    still comes from the existing hand-written fallback rules, so generating
///    them changes the *source of truth*, not a single rendered token (the
///    grammar snapshots stay byte-for-byte green). They retire the
///    `HIGHLIGHT_SURFACE_GAP` allowlist. To promote one to a live wired rule,
///    add an `{ "include": "#kw-<key>" }` to the relevant block and re-snapshot.
///
/// Still NOT generated at all: the `locale_negotiate` sub-block
/// (`source`/`strategy` are closed-catalog rules, not a single-group keyword
/// projection — the app-level `locale` block IS generated as `#kw-locale`);
/// `@`-decorators and value catalogs (regex-shaped / cross-cutting, matched by
/// `#decorators` / `#constants`).
const GROUPS: &[Group] = &[
    // ── tier 1: wired per-block statement/section alternations (H2) ──
    bare(
        "kw-cookie",
        Context::Cookie,
        "entity.name.function.statement.cookie.lazuli",
    ),
    bare(
        "kw-headers",
        Context::Headers,
        "entity.name.function.statement.headers.lazuli",
    ),
    bare(
        "kw-proxy",
        Context::Proxy,
        "entity.name.function.statement.proxy.lazuli",
    ),
    bare(
        "kw-limits",
        Context::Limits,
        "entity.name.function.statement.limits.lazuli",
    ),
    bare(
        "kw-encryption",
        Context::Encryption,
        "entity.name.function.statement.encryption.lazuli",
    ),
    bare(
        "kw-logging",
        Context::Logging,
        "entity.name.function.statement.logging.lazuli",
    ),
    bare(
        "kw-tracing",
        Context::Tracing,
        "entity.name.function.statement.tracing.lazuli",
    ),
    bare(
        "kw-runtime",
        Context::Runtime,
        "entity.name.function.statement.runtime.lazuli",
    ),
    bare(
        "kw-deploy",
        Context::Deploy,
        "entity.name.function.statement.deploy.lazuli",
    ),
    bare(
        "kw-services",
        Context::Services,
        "entity.name.function.statement.services.lazuli",
    ),
    bare(
        "kw-communication",
        Context::Communication,
        "entity.name.function.statement.communication.lazuli",
    ),
    bare(
        "kw-env",
        Context::Env,
        "entity.name.function.statement.env.lazuli",
    ),
    bare(
        "kw-integrations",
        Context::Integrations,
        "entity.name.function.statement.integration.lazuli",
    ),
    bare(
        "kw-packs",
        Context::Packs,
        "entity.name.function.statement.packs.lazuli",
    ),
    bare(
        "kw-cache",
        Context::Cache,
        "entity.name.function.statement.cache.lazuli",
    ),
    bare(
        "kw-translation",
        Context::Translation,
        "entity.name.function.statement.translation.lazuli",
    ),
    bare(
        "kw-locale",
        Context::Locale,
        "entity.name.function.statement.locale.lazuli",
    ),
    bare(
        "kw-tests",
        Context::Tests,
        "entity.name.function.statement.tests.lazuli",
    ),
    bare(
        "kw-defaults",
        Context::Defaults,
        "entity.name.function.statement.defaults.lazuli",
    ),
    bare(
        "kw-non-goals",
        Context::FeatureHeader,
        "entity.name.function.statement.non-goals.lazuli",
    ),
    bare(
        "kw-audit",
        Context::Audit,
        "entity.name.function.statement.audit.lazuli",
    ),
    bare(
        "kw-approval",
        Context::Approval,
        "entity.name.function.statement.approval.lazuli",
    ),
    bare(
        "kw-deprecated",
        Context::Deprecated,
        "entity.name.function.statement.deprecated.lazuli",
    ),
    bare(
        "kw-replay",
        Context::Webhook,
        "entity.name.function.statement.replay.lazuli",
    ),
    bare(
        "kw-digest",
        Context::Notification,
        "entity.name.function.statement.digest.lazuli",
    ),
    bare(
        "kw-throttle",
        Context::Notification,
        "entity.name.function.statement.throttle.lazuli",
    ),
    bare(
        "kw-plan-section",
        Context::Plan,
        "keyword.control.section.lazuli",
    ),
    bare(
        "kw-secret-rotation",
        Context::SecretRotation,
        "keyword.control.statement.lazuli",
    ),
    // ── tier 2: fallback-coverage alternations (WT-3 — generated, un-wired) ──
    // These retire the 44-entry `HIGHLIGHT_SURFACE_GAP` allowlist in
    // `crates/lazuli_lsp/tests/keyword_surface_parity.rs`: each group's
    // literals become substrings of the grammar so the parity highlight half is
    // satisfied by construction. (Decl-head / section groups span more than the
    // gap literals — every member is already highlighted by its hand-written
    // block opener, so projecting the whole group is snapshot-neutral.)
    bare(
        "kw-design-section",
        Context::Design,
        "keyword.control.section.lazuli",
    ),
    bare(
        "kw-surface-view-stmt",
        Context::SurfaceView,
        "keyword.control.statement.lazuli",
    ),
    bare(
        "kw-surface-view-decl",
        Context::SurfaceView,
        "keyword.control.declaration.structural.lazuli",
    ),
    bare(
        "kw-auth-stmt",
        Context::Auth,
        "keyword.control.statement.lazuli",
    ),
    bare(
        "kw-app-stmt",
        Context::App,
        "keyword.control.statement.lazuli",
    ),
    bare(
        "kw-app-section",
        Context::App,
        "keyword.control.section.lazuli",
    ),
    bare(
        "kw-top-decl",
        Context::TopLevel,
        "keyword.control.declaration.structural.lazuli",
    ),
    bare(
        "kw-top-stmt",
        Context::TopLevel,
        "keyword.control.statement.lazuli",
    ),
    bare(
        "kw-feature-decl",
        Context::FeatureHeader,
        "keyword.control.declaration.structural.lazuli",
    ),
    bare(
        "kw-feature-section",
        Context::FeatureHeader,
        "keyword.control.section.lazuli",
    ),
    dotted_group(
        "kw-feature-dotted",
        Context::FeatureHeader,
        "keyword.control.declaration.structural.lazuli",
    ),
    bare(
        "kw-job-stmt",
        Context::Job,
        "keyword.control.statement.lazuli",
    ),
    bare(
        "kw-channel-stmt",
        Context::Channel,
        "keyword.control.statement.lazuli",
    ),
    bare(
        "kw-webhook-stmt",
        Context::Webhook,
        "keyword.control.statement.lazuli",
    ),
    bare(
        "kw-tenant-migration-stmt",
        Context::TenantMigration,
        "keyword.control.statement.lazuli",
    ),
    bare(
        "kw-resource-stmt",
        Context::ResourceBody,
        "entity.name.function.statement.resource.lazuli",
    ),
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
            .filter(|c| c.context == g.context && c.scope == g.scope && g.sigil.matches(c.sigil))
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
        rule.insert(
            "comment".to_string(),
            Value::String(GEN_COMMENT.to_string()),
        );
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
