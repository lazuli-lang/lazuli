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

pub fn emit_pattern_header(p: &mut GoPrinter, pattern: (&str, &str)) {
    p.line(&format!("//lazuli:pattern {} {}", pattern.0, pattern.1));
}
