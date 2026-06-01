//! `docs/lazuli_way/runtime-surface.md` generation from the runtime Go surface.
//!
//! # What this generates
//!
//! An **intent-keyed** index of every exported symbol the Lazuli runtime
//! (`runtime/go/lazuli/**/*.go`) owns. This is mechanism (ii) of the
//! reinvention defense: the building agent reimplemented argon2id because
//! `auth.HashPassword` was never in its context window. A flat alphabetical
//! symbol dump does not defeat reinvention — *"hash a password →
//! `auth.HashPassword`"* does. So the doc has two layers:
//!
//! 1. **`## Reach for this`** — a hand-curated [`INTENT_TABLE`] of the Pareto
//!    families the pilot audit flagged (auth, lifecycle, CRUD effects, roles,
//!    money, semantic scalars, …), each with a *"reach for this when you need
//!    to X"* line. This is the teaching layer.
//! 2. **`## Full symbol census`** — the machine-scanned, exhaustive per-package
//!    symbol list. This is the drift-proof exhaustiveness layer: when the
//!    runtime grows a symbol, the next regen picks it up and the freshness test
//!    fails until committed.
//!
//! # Boundary
//!
//! Like `keyword_reference`, this is a **pure projector** assembled by hand for
//! byte-exact determinism (no `serde_json` needed — the output is Markdown).
//! The runtime Go source is the single input for the census; the curated intent
//! table is a `const` deliverable validated *against* the census so a curated
//! row can never name a symbol the runtime dropped
//! ([`validate_intent_against_census`]).
//!
//! # Freshness
//!
//! `gen-runtime-surface --check` regenerates the document in memory and asserts
//! it is byte-identical to the committed file.
//! `tools/xtask/tests/runtime_surface_fresh.rs` wires this into
//! `cargo test --workspace` so a hand-edit or a forgotten regen fails loudly.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Directory (relative to the workspace root) holding the runtime Go surface.
const RUNTIME_DIR: &str = "runtime/go/lazuli";

/// Path to the committed generated index, relative to the workspace root.
const DOC_REL: &str = "docs/lazuli_way/runtime-surface.md";

/// Header banner written at the top of the generated document. Anything before
/// [`BODY_SENTINEL`] is prose the generator owns and rewrites every run, so the
/// whole file is machine-owned and the `--check` gate is total — same posture
/// as `keyword_reference::HEADER`.
const HEADER: &str = "\
<!-- GENERATED FILE — DO NOT EDIT BY HAND.
     Source of truth: runtime/go/lazuli/**/*.go (the exported Go surface).
     Regenerate with: cargo run -p xtask -- gen-runtime-surface
     Freshness is gated by tools/xtask/tests/runtime_surface_fresh.rs. -->

# Lazuli runtime surface — what the runtime already owns

The runtime owns the MECHANISM; you declare the RESOURCE. Before you write a
handler, scan this index: if the runtime already exports the verb, delegate —
do NOT reimplement. See docs/lazuli_way/delegate-to-runtime.md.

This index is generated from `runtime/go/lazuli/` and is **intent-keyed**: the
families the pilot audit flagged for reinvention (auth, lifecycle, CRUD, roles,
money, semantic scalars, …) carry a *\"reach for this when you need to X\"* line.
A flat list of symbols does not defeat reinvention; an intent-keyed one does.";

/// Sentinel marking the start of the generated body. Everything from this line
/// to EOF is rebuilt verbatim each run.
const BODY_SENTINEL: &str = "<!-- BEGIN GENERATED BODY -->";

/// One curated capability family — the deliverable shape. The `symbols` are
/// validated against the live census ([`validate_intent_against_census`]); the
/// `note` is free prose (it may reference language-level `@semantic.*` catalog
/// entries that are not Go symbols).
struct IntentFamily {
    /// Section title, e.g. `"Password hashing"`.
    title: &'static str,
    /// The *"reach for this when you need to…"* clause, e.g. `"hash or verify a
    /// user password"`.
    reach: &'static str,
    /// The runtime package the symbols live in (`"auth"`, `"lazuli"`, …).
    package: &'static str,
    /// The exported symbols, bare (no package prefix) — each MUST exist in the
    /// census for `package`.
    symbols: &'static [&'static str],
    /// One-line gloss rendered as a blockquote under the family.
    note: &'static str,
}

