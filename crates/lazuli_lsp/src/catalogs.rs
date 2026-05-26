//! Closed-catalog declarations surfaced by the LSP.
//!
//! ## Why catalogs live here (and not in `lazuli_ir`)
//!
//! Some of these catalogs **also** appear in `lazuli_ir` as enum variants
//! (e.g. `RATE_LIMIT_ENV_CATALOG` mirrors `lazuli_ir::EnvName`,
//! `AUTH_REFRESH_THEFT_ACTION_VALUES` mirrors the corresponding IR enum).
//! That redundancy is intentional: the LSP needs the closed-value list as
//! a flat `&[&str]` to feed completion items and hover details without
//! reflecting over enum variants at runtime, while the IR carries the
//! typed variant for codegen and doctor.
//!
//! ## Rule Zero
//!
//! Growth here is gated on a proposal. Adding a new code or value
//! requires a matching IR variant (where one exists), a doctor diagnostic
//! that enforces the closed set, and a runtime emitter (for error
//! catalogs). The LSP-side flat list is the **last** thing to grow, not
//! the first.
//!
//! ## ABI guarantee
//!
//! All catalogs are re-exported from the crate root via `pub use
//! catalogs::*;` so external consumers (Hostpoint VSCode extension,
//! `lazuli_cli::doctor`) keep importing them from the same path
//! (`lazuli_lsp::ERROR_VOCAB_CODES`, etc.).

/// Closed env catalog for `rate_limit "..." in <env>` qualifications.
/// Mirrors `lazuli_ir::EnvName` (5-entry closed set per
/// `docs/proposals/ir-rate-limit-env-aware.md` §4.3). Adding a name
/// here without the matching IR variant + proposal is incorrect.
pub const RATE_LIMIT_ENV_CATALOG: &[&str] = &["production", "staging", "test", "dev", "local"];

/// Roadmap §1.5 (CL.C.2) — closed-catalog values for the `lock`
/// strategy keyword. Surfaced as `VALUE` completions so authors and
/// LLMs see the canonical set when typing inside `lock`.
pub const RESOURCE_LOCK_STRATEGY_VALUES: &[&str] = &["optimistic", "pessimistic", "row_level"];

/// Phase L — closed-catalog tokens offered as completion values for
/// `auth` subblock slots. The slot context is not inferred today; the
/// LSP simply offers these as `VALUE` completions so authors and LLMs
/// see the canonical set alongside keyword completions.
///
/// - `algorithm` → `argon2id`, `bcrypt`
/// - `oauth <provider>` → `google`, `github`, `microsoft`, `apple`
/// - `mfa <method>` → `totp`
/// - `refresh` → `true`, `false`
pub const AUTH_CATALOG_VALUES: &[&str] = &[
    "argon2id",
    "bcrypt",
    "google",
    "github",
    "microsoft",
    "apple",
    "totp",
    "true",
    "false",
];

/// Canonical duration-literal completions surfaced after `access_ttl `
/// inside an `auth.sessions` block. Soft suggestions (the parser
/// accepts any well-formed duration literal); these match the
/// hostpoint baseline and proposal `docs/proposals/auth-refresh-rotation.md`.
pub const AUTH_REFRESH_ACCESS_DURATION_LITERALS: &[&str] = &["\"15 minutes\"", "\"1 hour\""];

/// Canonical duration-literal completions surfaced after `refresh_ttl `
/// inside an `auth.sessions.rotation` block. Soft suggestions.
pub const AUTH_REFRESH_REFRESH_DURATION_LITERALS: &[&str] = &["\"30 days\"", "\"7 days\""];

/// Canonical duration-literal completions surfaced after `grace `
/// inside an `auth.sessions.rotation` block. Soft suggestions.
pub const AUTH_REFRESH_GRACE_DURATION_LITERALS: &[&str] = &["\"30 seconds\"", "\"5 minutes\""];

