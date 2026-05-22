# Status - Cell B.4 query.view

Commit: 74e510414bf9d46660ec9a2f8ad5ccd6139e4e04

IR delta:
- Added `SqlQuery.sql_kind: SqlQueryKind::{Sql, View}` with default SQL back-compat serialization.
- Added `QueryKind::View` and `ToolKind::QueryView`.
- Reused the existing `Query::Sql`/`SqlQuery` path for `query.view`, with `source @file.<name>.sql` lowered to `app/features/<feature>/queries/<name>.sql`.
- Extended Go runtime query registration with `QueryView`, `SQLArgs`, `SQLMany`, SQL text/file loading, and pgx struct scanning.

Parser test count:
- 2 new parser unit tests:
  - `query_view_parses_file_source_and_list_returns`
  - `query_view_parses_scalar_returns_and_scope`

Codegen golden/test names:
- Go: `query_view_emits_typed_sql_runtime_binding`
- TS SDK: `query_view_sdk_uses_declared_returns_type`
- IR round trip: `query_view_sql_kind_round_trips_without_colliding_with_query_tag`
- Doctor: `doctor_query_view_reports_missing_sql_file`, `doctor_query_view_reports_unsafe_sql_pattern`
- LSP: `rich_hover_for_query_view_requires_returns_and_file_source`, `completion_inside_query_view_offers_returns_source_params`

Verification:
- `cargo test` - passed.
- `go test ./...` in `runtime/go` - passed.
- `git diff --check` - passed.

Pilot smoke result:
- Not run. This worktree does not include a hostpoint app or the requested pilot read handlers (`get_host_home`, `get_property_detail_view`, `list_chat_inbox`), so there was no `@fn` raw-SQL handler to convert and no app-level `pnpm lazuli:generate && go build ./... && pnpm app:typecheck` target to execute.