/// The curated Pareto families — the priority families the pilot audit flagged.
/// Every symbol below is validated present in the scanned census before the doc
/// renders ([`validate_intent_against_census`]); a curated row naming a vanished
/// symbol fails the generator (and thus the freshness test). This `const` is the
/// extension point: a new family is one more row, no `render` change.
const INTENT_TABLE: &[IntentFamily] = &[
    IntentFamily {
        title: "Password hashing",
        reach: "hash or verify a user password",
        package: "auth",
        symbols: &[
            "HashPassword",
            "VerifyPassword",
            "HashWithArgon2",
            "Argon2Params",
            "SetArgon2Concurrency",
        ],
        note: "argon2id + concurrency cap + tuning are runtime-owned. Prefer dropping \
               `@fn.hash_password` and letting `@cap.Hashed(algorithm:argon2id)` auto-wire. \
               **Never import `golang.org/x/crypto/argon2`.**",
    },
    IntentFamily {
        title: "Sessions",
        reach: "mint, hash, issue, rotate, or invalidate a session",
        package: "auth",
        symbols: &[
            "MintSessionToken",
            "HashSessionToken",
            "IssueSession",
            "RotateSession",
            "ResolveSession",
            "InvalidateSession",
            "InvalidateSessionByID",
        ],
        note: "token + hash stay in lockstep with the runtime session resolver. Don't \
               hand-roll opaque-token mint/hash with `crypto/rand` + `crypto/sha256`.",
    },
    IntentFamily {
        title: "Password reset / email verification",
        reach: "issue or consume a reset/verify token",
        package: "auth",
        symbols: &[
            "RequestPasswordReset",
            "ConfirmPasswordReset",
            "PasswordResetToken",
            "PasswordResetContract",
            "IssueEmailVerificationToken",
            "VerifyEmailToken",
            "EmailVerificationToken",
            "EmailVerificationContract",
        ],
        note: "format + hashing + TTL + single-use consume come from the runtime against \
               the declared contract. No manual `SELECT … used_at … expiry` dance.",
    },
    IntentFamily {
        title: "JWT / OAuth / MFA",
        reach: "sign/verify a JWT, run an OAuth leg, enroll/verify MFA",
        package: "auth",
        symbols: &[
            "SignJWT",
            "VerifyJWT",
            "Claims",
            "BuildOAuthConfig",
            "GoogleOAuthRedirectURL",
            "GoogleOAuthCallback",
            "EnrollMFA",
            "VerifyMFA",
        ],
        note: "the auth leg is wired; you supply the binding, not the crypto.",
    },
    IntentFamily {
        title: "Roles",
        reach: "check the actor's role",
        package: "lazuli",
        symbols: &["HasRole", "RequireRole", "RequireActor"],
        note: "inline `lazuli.HasRole(ctx,\"ADMIN\")` — never an app-local `actorHasRole` helper.",
    },
    IntentFamily {
        title: "Money",
        reach: "construct or parse a money value",
        package: "lazuli",
        symbols: &["BRL", "USD", "EUR", "MoneyValue", "ParseMoneyLiteral"],
        note: "currency-aware money is a runtime type; see `docs/lazuli_way/money.md`.",
    },
    IntentFamily {
        title: "Rate limit",
        reach: "parse / apply a rate-limit spec",
        package: "lazuli",
        symbols: &[
            "ParseRateLimit",
            "RateLimit",
            "RateLimitFromDefault",
            "RateLimitByEnv",
            "RateLimitMiddleware",
        ],
        note: "declare `rate_limit \"<spec>\"`; the runtime parses + enforces.",
    },
    IntentFamily {
        title: "Lifecycle / state transitions",
        reach: "advance a resource's status through a state machine",
        package: "lazuli",
        symbols: &[
            "TransitionAdvance",
            "Transition",
            "LifecycleStateMismatchError",
        ],
        note: "declare a `lifecycle <field>` + `transition`; the runtime enforces the \
               from-state set and emits the mismatch error. Never hand-roll \
               `UPDATE … status IN (…) … RowsAffected == 0`. The generic FSM lives in \
               package `lifecycle` (`lifecycle.Machine` / `lifecycle.New` / \
               `lifecycle.Transition`). See `docs/lazuli_way/state-machines.md`.",
    },
    IntentFamily {
        title: "CRUD effects",
        reach: "create / update / delete / reorder a resource",
        package: "lazuli",
        symbols: &[
            "Creates",
            "CreatesEffect",
            "CreatesWithOwnerCheck",
            "Updates",
            "UpdatesEffect",
            "Deletes",
            "DeletesEffect",
            "Reorder",
            "ReorderEffect",
            "NewUpdate",
            "PartialUpdate",
            "UpdateBuilder",
            "OwnedByActor",
        ],
        note: "declare `creates`/`updates`/`deletes`; the runtime applies defaults, \
               `ctx.now`, actor-owner scoping. No raw `db.Exec` INSERT with literal \
               defaults; no hand-built partial-UPDATE placeholder bookkeeping. See \
               `docs/lazuli_way/crud-by-convention.md`.",
    },
    IntentFamily {
        title: "Semantic scalars",
        reach: "validate a hex color / percentage / email at the boundary",
        package: "lazuli",
        symbols: &["HexColor", "Percentage"],
        note: "declare the field as the scalar (`color: @semantic.HexColor`) from the \
               closed `@semantic.*` catalog (`@semantic.HexColor`, `@semantic.Percentage`, \
               `@semantic.Email`, `@semantic.Money`); the type validates at decode. Never a \
               regex `^#?[0-9A-Fa-f]{6}$` validator.",
    },
    IntentFamily {
        title: "Notifications",
        reach: "send a notification / digest",
        package: "notifications",
        symbols: &[
            "Send",
            "NewRegistry",
            "NotificationContract",
            "NewSMTPDispatcher",
        ],
        note: "the channel/digest/throttle machinery is runtime-owned.",
    },
    IntentFamily {
        title: "Storage / signed URLs",
        reach: "issue a signed upload/download URL",
        package: "storage",
        symbols: &[
            "IssueSignedURL",
            "IssueSignedUploadURL",
            "NewS3Store",
            "ObjectStore",
        ],
        note: "declare `@cap.File`; the runtime owns presigning.",
    },
    IntentFamily {
        title: "Encryption",
        reach: "encrypt/decrypt a field at rest",
        package: "encryption",
        symbols: &["NewCipher", "For", "ForCtx", "Register"],
        note: "declare `@cap.Encrypted(key:@key.*)`; the cipher + key rotation are \
               runtime-owned (maps geocoding is `lazuli.Geocoder`).",
    },
];