/// Closed catalog of `theft_detection_action` values, mirroring the
/// corresponding IR enum. See [`auth_refresh_theft_action_detail`] for
/// per-value hover copy.
pub const AUTH_REFRESH_THEFT_ACTION_VALUES: &[&str] = &["revoke_session_family", "revoke_user"];

/// Closed catalog of HTTP status codes that may carry an authored
/// `error_page <status>` block. The list pins the user-meaningful
/// statuses; the runtime renders a default error page for anything not
/// authored. See [`error_page_status_detail`] for hover copy.
pub const ERROR_PAGE_STATUS_VALUES: &[&str] = &[
    "400", "401", "403", "404", "405", "410", "422", "429", "500", "502", "503", "504",
];

/// Closed catalog of `error_page audience <kind>` values. Mirrors the
/// IR enum and gates which signed-in cohort the error page applies to.
pub const ERROR_PAGE_AUDIENCE_VALUES: &[&str] = &["public", "authenticated", "admin"];

/// Observability bucket cycle row 36 — closed-catalog values offered
/// as `VALUE` completions for the new `app.logging` / `app.tracing`
/// slots. Same shape and dispatch as `AUTH_CATALOG_VALUES`.
///
/// - `level` → `debug`, `info`, `warn`, `error`
/// - `format` → `json`, `text`
/// - `redact` → `pii`, `none`
/// - `propagate` (tracing) → `true`, `false`
pub const OBSERVABILITY_CATALOG_VALUES: &[&str] = &[
    "debug", "info", "warn", "error", "json", "text", "pii", "none",
];

/// Migrations bucket cycle Route C — closed `deploy.strategy` catalog.
/// Three rollout patterns the language fixes so doctor can pin a
/// finite ruleset (`DEPLOY-STRATEGY-001`).
pub const DEPLOY_STRATEGY_VALUES: &[&str] = &["rolling", "blue_green", "canary"];

/// i18n bucket cycle — closed `locale_negotiate.source` catalog.
/// Five request axes the runtime can read to populate `ctx.locale`.
/// Doctor `locale_negotiate_source_invalid` enforces this set.
pub const LOCALE_NEGOTIATE_SOURCE_VALUES: &[&str] = &[
    "accept_language",
    "query_param",
    "cookie",
    "user_profile",
    "subdomain",
];

/// i18n bucket cycle — closed `locale_negotiate.strategy` catalog.
/// Three matching algorithms. Doctor `locale_negotiate_strategy_invalid`
/// enforces this set.
pub const LOCALE_NEGOTIATE_STRATEGY_VALUES: &[&str] =
    &["best_match", "prefix_match", "exact_match"];

/// i18n bucket cycle — closed CLDR plural-arm catalog. Doctor
/// `cldr_plural_arm_invalid` enforces this set.
pub const CLDR_PLURAL_ARM_VALUES: &[&str] = &["zero", "one", "two", "few", "many", "other"];

/// Notifications expanded bucket cycle — closed catalog for
/// `notification.digest.template_strategy`. Two strategies: `merge`
/// (last-write-wins per payload key) and `append` (emits a list the
/// digest template iterates over). Doctor surfaces unknown values
/// silently as `None` in IR; LSP completion narrows authoring.
pub const NOTIFICATION_DIGEST_TEMPLATE_STRATEGY_VALUES: &[&str] = &["merge", "append"];

/// i18n bucket cycle — popular BCP-47 tags surfaced as soft completions.
/// The set is **not** closed (BCP-47 tags are open); these are
/// authoring hints only. Doctor never validates against this list.
pub const BCP47_POPULAR_TAGS: &[&str] = &[
    "en-US", "en-GB", "pt-BR", "pt-PT", "es-ES", "es-AR", "es-MX", "fr-FR", "de-DE", "it-IT",
    "ja-JP", "zh-CN", "zh-TW", "ko-KR",
];

