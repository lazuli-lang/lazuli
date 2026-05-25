//! `api <name>` declaration — explicit HTTP endpoint with a Go handler.
//!
//! The `api` surface is the escape hatch for HTTP endpoints that don't
//! fit `command` / `query` / `webhook` (e.g. file uploads, server-sent
//! events, raw export streams). The author commits to a method, path,
//! output type, optional rate limit, and a handler reference.
//!
//! Authoring shape:
//!
//! ```text
//! api export_customers
//!   method GET
//!   path "/api/customers/export"
//!   route tenant_id: ID
//!   policy @policy.admin
//!   rate_limit "10 per minute per actor"
//!   output ExportPayload
//!   handler "./api/export_customers.go"
//! ```
//!
//! `ApiDecl` re-uses `HttpMethod` (from `agent`), `RateLimitSpecAst`,
//! `PolicyExprAst`, `LocaleNegotiateDecl`, plus the command-shared
//! `CommandRouteSlot`, `CommandInputDecl`, `CommandDeprecatedDecl`.

use serde::{Deserialize, Serialize};

use super::{
    CommandDeprecatedDecl, CommandInputDecl, CommandRouteSlot, HttpMethod, LocaleNegotiateDecl,
    PolicyExprAst, RateLimitSpecAst, Span,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiDecl {
    pub name: String,
    /// `method GET|POST|PUT|PATCH|DELETE`. Captured as a typed enum.
    pub method: HttpMethod,
    /// `path "/api/customers/export"` — verbatim path literal.
    pub path: String,
    /// `output <TypeRef>` — captured as raw type text. The analyzer
    /// projects to `TypeRef`.
    pub output: String,
    /// `policy @policy.<name>`.
    pub policy: Option<String>,
    /// RB.S6 — structured policy expression form.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_expr: Option<PolicyExprAst>,
    /// `rate_limit "<N per period per scope>"` declarations on the
    /// api block. See `ir-rate-limit-env-aware` cell 1.
    pub rate_limit: Option<RateLimitSpecAst>,
    /// `handler "./api/<name>.go"`.
    pub handler: Option<String>,
    /// i18n bucket cycle — per-api `locale_negotiate` block override.
    pub locale_negotiate: Option<LocaleNegotiateDecl>,
    /// `route <name>: <Type>` slots — path placeholders bound to typed
    /// values. Captured verbatim; codegen currently materializes them
    /// as args inferred from the path string.
    #[serde(default)]
    pub route: Vec<CommandRouteSlot>,
    /// `input` block — typed body fields. Captured verbatim; codegen
    /// does not lower these yet (handler @fn.<name> reads the request
    /// body itself).
    #[serde(default)]
    pub input: Option<CommandInputDecl>,
    /// `deprecated` child block shared with commands.
    pub deprecated: Option<CommandDeprecatedDecl>,
    pub span: Span,
}