/// One package's scanned, sorted, de-duplicated exported symbols.
#[derive(Default)]
struct PackageSurface {
    symbols: Vec<String>,
}

/// Recursively collect `*.go` files under `dir`, skipping `*_test.go` and any
/// `internal/` subtree (those are not the public package surface).
fn collect_go(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            if p.file_name().and_then(|n| n.to_str()) == Some("internal") {
                continue;
            }
            collect_go(&p, out);
        } else {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name.ends_with(".go") && !name.ends_with("_test.go") {
                out.push(p);
            }
        }
    }
}

/// Extract the exported symbol from a top-level Go declaration line, if any.
/// A symbol is exported when the line matches `^func [A-Z]\w*` (a top-level
/// func; the method form `^func (recv) Name` has a `(` after `func ` and is NOT
/// a package export, so it is skipped) or `^type [A-Z]\w*`. The captured name is
/// the run of `[A-Za-z0-9_]` after the keyword (stops at `(`, `[`, ` `, etc.),
/// so generic decls like `type Machine[E ~string]` yield `Machine`.
fn exported_symbol(line: &str) -> Option<String> {
    let rest = if let Some(r) = line.strip_prefix("func ") {
        // Method receivers (`func (r Recv) Name`) start with `(` — not a package
        // export.
        if r.starts_with('(') {
            return None;
        }
        r
    } else if let Some(r) = line.strip_prefix("type ") {
        r
    } else {
        return None;
    };
    let mut chars = rest.chars();
    let first = chars.next()?;
    if !first.is_ascii_uppercase() {
        return None;
    }
    let mut name = String::new();
    name.push(first);
    for c in chars {
        if c.is_ascii_alphanumeric() || c == '_' {
            name.push(c);
        } else {
            break;
        }
    }
    Some(name)
}