// ── IR Error-Vocab — closed catalogs surfaced by the LSP ────────────────────
//
// See `docs/proposals/ir-error-messages-vocab.md` §2.C, §3.4, §7.
// These mirror the runtime's `error.go:128-138` closed catalog of overridable
// error codes and the exposure-field catalogs lowered into `FeatureErrors`.
// The 12 codes here are the **only** error families authors can override via
// `feature.errors.<code> message @translation.<key>`; adding a new code
// requires a proposal (Rule Zero — closed catalog growth at zero).
//
// DB-INTEGRITY-CATALOG-EXT (2026-05-19): extended with the 4 db-integrity
// codes emitted by the runtime's `classifyDBError` helper (handle_db_errors.go).

/// Closed catalog of overridable error codes for `feature.errors`. Mirrors
/// the runtime's emitted-code set; the proposal §2.C pins this list.
pub const ERROR_VOCAB_CODES: &[&str] = &[
    "policy_denied",
    "validation_failed",
    "tenant_mismatch",
    "not_found",
    "rate_limited",
    "bad_request",
    "method_not_allowed",
    "integration_error",
    "unique_violation",
    "foreign_key_violation",
    "not_null_violation",
    "check_violation",
];

/// Closed catalog of exposable wire-envelope fields for `expose client 4xx`.
/// `message_key` is the new opt-in field introduced by proposal §2.G; the
/// other three (`message`, `code`, `data`) pre-existed in the LSP-only shape
/// check (`error_exposure_fields_valid`).
pub const ERROR_VOCAB_EXPOSE_4XX_FIELDS: &[&str] = &["message", "code", "data", "message_key"];

/// Closed catalog of exposable wire-envelope fields for `expose client 5xx`.
/// `message` is **deliberately excluded** — 5xx errors are framework-internal
/// and their messages may carry stack traces or implementation details.
/// Doctor diagnostic `ERR-VOCAB-EXPOSE-5XX-MESSAGE` rejects `message` here.
pub const ERROR_VOCAB_EXPOSE_5XX_FIELDS: &[&str] = &["code", "data"];

/// Closed catalog of `errors default` values.
pub const ERROR_VOCAB_DEFAULT_VALUES: &[&str] = &["hide", "expose"];

// ── Catalog detail lookups ─────────────────────────────────────────────────
//
// Pure `match` lookups that pair each catalog value with its
// one-line hover/completion description. Kept beside the catalogs
// themselves because they share the same closed-set discipline:
// adding a new value here without growing the catalog is incorrect.

/// Hover/completion description for [`RESOURCE_LOCK_STRATEGY_VALUES`].
///
/// ## Examples
///
/// ```
/// use lazuli_lsp::resource_lock_strategy_detail;
/// assert!(resource_lock_strategy_detail("optimistic").unwrap().contains("compare-and-set"));
/// assert!(resource_lock_strategy_detail("not-a-strategy").is_none());
/// ```
pub fn resource_lock_strategy_detail(value: &str) -> Option<&'static str> {
    match value {
        "optimistic" => Some(
            "`lock optimistic version_field: <field>` - compare-and-set on a monotonic integer version column.",
        ),
        "pessimistic" => Some(
            "`lock pessimistic` - runtime acquires a transaction-wide row lock (`SELECT ... FOR UPDATE`).",
        ),
        "row_level" => {
            Some("`lock row_level` - runtime applies `FOR UPDATE` per row at read time.")
        }
        _ => None,
    }
}

/// Hover/completion description for [`ERROR_PAGE_STATUS_VALUES`].
///
/// ## Examples
///
/// ```
/// use lazuli_lsp::error_page_status_detail;
/// assert_eq!(error_page_status_detail("404"), Some("Not Found custom error page."));
/// assert!(error_page_status_detail("418").is_none());
/// ```
pub fn error_page_status_detail(value: &str) -> Option<&'static str> {
    match value {
        "400" => Some("Bad Request custom error page."),
        "401" => Some("Unauthorized custom error page."),
        "403" => Some("Forbidden custom error page."),
        "404" => Some("Not Found custom error page."),
        "405" => Some("Method Not Allowed custom error page."),
        "410" => Some("Gone custom error page."),
        "422" => Some("Unprocessable Content custom error page."),
        "429" => Some("Too Many Requests custom error page."),
        "500" => Some("Internal Server Error custom error page."),
        "502" => Some("Bad Gateway custom error page."),
        "503" => Some("Service Unavailable / maintenance custom error page."),
        "504" => Some("Gateway Timeout custom error page."),
        _ => None,
    }
}

