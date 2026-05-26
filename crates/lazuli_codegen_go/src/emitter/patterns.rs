//! Closed codegen pattern catalog for generated Go attribution.
//!
//! Each emitted operation block or Go function that represents codegen
//! behavior carries a `//lazuli:pattern <id> <version>` header. The
//! emitter lint (see [`crate::emitter::lint`]) rejects generated
//! functions that omit the header — the catalog is intentionally closed
//! so a pattern audit can enumerate every shape the generator emits.
//!
//! Each constant is a `(id, version)` tuple. Version bumps are reserved
//! for breaking changes to the emitted body; non-breaking enhancements
//! reuse the same version.

use super::printer::GoPrinter;

/// `command_pgx_insert v1` — emitted on Postgres INSERT command handlers.
pub const PATTERN_COMMAND_PGX_INSERT: (&str, &str) = ("command_pgx_insert", "v1");
/// `command_pgx_update v1` — emitted on Postgres UPDATE command handlers.
pub const PATTERN_COMMAND_PGX_UPDATE: (&str, &str) = ("command_pgx_update", "v1");
/// `query_pgx_list v1` — emitted on `query.list` paginated readers.
pub const PATTERN_QUERY_PGX_LIST: (&str, &str) = ("query_pgx_list", "v1");
/// `query_pgx_lookup v1` — emitted on `query.lookup` single-row readers.
pub const PATTERN_QUERY_PGX_LOOKUP: (&str, &str) = ("query_pgx_lookup", "v1");
/// `query_pgx_sql v1` — emitted on raw-SQL query handlers.
pub const PATTERN_QUERY_PGX_SQL: (&str, &str) = ("query_pgx_sql", "v1");
/// `job_handler v1` — emitted on background job handlers.
pub const PATTERN_JOB_HANDLER: (&str, &str) = ("job_handler", "v1");
/// `webhook_receiver v1` — emitted on inbound webhook handlers.
pub const PATTERN_WEBHOOK_RECEIVER: (&str, &str) = ("webhook_receiver", "v1");
/// `auth_login v1` — emitted on the login handler.
pub const PATTERN_AUTH_LOGIN: (&str, &str) = ("auth_login", "v1");
/// `auth_refresh v1` — emitted on the refresh-token handler.
pub const PATTERN_AUTH_REFRESH: (&str, &str) = ("auth_refresh", "v1");
/// `notification_dispatch v1` — emitted on notification dispatch handlers.
pub const PATTERN_NOTIFICATION_DISPATCH: (&str, &str) = ("notification_dispatch", "v1");
/// `error_wrap_helper v1` — emitted on generated error-wrapping helpers.
pub const PATTERN_ERROR_WRAP_HELPER: (&str, &str) = ("error_wrap_helper", "v1");
/// `main_entrypoint v1` — emitted on `main.go`'s `func main()`.
pub const PATTERN_MAIN_ENTRYPOINT: (&str, &str) = ("main_entrypoint", "v1");
/// `extension_stub v1` — emitted on user-extension stub functions.
pub const PATTERN_EXTENSION_STUB: (&str, &str) = ("extension_stub", "v1");
/// `report_run v1` — emitted on report `run` handlers.
pub const PATTERN_REPORT_RUN: (&str, &str) = ("report_run", "v1");
/// `report_automount v1` — emitted on auto-mounted report registrations.
pub const PATTERN_REPORT_AUTOMOUNT: (&str, &str) = ("report_automount", "v1");
/// `encryption_register v1` — emitted on field-encryption registration.
pub const PATTERN_ENCRYPTION_REGISTER: (&str, &str) = ("encryption_register", "v1");
/// `poller_register v1` — emitted on poller registration.
pub const PATTERN_POLLER_REGISTER: (&str, &str) = ("poller_register", "v1");
/// `rbac_register v1` — emitted on RBAC role/permission registration.
pub const PATTERN_RBAC_REGISTER: (&str, &str) = ("rbac_register", "v1");
/// `feature_register v1` — emitted on per-feature `init()` registration.
pub const PATTERN_FEATURE_REGISTER: (&str, &str) = ("feature_register", "v1");
/// `mcp_server_register v1` — emitted on MCP server registration.
pub const PATTERN_MCP_SERVER_REGISTER: (&str, &str) = ("mcp_server_register", "v1");
/// `cors_register v1` — emitted on CORS allow-list registration.
pub const PATTERN_CORS_REGISTER: (&str, &str) = ("cors_register", "v1");
/// `error_resolver v1` — emitted on the generated error-resolver table.
pub const PATTERN_ERROR_RESOLVER: (&str, &str) = ("error_resolver", "v1");
/// `translation_catalog v1` — emitted on the i18n message catalog.
pub const PATTERN_TRANSLATION_CATALOG: (&str, &str) = ("translation_catalog", "v1");
/// `app_integration_register v1` — emitted on app-integration registration.
pub const PATTERN_APP_INTEGRATION_REGISTER: (&str, &str) = ("app_integration_register", "v1");
/// `route_guard v1` — emitted on per-route guard wiring.
pub const PATTERN_ROUTE_GUARD: (&str, &str) = ("route_guard", "v1");

/// Write a `//lazuli:pattern <id> <version>` header line.
///
/// Every generator that produces a Lazuli-owned function calls this just
/// before emitting the `func ...` line. The lint pass then walks
/// generated files and rejects any `func` body that doesn't carry the
/// preceding pattern marker.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_codegen_go::emitter::printer::GoPrinter;
/// use lazuli_codegen_go::emitter::patterns::{emit_pattern_header, PATTERN_AUTH_LOGIN};
/// let mut p = GoPrinter::new();
/// emit_pattern_header(&mut p, PATTERN_AUTH_LOGIN);
/// assert!(p.finish().contains("//lazuli:pattern auth_login v1"));
/// ```
pub fn emit_pattern_header(p: &mut GoPrinter, pattern: (&str, &str)) {
    p.line(&format!("//lazuli:pattern {} {}", pattern.0, pattern.1));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emitter::printer::GoPrinter;

    #[test]
    fn emit_pattern_header_writes_marker_line() {
        let mut p = GoPrinter::new();
        emit_pattern_header(&mut p, PATTERN_AUTH_LOGIN);
        let out = p.finish();
        assert!(out.contains("//lazuli:pattern auth_login v1"));
    }
}