/// The package label for a `.go` file = its leaf directory under `RUNTIME_DIR`.
/// Files directly under `runtime/go/lazuli/` → package `lazuli`; a file under
/// `auth/` → `auth`. This matches the package layout the census proves, NOT the
/// Go `package` declaration (root files declare `package lazuli` already, but
/// the partition is by directory so it is robust to internal package renames).
fn package_for(path: &Path, root: &Path) -> String {
    let parent = path.parent().unwrap_or(root);
    if parent == root {
        return "lazuli".to_string();
    }
    parent
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("lazuli")
        .to_string()
}

/// Walk `root` (`runtime/go/lazuli`) and project the exported surface to a
/// per-package, sorted, de-duplicated map. Skips `*_test.go` + `internal/`.
fn scan_surface(root: &Path) -> BTreeMap<String, PackageSurface> {
    let mut files = Vec::new();
    collect_go(root, &mut files);

    let mut by_pkg: BTreeMap<String, PackageSurface> = BTreeMap::new();
    for file in &files {
        let pkg = package_for(file, root);
        let text = std::fs::read_to_string(file).unwrap_or_default();
        let entry = by_pkg.entry(pkg).or_default();
        for line in text.lines() {
            if let Some(sym) = exported_symbol(line) {
                entry.symbols.push(sym);
            }
        }
    }
    for surface in by_pkg.values_mut() {
        surface.symbols.sort();
        surface.symbols.dedup();
    }
    by_pkg
}

/// One-line gloss per package for the census section. A package without an
/// explicit gloss falls through to a generic line — still deterministic, never
/// a build break when the runtime grows a package.
fn package_gloss(pkg: &str) -> &'static str {
    match pkg {
        "audit" => "audit-trail sinks + rows.",
        "auth" => "password/session/reset/JWT/OAuth/MFA — the auth mechanism (see intent section).",
        "billing" => "plan catalog, quotas, feature gates, subscriptions.",
        "breach" => "breached-credential checking.",
        "cache" => "query cache: tags, invalidation, Redis backend.",
        "captcha" => "captcha verification.",
        "email" => "SMTP / Sendgrid mail adapters.",
        "encryption" => "field-at-rest cipher + key rotation (see intent section).",
        "events" => "event dispatch, outbox/inbox dedup pump.",
        "i18n" => "locale negotiation, catalogs, error-message resolution.",
        "jobs" => "background jobs: dispatch, retries, idempotency (River).",
        "lifecycle" => "generic state-machine FSM (`Machine`/`New`/`Transition`).",
        "maps" => "geocoding request/response types.",
        "mcp" => "Model-Context-Protocol server/client/tool/resource registration.",
        "migrations" => "tenant-aware migration planning + deploy strategy.",
        "mtls" => "mutual-TLS identity verification.",
        "notifications" => "channels, digests, throttles, SMTP dispatch (see intent section).",
        "observability" => "logging, tracing, audit records, health/readiness probes.",
        "payments" => "payment gateway + webhook event types.",
        "plangate" => "plan-gate kinds + refs.",
        "poller" => "external-status polling: schedulers, predicates, backoff.",
        "probe" => "liveness / readiness probe constructors.",
        "report" => "CSV/XLSX report runners, columns, params, policy.",
        "reputation" => "reputation scoring.",
        "runtime" => "codegen-emitted glue sentinels + constructors (referential-guard \
                      `ErrReferencedInUse` / `NewReferencedInUseError`).",
        "secrets" => "secret providers + leased secrets.",
        "storage" => "object store + signed URLs (see intent section).",
        "vectorstore" => "vector store: collections, embeddings, similarity queries.",
        "waf" => "web-application-firewall filters + decisions.",
        "webhooks" => "inbound/outbound webhooks: routing, HMAC verify, replay, DLQ.",
        "lazuli" => "the core runtime package: ctx, db, effects, policy, roles, money, \
                     rate-limit, lifecycle, semantic scalars (see intent section).",
        _ => "runtime package.",
    }
}

