//! auth_session_canonical_columns_001 — the resource bound by
//! `auth sessions resource <X>` must declare every column the runtime
//! session resolver reads, or authentication silently fails.
//!
//! ## The silent-403 failure mode
//!
//! The production session middleware
//! (`runtime/go/lazuli/session.go::populateProductionSession`) hands the
//! request token to the registered `SessionResolver`. The generated
//! resolver (`runtime/go/lazuli/auth/session.go::ResolveSession`) issues
//! a fixed SELECT against the session table:
//!
//! ```sql
//! SELECT id, "user", expires_at FROM <session_table> WHERE token_hash = $1 LIMIT 1
//! ```
//!
//! then rejects the row when `expires_at` is not in the future. If the
//! declared session resource is MISSING a column this SELECT (or the
//! expiry guard) needs, the resolver finds no usable row — every
//! authenticated request falls through to anonymous and the policy layer
//! 403s it, with **no error naming the real cause**. The bug looks like
//! "login works but every subsequent request is forbidden," and the only
//! evidence is a `column does not exist` buried in a `slog.Warn`.
//!
//! ## The contract this rule enforces
//!
//! Two columns are **always** read by the resolver and must be authored
//! on the session resource (the framework does not synthesize them):
//!
//! - **`expires_at`** (`DateTime`) — the temporal validity bound. Read by
//!   every resolver path (single-token `ResolveSession`, rotation
//!   `findRefreshRow`) and compared against `ctx.now`. Without it the
//!   resolver cannot decide whether a session is live.
//! - **a foreign key to the auth identity resource** — the actor the
//!   resolver returns (`userID`). Resolved name-agnostically from
//!   `auth identity <Resource>.<field>`: the session resource must carry a
//!   field whose type references that identity resource (canonically
//!   `user: User` / `customer: Customer`). Without it the resolver has no
//!   actor to attach to `ctx.User`.
//!
//! ## What is deliberately NOT required (avoids false positives)
//!
//! - **`id` / `created_at`** — framework-synthesized (`id` is the implicit
//!   `BIGSERIAL PRIMARY KEY`; `created_at` arrives via `timestamps` /
//!   conventions), so a session resource that omits them is still valid.
//! - **the credential-hash column** — the resolver reads `token_hash` on
//!   the single-token path but `refresh_token_hash` on the rotation-only
//!   path; both spellings ship in real session resources
//!   (`production-grade`, `user-auth`, `full-capsule` carry only
//!   `refresh_token_hash`). Asserting a single literal would false-flag
//!   them, so this rule leaves the hash column to the codegen/runtime
//!   layer.
//! - **`org_id` / `revoked_at` / refresh columns** — tenancy and
//!   rotation columns the runtime reads behind an `isUndefinedColumn`
//!   fallback (it degrades to a narrower SELECT), so their absence does
//!   not silently 403 the core auth path.
//!
//! ## Severity
//!
//! **error** under strict/production (joins the session-family
//! enforcement codes `auth-session-ttl` / `auth_sessions_resource_unknown`
//! / `session-query-temporal-validity` via
//! `security_profile::is_security_enforcement_code`), WARNING under the
//! prototype profile — so it blocks under the scaffolded `[doctor]
//! profile = "strict"`.
//!
//! Resolution order vs. the sibling `auth_sessions_resource_unknown_001`:
//! that rule owns the "binding names a resource the feature never
//! declared" case. This rule only runs once the binding resolves to a
//! real local resource, so the two never double-fire.
//!
//! Reference: runtime/go/lazuli/auth/session.go (`ResolveSession`)
//! Reference: docs/canonical-semantics.md §"Active sessions"

use std::path::{Path, PathBuf};

use lazuli_ir::{BuiltinType, Feature, Resource, TypeRef};

// ── output ──────────────────────────────────────────────────────────────────

/// One resolver-required column absent from the declared session resource.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// `.lzi` path the `auth sessions` declaration lives in.
    pub path: PathBuf,
    /// Feature owning the `auth sessions` binding.
    pub feature: String,
    /// The session resource named by `auth sessions resource <X>`.
    pub session_resource: String,
    /// The missing column the resolver requires.
    pub missing: MissingColumn,
    /// Byte offset of the session resource header (from its `span_ref`)
    /// for source anchoring. `None` when the IR carried no span.
    pub offset: Option<usize>,
}

/// Which resolver-required column is absent, and (for the identity FK) the
/// identity resource the missing reference should point at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MissingColumn {
    /// The `expires_at` temporal-validity column.
    ExpiresAt,
    /// A foreign key to the named auth identity resource.
    IdentityRef { identity_resource: String },
}

impl Finding {
    /// Stable snake_case doctor rule code (parity with the sibling
    /// `auth_*_001` modules whose `CODE` is snake_case).
    pub const CODE: &'static str = "auth_session_canonical_columns_001";

    /// Kebab-case LSP/profile code registered in
    /// `security_profile::is_security_enforcement_code` so the rule is a
    /// WARNING under prototype and an ERROR under strict/production.
    pub const KEBAB_CODE: &'static str = "auth-session-canonical-columns";

