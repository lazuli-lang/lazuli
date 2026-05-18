//! Closed codegen pattern catalog for generated Go attribution.
//!
//! Each emitted operation block or Go function that represents codegen
//! behavior carries a `//lazuli:pattern <id> <version>` header. The
//! emitter lint rejects generated functions that omit the header.

use super::printer::GoPrinter;

pub const PATTERN_COMMAND_PGX_INSERT: (&str, &str) = ("command_pgx_insert", "v1");
pub const PATTERN_COMMAND_PGX_UPDATE: (&str, &str) = ("command_pgx_update", "v1");
pub const PATTERN_QUERY_PGX_LIST: (&str, &str) = ("query_pgx_list", "v1");
pub const PATTERN_QUERY_PGX_LOOKUP: (&str, &str) = ("query_pgx_lookup", "v1");
pub const PATTERN_QUERY_PGX_SQL: (&str, &str) = ("query_pgx_sql", "v1");
pub const PATTERN_JOB_HANDLER: (&str, &str) = ("job_handler", "v1");
pub const PATTERN_WEBHOOK_RECEIVER: (&str, &str) = ("webhook_receiver", "v1");
pub const PATTERN_AUTH_LOGIN: (&str, &str) = ("auth_login", "v1");
pub const PATTERN_NOTIFICATION_DISPATCH: (&str, &str) = ("notification_dispatch", "v1");
pub const PATTERN_ERROR_WRAP_HELPER: (&str, &str) = ("error_wrap_helper", "v1");
pub const PATTERN_MAIN_ENTRYPOINT: (&str, &str) = ("main_entrypoint", "v1");
pub const PATTERN_EXTENSION_STUB: (&str, &str) = ("extension_stub", "v1");
pub const PATTERN_REPORT_RUN: (&str, &str) = ("report_run", "v1");
pub const PATTERN_REPORT_AUTOMOUNT: (&str, &str) = ("report_automount", "v1");
pub const PATTERN_ENCRYPTION_REGISTER: (&str, &str) = ("encryption_register", "v1");
pub const PATTERN_POLLER_REGISTER: (&str, &str) = ("poller_register", "v1");
pub const PATTERN_RBAC_REGISTER: (&str, &str) = ("rbac_register", "v1");
pub const PATTERN_FEATURE_REGISTER: (&str, &str) = ("feature_register", "v1");
pub const PATTERN_MCP_SERVER_REGISTER: (&str, &str) = ("mcp_server_register", "v1");
pub const PATTERN_CORS_REGISTER: (&str, &str) = ("cors_register", "v1");
pub const PATTERN_ERROR_RESOLVER: (&str, &str) = ("error_resolver", "v1");

pub fn emit_pattern_header(p: &mut GoPrinter, pattern: (&str, &str)) {
    p.line(&format!("//lazuli:pattern {} {}", pattern.0, pattern.1));
}