/// Every `package.Symbol` named in `table` must appear in `scan`. A curated row
/// naming a vanished symbol fails here, keeping the intent layer honest as the
/// runtime evolves — the same const-table-is-the-extension-point discipline as
/// the doctor `REINVENTION_TABLE`.
fn validate_intent_against_census(
    table: &[IntentFamily],
    scan: &BTreeMap<String, PackageSurface>,
) -> Result<(), String> {
    for family in table {
        let pkg = scan.get(family.package).ok_or_else(|| {
            format!(
                "intent family `{}` names package `{}`, absent from the runtime census",
                family.title, family.package
            )
        })?;
        for sym in family.symbols {
            if !pkg.symbols.iter().any(|s| s == sym) {
                return Err(format!(
                    "intent family `{}` names `{}.{}`, absent from the runtime census — \
                     a curated row may not name a symbol the runtime dropped",
                    family.title, family.package, sym
                ));
            }
        }
    }
    Ok(())
}

/// Build the full generated body (everything from [`BODY_SENTINEL`] to EOF),
/// returning the document text and the total scanned-symbol count.
fn render(scan: &BTreeMap<String, PackageSurface>, table: &[IntentFamily]) -> (String, usize) {
    let total: usize = scan.values().map(|p| p.symbols.len()).sum();
    let pkg_count = scan.len();

    let mut out = String::new();
    out.push_str(BODY_SENTINEL);
    out.push_str("\n\n");

    // --- Intent section (the teaching layer, driven by the table). ---
    out.push_str("## Reach for this (intent-keyed)\n");
    out.push_str(
        "\nThe families the pilot audit flagged for reinvention. \
         Reach for the runtime verb; do not hand-roll it.\n",
    );
    for family in table {
        out.push_str(&format!("\n### {}\n", family.title));
        out.push_str(&format!(
            "**Reach for this when you need to:** {}.\n\n",
            family.reach
        ));
        let qualified: Vec<String> = family
            .symbols
            .iter()
            .map(|s| format!("`{}.{}`", family.package, s))
            .collect();
        out.push_str(&qualified.join(" · "));
        out.push('\n');
        out.push_str(&format!("\n> {}\n", family.note));
    }

    // --- Census section (the exhaustiveness layer, driven by the scan). ---
    out.push_str("\n## Full symbol census\n");
    out.push_str(&format!(
        "\n_Generated from {total} exported symbols across {pkg_count} runtime packages._\n"
    ));
    for (pkg, surface) in scan {
        out.push_str(&format!("\n### {pkg}\n"));
        out.push_str(&format!("> {}\n\n", package_gloss(pkg)));
        if surface.symbols.is_empty() {
            out.push_str("_(no exported symbols)_\n");
            continue;
        }
        let cells: Vec<String> = surface
            .symbols
            .iter()
            .map(|s| format!("`{s}`"))
            .collect();
        out.push_str(&cells.join(" · "));
        out.push('\n');
    }

    (out, total)
}

/// Assemble the full document text: header + generated body.
fn build_document() -> Result<(String, usize), String> {
    let root = workspace_root().join(RUNTIME_DIR);
    let scan = scan_surface(&root);
    if scan.is_empty() {
        return Err(format!(
            "no runtime packages scanned under {} — is the runtime present?",
            root.display()
        ));
    }
    validate_intent_against_census(INTENT_TABLE, &scan)?;

    let (body, total) = render(&scan, INTENT_TABLE);
    let mut doc = String::new();
    doc.push_str(HEADER);
    doc.push_str("\n\n");
    doc.push_str(&body);
    // Normalize to a single trailing newline.
    let trimmed = doc.trim_end();
    Ok((format!("{trimmed}\n"), total))
}

fn workspace_root() -> PathBuf {
    // CARGO_MANIFEST_DIR is .../tools/xtask; climb two levels to the root.
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop(); // xtask
    p.pop(); // tools
    p
}

