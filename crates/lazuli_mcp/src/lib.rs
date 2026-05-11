//! Lazuli MCP server crate (skeleton).
//!
//! ## Write surface (do not extend without an architect ack)
//!
//! `write_dsl_feature(new_text)` is the *only* MCP write tool, per
//! `docs/mcp-abi.md:24` and the boundary listed in
//! `docs/proposals/ai-primitives-v0-implementation.md`. The Cut A series
//! does not introduce a second write path. Tool / eval / discriminator
//! authoring goes through the same `.lzi` source the rest of the
//! language uses.
//!
//! ## Read surface (Cut A delta — pending wiring)
//!
//! Cut A bumps `LZIR_SCHEMA` from `0.3.6` to `0.4.0` (additive). MCP
//! read tools may expose a new `tools` projection mirroring
//! `lazuli inspect --expand=tools`, surfacing per-agent dispatch
//! graphs (binding reference + kind + scope + derived_effect). This is
//! a minor MCP bump per `docs/mcp-abi.md:103`; the language-side IR
//! contract is already pinned by Phase 2.

/// MCP server identifier published in the LSP-style initialise handshake.
pub fn server_name() -> &'static str {
    "lazuli-mcp"
}

/// IR schema version this MCP build understands. Tracks
/// `lazuli_ir::LZIR_SCHEMA`. Current major.minor: `0.5.x` (Cut A.7).
///
/// Read-tool consumers can compare this against their request's
/// expected schema to fail fast on a stale binary instead of returning
/// a payload that silently lacks the new Cut A / Cut A.7 fields.
pub const SUPPORTED_LZIR_SCHEMA: &str = lazuli_ir::LZIR_SCHEMA;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supported_schema_tracks_lazuli_ir() {
        assert_eq!(SUPPORTED_LZIR_SCHEMA, lazuli_ir::LZIR_SCHEMA);
    }

    #[test]
    fn schema_in_cut_a_series_range() {
        // Pin the version so a future cut that bumps LZIR_SCHEMA past
        // 0.x.y must also revisit the MCP wiring (read tools that
        // surface the Cut A / A.7 fields, the optional `tools` and
        // `expose` projections).
        assert!(SUPPORTED_LZIR_SCHEMA.starts_with("0."));
    }
}
