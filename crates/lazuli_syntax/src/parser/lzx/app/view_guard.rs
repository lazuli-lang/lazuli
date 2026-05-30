//! `.lzx` **policy / view-guard** family — access control + lifecycle gates.
//!
//! Two related blocks live here:
//!
//! 1. **`route_guard`** — the app-level defaults block:
//!
//!    ```text
//!    route_guard
//!      default_policy @policy.member
//!      on_unauthenticated redirect "/login"
//!      on_unauthorized redirect "/403"
//!      skeleton @client.app_shell
//!    ```
//!
//! 2. **`policy <ref>`** — the per-route / per-view guard:
//!
//!    ```text
//!    policy @policy.member
//!      on_unauthenticated redirect "/login"
//!      on_unauthorized redirect "/403"
//!      requires_lifecycle Customer = onboarded
//!      on_lifecycle_pending @resume onboarding
//!      forbid_when @role.host dispatch_to "/concierge"
//!    ```
//!
//! `policy <ref>` accepts a single atom (`@policy.member`) or a list
//! form (`[@policy.a, @policy.b]`). The redirect, lifecycle, and
//! forbid-when sub-clauses are parsed by dedicated helpers in this
//! module; the lifecycle pair travels with the guard so it stays
//! colocated with its consumers.
//!
//! The `attach_lzx_*` helpers are guard mutators used by
//! experience-view parsers that build the guard incrementally — they
//! enforce "declared at most once" and merge spans.

use crate::ast::{
    LzxForbidWhen, LzxRequiresField, LzxRequiresLifecycle, LzxRequiresLifecycleIn,
    LzxRouteGuardDefaults, LzxScalarLiteral, LzxViewGuard, Span,
};

use super::super::super::common::{
    SourceLine, is_lzx_bare_ident, is_lzx_resume_ref, is_trivia, line_error, line_error_owned,
    unquote_lzx_value,
};
use super::super::super::error::ParseError;

include!("view_guard_p1.rs");
include!("view_guard_p2.rs");
