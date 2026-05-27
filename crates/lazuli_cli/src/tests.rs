//! `lazuli_cli` test suite — pulled from main.rs's `#[cfg(test)] mod tests`
//! block as part of the W4.5 R2 split. Kept as `mod tests { ... }` so the
//! inner string-literal content (raw and non-raw) preserves its original
//! indentation; de-indenting would corrupt the multi-line .lzi fixture
//! strings the tests assert against.

#[cfg(test)]
mod tests {
    // Top-level imports were used by tests now extracted into sibling
    // sub-modules under `tests/`. Each included file carries its own
    // `use` lines; the parent only needs `test_support` for the
    // shared-fixtures `pub(super)` re-export pattern.

    // NOTE: tests for `query_ident` / `strip_query_verb_prefix` (the
    // verb-prefix dedup added alongside the the canonical pilot bug fix) cannot
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