/// Entry point for the `gen-runtime-surface` subcommand. IDENTICAL control flow
/// to `keyword_reference::run`.
pub fn run(check: bool) -> Result<(), String> {
    let path = workspace_root().join(DOC_REL);
    let (fresh, total) = build_document()?;

    let committed = std::fs::read_to_string(&path).unwrap_or_default();
    // Compare on normalized line endings so a CRLF checkout doesn't spuriously
    // diff against the LF the generator emits.
    let same = normalize(&committed) == normalize(&fresh);

    if check {
        if same {
            println!("gen-runtime-surface --check: {DOC_REL} is fresh ({total} symbols).");
            Ok(())
        } else {
            Err(format!(
                "stale {DOC_REL} — run `cargo run -p xtask -- gen-runtime-surface`."
            ))
        }
    } else if same {
        println!("gen-runtime-surface: {DOC_REL} already fresh ({total} symbols), no write.");
        Ok(())
    } else {
        std::fs::write(&path, &fresh).map_err(|e| format!("writing {}: {e}", path.display()))?;
        println!("gen-runtime-surface: wrote {total} symbols to {DOC_REL}.");
        Ok(())
    }
}

/// Normalize line endings for a byte-exact-modulo-EOL comparison.
fn normalize(s: &str) -> String {
    s.replace("\r\n", "\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn census() -> BTreeMap<String, PackageSurface> {
        scan_surface(&workspace_root().join(RUNTIME_DIR))
    }

    #[test]
    fn census_partitions_by_package() {
        let scan = census();
        let has = |pkg: &str, sym: &str| {
            scan.get(pkg)
                .is_some_and(|p| p.symbols.iter().any(|s| s == sym))
        };
        // Known-good partition from runtime-surface.md.
        assert!(has("auth", "HashPassword"), "auth.HashPassword missing");
        assert!(
            has("lazuli", "TransitionAdvance"),
            "lazuli.TransitionAdvance missing"
        );
        assert!(has("lifecycle", "Machine"), "lifecycle.Machine missing");
        assert!(has("lazuli", "HexColor"), "lazuli.HexColor missing");

        // `*_test.go` symbols are skipped: no Go test funcs leak in.
        let leaked_test_sym = scan
            .values()
            .flat_map(|p| p.symbols.iter())
            .any(|s| s.starts_with("Test") && s.len() > 4 && s != "TestMime");
        assert!(
            !leaked_test_sym,
            "a Go `Test…` function leaked into the census (a *_test.go was scanned)"
        );

        // The `internal/` subtree is excluded: its leaf dir never becomes a package.
        assert!(
            !scan.contains_key("internal") && !scan.contains_key("wiresmoke"),
            "the internal/ subtree must be excluded from the census"
        );
    }

    #[test]
    fn intent_symbols_exist_in_census() {
        let scan = census();
        validate_intent_against_census(INTENT_TABLE, &scan)
            .expect("every INTENT_TABLE symbol must exist in the scanned census");
    }

    #[test]
    fn intent_table_is_extensible() {
        let scan = census();
        // Adding a family row that names a real scanned symbol renders + validates
        // without touching render's body — proves the parameterization.
        let fixture = [IntentFamily {
            title: "Fixture family",
            reach: "prove the table parameterizes",
            package: "auth",
            symbols: &["HashPassword"],
            note: "fixture row.",
        }];
        validate_intent_against_census(&fixture, &scan)
            .expect("a fixture family naming a real symbol must validate");
        let (body, _) = render(&scan, &fixture);
        assert!(
            body.contains("### Fixture family"),
            "render must surface a table-driven family with no body change"
        );
    }

    #[test]
    fn method_receivers_are_not_exports() {
        // `func (t TransitionAdvance) froms()` is a method, not a package export.
        assert_eq!(exported_symbol("func (t TransitionAdvance) froms() []string {"), None);
        assert_eq!(exported_symbol("func HashPassword(ctx *Ctx) {"), Some("HashPassword".into()));
        assert_eq!(exported_symbol("type Machine[E ~string] struct {"), Some("Machine".into()));
        assert_eq!(exported_symbol("func unexported() {"), None);
        assert_eq!(exported_symbol("type lowercase struct {"), None);
    }
}