/// Hover/completion description for a closed-catalog value in
/// [`AUTH_CATALOG_VALUES`].
///
/// ## Examples
///
/// ```
/// use lazuli_lsp::auth_catalog_detail;
/// assert!(auth_catalog_detail("argon2id").unwrap().contains("hash"));
/// assert!(auth_catalog_detail("bogus").is_none());
/// ```
pub fn auth_catalog_detail(value: &str) -> Option<&'static str> {
    match value {
        "argon2id" => Some("Password hash algorithm — recommended for v0."),
        "bcrypt" => Some("Password hash algorithm — legacy migration only."),
        "google" => Some("OAuth provider — `google`."),
        "github" => Some("OAuth provider — `github`."),
        "microsoft" => Some("OAuth provider — `microsoft`."),
        "apple" => Some("OAuth provider — `apple`."),
        "totp" => Some("MFA method — Time-based One-Time Password."),
        "true" => Some("Boolean — `true`."),
        "false" => Some("Boolean — `false`."),
        _ => None,
    }
}

/// Hover/completion description for [`AUTH_REFRESH_THEFT_ACTION_VALUES`].
///
/// ## Examples
///
/// ```
/// use lazuli_lsp::auth_refresh_theft_action_detail;
/// assert!(auth_refresh_theft_action_detail("revoke_user").is_some());
/// assert!(auth_refresh_theft_action_detail("ignore").is_none());
/// ```
pub fn auth_refresh_theft_action_detail(value: &str) -> Option<&'static str> {
    match value {
        "revoke_session_family" => Some(
            "Default theft response: revoke the parent_session_id family for this device chain; other devices stay signed in.",
        ),
        "revoke_user" => Some(
            "Aggressive theft response: revoke every session for the user; use for high-stakes apps.",
        ),
        _ => None,
    }
}

/// Hover/completion description for the observability closed-catalog
/// values. Mirrors `auth_catalog_detail` shape.
///
/// ## Examples
///
/// ```
/// use lazuli_lsp::observability_catalog_detail;
/// assert!(observability_catalog_detail("json").unwrap().contains("Log format"));
/// assert!(observability_catalog_detail("yaml").is_none());
/// ```
pub fn observability_catalog_detail(value: &str) -> Option<&'static str> {
    match value {
        "debug" => Some("Log level — verbose tracing for local development."),
        "info" => Some("Log level — production default."),
        "warn" => Some("Log level — recoverable errors and degraded conditions."),
        "error" => Some("Log level — request failures and exceptions."),
        "json" => Some("Log format — machine-parseable single-line JSON."),
        "text" => Some("Log format — human-readable for local development."),
        "pii" => Some("Redaction — auto-strip fields tagged with `@pii.*`."),
        "none" => Some("Redaction — disabled; adapter may still redact."),
        _ => None,
    }
}

/// Notifications expanded bucket cycle — hover/completion description
/// for `notification.digest.template_strategy` closed-catalog values
/// in [`NOTIFICATION_DIGEST_TEMPLATE_STRATEGY_VALUES`].
///
/// ## Examples
///
/// ```
/// use lazuli_lsp::notification_digest_template_strategy_detail;
/// assert!(notification_digest_template_strategy_detail("merge").is_some());
/// assert!(notification_digest_template_strategy_detail("nope").is_none());
/// ```
pub fn notification_digest_template_strategy_detail(value: &str) -> Option<&'static str> {
    match value {
        "merge" => Some(
            "Merge — collapse per-trigger payloads into a single object (last-write-wins per key). Default when omitted.",
        ),
        "append" => {
            Some("Append — emit a list of per-trigger payloads the digest template iterates over.")
        }
        _ => None,
    }
}

