//! `lazuli_cli` test suite — pulled from main.rs's `#[cfg(test)] mod tests`
//! block as part of the W4.5 R2 split. Kept as `mod tests { ... }` so the
//! inner string-literal content (raw and non-raw) preserves its original
//! indentation; de-indenting would corrupt the multi-line .lzi fixture
//! strings the tests assert against.

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    use clap::Parser;
    use tempfile::TempDir;

    use crate::cli_args::{DesignExportTarget, DesignImportFormat};
use crate::go_work_io::add_missing_go_work_use_entries;
use crate::{
        Cli, Commands, DesignCommand, ExpandSet,
        GenerateKind, MigrateCommand, REGISTRY_TEMPLATE,
        app_template, default_module_name, emit_feature_barrel_ts, emit_feature_react_hooks_ts,
        emit_feature_sdk_ts, expand_canonical_source, inspect_canonical_source, inspect_json_value,
        new_command, parse_expand_set, pascal_case, pascal_case_project_name,
        render_inspect_symbol_lazuli, scaffold_bare, scaffold_from_template, templates,
        write_go_work_preserving_entries,
    };

    // NOTE: tests for `query_ident` / `strip_query_verb_prefix` (the
    // verb-prefix dedup added alongside the Hostpoint bug fix) cannot
    // live here because the `lazuli_cli` test binary currently fails to
    // compile on this branch's base (pre-existing `doctor::lzx::ir_stub`
    // field mismatches, unrelated to this change — see `cargo test -p
    // lazuli_cli` baseline). The behaviour is covered by the matching
    // tests in `lazuli_codegen_ts::lzx::tests` (the helper logic is
    // identical and was factored to mirror the CLI's local copy).

    mod migrate {
        include!("tests/migrate.rs");
    }

    mod test_support {
        include!("tests/test_support.rs");
    }
    use test_support::*;

    mod codegen_ts_enums {
        include!("tests/codegen_ts_enums.rs");
    }

    mod codegen_ts_command_sdk {
        include!("tests/codegen_ts_command_sdk.rs");
    }

    mod codegen_ts_react_hooks {
        include!("tests/codegen_ts_react_hooks.rs");
    }

    mod codegen_ts_query_sdk {
        include!("tests/codegen_ts_query_sdk.rs");
    }

    mod codegen_ts_plugin_semantic {
        include!("tests/codegen_ts_plugin_semantic.rs");
    }

    mod dispatch {
        include!("tests/dispatch.rs");
    }

    mod in_place {
        include!("tests/in_place.rs");
    }

    mod scaffold {
        include!("tests/scaffold.rs");
    }

    mod inspect_expand_basic {
        include!("tests/inspect_expand_basic.rs");
    }

    mod inspect_manifest_json {
        include!("tests/inspect_manifest_json.rs");
    }

    mod inspect_expand_projections {
        include!("tests/inspect_expand_projections.rs");
    }

    mod inspect_summary_agent {
        include!("tests/inspect_summary_agent.rs");
    }

    mod inspect_auth_storage {
        include!("tests/inspect_auth_storage.rs");
    }

    mod inspect_http_render {
        include!("tests/inspect_http_render.rs");
    }
}