    /// The temporal-validity column the resolver always reads.
    pub const EXPIRES_AT: &'static str = "expires_at";

    /// Render the remediation message naming the missing column and why
    /// the resolver needs it.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// // let msg = finding.message();
    /// ```
    pub fn message(&self) -> String {
        match &self.missing {
            MissingColumn::ExpiresAt => format!(
                "auth session resource `{resource}` (feature `{feature}`) is missing the `{col}: DateTime` column. The runtime session resolver reads `{col}` on every authenticated request to enforce session expiry; without it the resolver finds no usable row and every authenticated request silently 403s. Add `{col}: DateTime required`.",
                resource = self.session_resource,
                feature = self.feature,
                col = Self::EXPIRES_AT,
            ),
            MissingColumn::IdentityRef { identity_resource } => format!(
                "auth session resource `{resource}` (feature `{feature}`) declares no foreign key to the identity resource `{identity}`. The session resolver reads this column to recover the authenticated actor (`ctx.User`); without it the resolver cannot attach an identity and every authenticated request silently 403s. Add a `<name>: {identity} required` field (canonically `user: {identity}`).",
                resource = self.session_resource,
                feature = self.feature,
                identity = identity_resource,
            ),
        }
    }
}

// ── detection ─────────────────────────────────────────────────────────────────

/// Run auth_session_canonical_columns_001 on a single feature.
///
/// Returns one finding per resolver-required column the declared session
/// resource omits. Empty when the feature has no `auth.sessions` binding,
/// the binding names a resource the feature does not declare (a separate
/// rule, `auth_sessions_resource_unknown_001`, owns that), or the resource
/// carries every required column.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_ir::Feature;
/// // let findings = check(&feature, Path::new("account.lzi"));
/// ```
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    let Some(auth) = feature.auth.as_ref() else {
        return Vec::new();
    };
    let Some(sessions) = auth.sessions.as_ref() else {
        return Vec::new();
    };
    let session_resource = sessions.resource.name.as_str();

    // Resolve the session resource locally. When the binding names a
    // resource the feature does not declare, there is nothing to inspect —
    // `auth_sessions_resource_unknown_001` fires for that case instead.
    let Some(resource) = feature
        .resources
        .iter()
        .find(|r| r.name == session_resource)
    else {
        return Vec::new();
    };

    // The identity resource the session FK must reference, recovered from
    // `auth identity <Resource>.<field>`. Always present (AuthIdentity is a
    // required slot on the Auth block).
    let identity_resource = auth.identity.field.resource.name.as_str();

    let mut findings = Vec::new();
    let offset = resource.span_ref.as_ref().map(|s| s.start);

    // (1) expires_at — temporal validity bound, read on every resolver path.
    if !has_expires_at(resource) {
        findings.push(Finding {
            path: path.to_path_buf(),
            feature: feature.name.clone(),
            session_resource: session_resource.to_owned(),
            missing: MissingColumn::ExpiresAt,
            offset,
        });
    }

    // (2) a foreign key to the identity resource — the actor the resolver
    // returns. Name-agnostic: any field whose type references the identity
    // resource satisfies it (`user: User`, `customer: Customer`, ...).
    if !has_identity_ref(resource, identity_resource) {
        findings.push(Finding {
            path: path.to_path_buf(),
            feature: feature.name.clone(),
            session_resource: session_resource.to_owned(),
            missing: MissingColumn::IdentityRef {
                identity_resource: identity_resource.to_owned(),
            },
            offset,
        });
    }

    findings
}

/// True when the resource declares an `expires_at` column of a
/// date/time-compatible type. The name is fixed (the resolver SELECTs the
/// literal `expires_at`); the type is checked loosely (`DateTime`, or a
/// `Date`) so a slightly-off-but-temporal declaration still satisfies the
/// resolver's scan into a `time.Time`.
fn has_expires_at(resource: &Resource) -> bool {
    resource.fields.iter().any(|f| {
        f.name == Finding::EXPIRES_AT
            && matches!(
                f.type_ref,
                TypeRef::Builtin(BuiltinType::DateTime) | TypeRef::Builtin(BuiltinType::Date)
            )
    })
}

/// True when the resource declares at least one field whose type
/// references `identity_resource` — the session's foreign key to the
/// authenticated actor. Tolerates the `Many` wrapper defensively even
/// though a session FK is singular in practice.
fn has_identity_ref(resource: &Resource, identity_resource: &str) -> bool {
    resource
        .fields
        .iter()
        .any(|f| type_ref_targets(&f.type_ref, identity_resource))
}

/// True when `type_ref` (possibly wrapped in `Many`) is a user-defined
/// reference to `target`.
fn type_ref_targets(type_ref: &TypeRef, target: &str) -> bool {
    match type_ref {
        TypeRef::UserDefined(qn) => qn.name == target,
        TypeRef::Many(inner) => type_ref_targets(inner, target),
        _ => false,
    }
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    include!("auth_session_canonical_columns_001_tests.rs");
}