/// Hover/completion description for the deploy.strategy closed-catalog
/// values. Mirrors `observability_catalog_detail` shape. See
/// [`DEPLOY_STRATEGY_VALUES`] for the canonical list.
///
/// ## Examples
///
/// ```
/// use lazuli_lsp::deploy_strategy_detail;
/// assert!(deploy_strategy_detail("canary").unwrap().contains("Canary"));
/// assert!(deploy_strategy_detail("yeet").is_none());
/// ```
pub fn deploy_strategy_detail(value: &str) -> Option<&'static str> {
    match value {
        "rolling" => Some(
            "Rolling rollout — replace instances one window at a time. Lowest risk; longest cutover.",
        ),
        "blue_green" => Some(
            "Blue/green rollout — provision parallel stack, flip traffic atomically. Fast rollback; doubles infra during cutover.",
        ),
        "canary" => Some(
            "Canary rollout — shift traffic incrementally to a fresh cohort. Best for unproven changes; requires per-version observability.",
        ),
        _ => None,
    }
}

// ── IR Error-Vocab built-in tables ─────────────────────────────────────────
//
// Pure data tables companion to `ERROR_VOCAB_CODES`. The hover layer
// renders these as the **fallback** when a feature has no
// `errors.<code> message @translation.<key>` override.

/// Built-in PT-BR fallback strings shipped with the runtime — used by the
/// LSP hover to **resolve** the displayed text when the surrounding feature
/// has no override (proposal §2.D, §7.2). Mirrors
/// `runtime/go/lazuli/i18n/builtin.pt-BR.json`; the LSP keeps a copy here so
/// hover stays self-contained (no filesystem read at hover time).
///
/// ## Examples
///
/// ```
/// use lazuli_lsp::error_vocab_code_builtin_en_us;
/// assert!(error_vocab_code_builtin_en_us("not_found").is_some());
/// assert!(error_vocab_code_builtin_en_us("nonsense").is_none());
/// ```
pub fn error_vocab_code_builtin_en_us(code: &str) -> Option<&'static str> {
    match code {
        "policy_denied" => Some("You need to sign in to do this."),
        "validation_failed" => Some("Some of the information you sent is missing or incorrect."),
        "tenant_mismatch" => Some("This action does not belong to the current workspace."),
        "not_found" => Some("We couldn't find the requested item."),
        "rate_limited" => Some("Too many attempts — please wait a moment."),
        "bad_request" => Some("The request was sent in an invalid format."),
        "method_not_allowed" => Some("This operation is not supported on this route."),
        "integration_error" => Some("There was a problem reaching an external service."),
        "unique_violation" => Some("This item already exists. Try another value."),
        "foreign_key_violation" => {
            Some("We couldn't complete this because a related item is missing.")
        }
        "not_null_violation" => {
            Some("Some required information is missing. Please review and try again.")
        }
        "check_violation" => Some("One of the values doesn't meet the rules for this operation."),
        _ => None,
    }
}

