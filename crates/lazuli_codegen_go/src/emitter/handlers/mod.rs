//! Starter stub emission for user-authored Go extension handlers.
//!
//! This walker finds `@fn.<name>` and `@hook.<name>` references that are
//! already surfaced in the IR and emits one non-`.gen.go` starter file per
//! feature/name pair:
//!
//! ```text
//! app/features/<feature>/handlers/<name>.go
//! ```
//!
//! Starter stubs live in a dedicated user-authored handler package
//! (`package <feature>handlers`). They import generated feature
//! contracts from the generated Go module and self-register via
//! `lazuli.RegisterFn`, which keeps generated feature packages free of
//! imports back into user code.
//!
//! The files are intentionally user territory. Callers pass the files already
//! present under the Go output tree; this module skips any matching path so
//! regeneration never overwrites user-authored code. The orchestrator remains
//! responsible for performing the same check at write time.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use lazuli_ir::Module;

use crate::GeneratedFile;

mod collect;
mod emit;
mod paths;
mod types;

use collect::collect_handler_stubs;
#[cfg(test)]
use collect::{HandlerRef, extract_handler_refs};
use emit::emit_stub_contents;
#[allow(unused_imports)]
pub(crate) use paths::APP_FEATURES_PREFIX;
#[cfg(test)]
use paths::exported_func_name;
use paths::{handler_path, path_exists};
#[cfg(test)]
use types::go_type_for_stub;

/// Emit starter stubs for every `@fn.*` and `@hook.*` reference in the IR.
///
/// Paths returned in `GeneratedFile.path` are project-root-relative
/// (`app/features/<feature>/handlers/<name>.go`) — the orchestrator detects
/// the `app/features/` prefix and writes to project root, not to the codegen
/// `out_dir`. This is the Tier 1 portable home defined in
/// `docs/project-structure.md`.
///
/// `module_name` is the user's Go module path (e.g.
/// `github.com/myorg/myapp/generated` or `lazuli/myapp`) — used to construct
/// the import path `<module_name>/<feature>` so user handlers can reference
/// generated types via `<feature>gen.<TypeName>`.
///
/// `existing_files` covers both the new app/features path and the
/// legacy dist/go path (pre-pivot scaffolds) so migration is non-
/// destructive — handlers authored at either location are skipped.
pub fn emit_handler_stubs(
    module: &Module,
    module_name: &str,
    existing_files: &BTreeSet<PathBuf>,
) -> Vec<GeneratedFile> {
    let stubs = collect_handler_stubs(module);

    stubs
        .into_values()
        .filter_map(|stub| {
            let path = handler_path(&stub.feature, &stub.name);
            if path_exists(existing_files, &path) {
                return None;
            }
            Some(GeneratedFile {
                path,
                contents: emit_stub_contents(&stub, module_name),
            })
        })
        .collect()
}

/// Set of feature names that have at least one user-authored handler
/// the runtime registry needs to resolve. Drives `main.go`'s anonymous
/// import block for handler packages — features without handlers don't
/// have an `app/features/<f>/handlers/` directory on disk, so importing
/// them anyway would fail `go build` with "package not found". Walks
/// the same IR sites `emit_handler_stubs` does so the two stay in sync.
pub fn features_with_handlers(module: &Module) -> BTreeSet<String> {
    collect_handler_stubs(module)
        .into_values()
        .map(|stub| stub.feature)
        .collect()
}

// ----------------------------------------------------------------------------
// Carrier shapes — shared across collect/emit. They live here (in the parent
// module) because both sibling modules consume them; centralising the
// definitions also keeps `pub(super)` field visibility tight to the
// `handlers/` sub-tree.
// ----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct StubKey {
    pub(super) feature: String,
    pub(super) path_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct HandlerStub {
    pub(super) feature: String,
    pub(super) namespace: HandlerNamespace,
    pub(super) name: String,
    pub(super) site: String,
    pub(super) input_type: String,
    pub(super) output_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct HandlerSignature {
    pub(super) input_type: String,
    pub(super) output_type: String,
}

impl HandlerSignature {
    pub(super) fn any() -> Self {
        Self {
            input_type: "any".to_owned(),
            output_type: "any".to_owned(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum HandlerNamespace {
    Fn,
    Hook,
}

impl HandlerNamespace {
    pub(super) fn prefix(self) -> &'static str {
        match self {
            HandlerNamespace::Fn => "@fn.",
            HandlerNamespace::Hook => "@hook.",
        }
    }
}

pub(super) type SignatureKey = (String, HandlerNamespace, String);
pub(super) type SignatureMap = BTreeMap<SignatureKey, HandlerSignature>;


#[cfg(test)]
mod tests;
