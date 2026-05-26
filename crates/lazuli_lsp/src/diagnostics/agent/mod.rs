//! Diagnostics for `agent <name>` declarations (Lazuli AI primitive).
//!
//! Covers five orthogonal checks, all file-local — cross-feature
//! reachability (tool target resolution, policy compatibility,
//! discriminator enum/record lookup) lives in `lazuli_cli::doctor`. The
//! LSP is the fast inner loop; doctor is the workspace pass.
//!
//! | Producer | Concern |
//! |---|---|
//! | [`agent_contract_diagnostics`] | The `agent` header demands `policy`, `output`, `model @llm.*`, `prompt`; plus shape checks for `temperature` / `top_p` / `max_tokens` / `seed`. |
//! | [`agent_tools_diagnostics`] | Each entry in `tools` is `@tool.<dotted>` or `[<feature>.]<kind>[.<sub>].<name>`. |
//! | [`agent_evals_diagnostics`] | `evals` children are `case <name>` blocks containing `requires` / `forbids` / `golden`; `eval` requires `temperature 0` + `seed`. |
//! | [`agent_discriminator_diagnostics`] | `discriminator` is a `record`-only field marker. |
//! | [`agent_expose_diagnostics`] | `expose http` slot shape + same-file collision check; GET + `output stream` warns. |
//!
//! Sub-concerns live in sibling modules; this `mod.rs` re-exports them
//! verbatim so the crate-root `pub(crate) use diagnostics::agent::*;`
//! line in `lib.rs` keeps every producer reachable at its original
//! `crate::<fn>` path.

use crate::leading_spaces;

mod contract;
mod discriminator;
mod evals;
mod expose;
mod tools;

pub(crate) use contract::agent_contract_diagnostics;
pub(crate) use discriminator::{agent_discriminator_diagnostics, contains_token};
pub(crate) use evals::{agent_evals_diagnostics, validate_eval_predicate_shape};
pub(crate) use expose::{
    LocalExpose, agent_expose_diagnostics, extract_path_slots, lsp_normalise_path,
};
pub(crate) use tools::{agent_tools_diagnostics, validate_tool_reference_shape};

/// Iterate every `agent <name>` block in the source, yielding the
/// header line index and the body slice (one-based inclusive on the
/// header, exclusive on the next sibling). The caller decides which
/// children to inspect. Shared helper for the file-local LSP checks.
pub(crate) fn iter_agent_blocks(source: &str) -> Vec<(usize, Vec<usize>)> {
    let lines: Vec<&str> = source.lines().collect();
    let mut blocks: Vec<(usize, Vec<usize>)> = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        let line = lines[index];
        let trimmed = line.trim_start();
        let leading = leading_spaces(line);
        if leading == 2 && trimmed.starts_with("agent ") {
            let header = index;
            let mut body = Vec::new();
            index += 1;
            while index < lines.len() {
                let inner = lines[index];
                let inner_trimmed = inner.trim_start();
                if inner_trimmed.is_empty() || inner_trimmed.starts_with('#') {
                    body.push(index);
                    index += 1;
                    continue;
                }
                if leading_spaces(inner) <= 2 {
                    break;
                }
                body.push(index);
                index += 1;
            }
            blocks.push((header, body));
            continue;
        }
        index += 1;
    }
    blocks
}