/// One-line catalog-style hover/completion description for each of the
/// closed-catalog error codes in [`ERROR_VOCAB_CODES`]. Shown inline in
/// completion lists and as a fallback hover (when no feature-level
/// override is found in the document).
///
/// ## Examples
///
/// ```
/// use lazuli_lsp::error_vocab_code_detail;
/// assert!(error_vocab_code_detail("rate_limited").unwrap().contains("Throttle"));
/// assert!(error_vocab_code_detail("not_a_code").is_none());
/// ```
pub fn error_vocab_code_detail(code: &str) -> Option<&'static str> {
    match code {
        "policy_denied" => Some(
            "Authorization failure — actor lacks the required policy. Resolves through the per-command, per-policy, feature, builtin chain.",
        ),
        "validation_failed" => Some(
            "Input validation failure — per-field details ride in `data.field_errors`; this message is the human-readable headline.",
        ),
        "tenant_mismatch" => Some(
            "Cross-tenant access rejected — the targeted resource does not belong to the active workspace.",
        ),
        "not_found" => Some(
            "Resource lookup failed — no row matched the requested key under the current scope.",
        ),
        "rate_limited" => {
            Some("Throttle tripped — the caller exceeded the declared `rate_limit` axis budget.")
        }
        "bad_request" => Some(
            "Malformed request — the request envelope failed structural parsing or schema validation.",
        ),
        "method_not_allowed" => {
            Some("HTTP verb mismatch — the route is registered under a different method.")
        }
        "integration_error" => Some(
            "External service failure — an outbound integration returned an error or timed out.",
        ),
        "unique_violation" => Some(
            "Database unique constraint tripped (Postgres SQLSTATE 23505). Maps to HTTP 409. Authors override with `errors unique_violation message @translation.<key>` for domain-specific phrasing.",
        ),
        "foreign_key_violation" => Some(
            "Database foreign-key constraint tripped (Postgres SQLSTATE 23503). Maps to HTTP 400. A row references a parent that does not exist (or was deleted).",
        ),
        "not_null_violation" => Some(
            "Database NOT NULL constraint tripped (Postgres SQLSTATE 23502). Maps to HTTP 400. A required column was left blank by the request bindings.",
        ),
        "check_violation" => Some(
            "Database CHECK constraint tripped (Postgres SQLSTATE 23514). Maps to HTTP 400. A column value failed the table's declared check expression.",
        ),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_catalog_entry_has_a_detail_string() {
        // Smoke test: every value in each closed catalog must yield a
        // hover description. A catalog that grows without a matching
        // detail arm is the exact regression this guards against.
        for v in RESOURCE_LOCK_STRATEGY_VALUES {
            assert!(
                resource_lock_strategy_detail(v).is_some(),
                "missing lock detail: {v}"
            );
        }
        for v in ERROR_PAGE_STATUS_VALUES {
            assert!(
                error_page_status_detail(v).is_some(),
                "missing error_page detail: {v}"
            );
        }
        for v in AUTH_CATALOG_VALUES {
            assert!(
                auth_catalog_detail(v).is_some(),
                "missing auth detail: {v}"
            );
        }
        for v in AUTH_REFRESH_THEFT_ACTION_VALUES {
            assert!(
                auth_refresh_theft_action_detail(v).is_some(),
                "missing theft action detail: {v}"
            );
        }
        for v in OBSERVABILITY_CATALOG_VALUES {
            assert!(
                observability_catalog_detail(v).is_some(),
                "missing observability detail: {v}"
            );
        }
        for v in NOTIFICATION_DIGEST_TEMPLATE_STRATEGY_VALUES {
            assert!(
                notification_digest_template_strategy_detail(v).is_some(),
                "missing notification digest detail: {v}"
            );
        }
        for v in DEPLOY_STRATEGY_VALUES {
            assert!(
                deploy_strategy_detail(v).is_some(),
                "missing deploy strategy detail: {v}"
            );
        }
        for code in ERROR_VOCAB_CODES {
            assert!(
                error_vocab_code_detail(code).is_some(),
                "missing error vocab detail: {code}"
            );
            assert!(
                error_vocab_code_builtin_en_us(code).is_some(),
                "missing error vocab builtin: {code}"
            );
        }
    }

    #[test]
    fn unknown_values_resolve_to_none() {
        assert!(resource_lock_strategy_detail("not-a-strategy").is_none());
        assert!(error_page_status_detail("999").is_none());
        assert!(auth_catalog_detail("nope").is_none());
        assert!(error_vocab_code_detail("not_a_code").is_none());
    }
}
