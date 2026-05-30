    // Inspect-CLI per-axis projection tests (aggregates, commands, apis,
    // resources, queries, records, tools, tenant_migrations) — split from
    // `crates/lazuli_cli/src/tests.rs`.

    use std::path::Path;

    use crate::{inspect_canonical_source, parse_expand_set};

    // CL.C.4 — `--expand=aggregates` projection test (spec wave-c-cl4).

include!("inspect_expand_projections_p1.rs");
include!("inspect_expand_projections_p2.rs");
