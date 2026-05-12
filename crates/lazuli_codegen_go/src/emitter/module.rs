//! Top-level emitter walker. Drives per-feature emission and the root
//! `go.mod`. For cell E1 the per-feature output is intentionally
//! empty (just banner + `package` directive); per-kind walkers
//! (Resource, Command, Query, …) land in E2-E4 and G1-G7.
//!
//! Per proposal §2.4 every iteration is deterministic — features are
//! sorted by name before emission so output is byte-equivalent across
//! runs regardless of feature insertion order at the IR layer.

use std::collections::BTreeMap;

use lazuli_ir::Module;

use super::enums::emit_enum_file;
use super::imports::ImportSet;
use super::printer::GoPrinter;
use super::resource::emit_resource_file;
use crate::{GeneratedFile, GoEmitOptions, LAZULI_GO_VERSION};

/// Default Go module path used when the caller did not supply one and
/// the IR exposes no `app.name`. Matches proposal §1.1's "fallback
/// `lazuli/app`" rule.
const DEFAULT_MODULE_NAME: &str = "lazuli/app";

/// Default Go toolchain pin emitted into `go.mod`. Matches
/// `runtime/go/go.mod` (currently `go 1.25.0`) so the generated
/// module shares the same toolchain expectation as the hand-written
/// Lazuli Go library.
const DEFAULT_GO_TOOLCHAIN: &str = "go 1.25";

/// Walk the IR module and produce every `.gen.go` plus the root
/// `go.mod`. Per cell E1 this only emits the file skeleton; kinds
/// land in subsequent cells.
pub fn emit_module(module: &Module, options: &GoEmitOptions) -> Vec<GeneratedFile> {
    let module_name = resolve_module_name(module, options);
    let lazuli_go_version = if options.lazuli_go_version.is_empty() {
        LAZULI_GO_VERSION.to_owned()
    } else {
        options.lazuli_go_version.clone()
    };

    // BTreeMap so the iteration order is deterministic regardless of
    // how features were inserted into the IR `Vec`. Feature names
    // are unique per module.
    let mut features: BTreeMap<&str, &lazuli_ir::Feature> = BTreeMap::new();
    for feature in &module.features {
        features.insert(feature.name.as_str(), feature);
    }

    let mut files = Vec::with_capacity(features.len() + 1);

    // Root `go.mod` first so byte-comparison fixtures find it at
    // index 0.
    files.push(GeneratedFile {
        path: "go.mod".to_owned(),
        contents: emit_go_mod(&module_name, &lazuli_go_version),
    });

    let source_label = resolve_source_label(module);
    for feature in features.values() {
        let path = format!("{name}/{name}.gen.go", name = feature.name);
        let contents = emit_feature_stub(&source_label, &feature.name);
        files.push(GeneratedFile { path, contents });

        // Cell E2 — `Resource` + `Record` emission lands in a sibling
        // file. Features that declare neither skip the file entirely
        // (an empty body would leave a stray `package <feature>` and
        // gofmt would tolerate it but the file would carry no signal).
        if let Some(contents) = emit_resource_file(&source_label, feature) {
            let resource_path = format!("{name}/resource.gen.go", name = feature.name);
            files.push(GeneratedFile {
                path: resource_path,
                contents,
            });
        }

        // Cell E2.5 — `EnumDecl` emission. Per-feature typed aliases
        // plus aligned const blocks land in a sibling `enum.gen.go`.
        // Skipped entirely when the feature declares no enums so the
        // output listing stays signal-rich.
        if let Some(contents) = emit_enum_file(&source_label, feature) {
            let enum_path = format!("{name}/enum.gen.go", name = feature.name);
            files.push(GeneratedFile {
                path: enum_path,
                contents,
            });
        }
    }

    files
}

fn resolve_module_name(module: &Module, options: &GoEmitOptions) -> String {
    if let Some(name) = options
        .module_name
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        return name.to_owned();
    }
    match module.app.as_ref() {
        Some(app) if !app.name.trim().is_empty() => format!("lazuli/{}", to_kebab(&app.name)),
        _ => DEFAULT_MODULE_NAME.to_owned(),
    }
}

fn resolve_source_label(module: &Module) -> String {
    match module.app.as_ref() {
        Some(app) => app.name.clone(),
        None => "lazuli module".to_owned(),
    }
}

fn emit_go_mod(module_name: &str, lazuli_go_version: &str) -> String {
    let mut p = GoPrinter::new();
    p.line(&format!("module {}", module_name));
    p.blank();
    p.line(DEFAULT_GO_TOOLCHAIN);
    p.blank();
    p.line("require (");
    p.indent();
    // The Lazuli Go lib publishes a single Go module at
    // `lazuli.dev/runtime`; per-bucket subpackages (`auth`, `storage`,
    // `jobs`, the top-level `lazuli` package, ...) live under it. The
    // `require` clause names the module, not the subpackage; generated
    // imports reference `lazuli.dev/runtime/lazuli` and
    // `lazuli.dev/runtime/lazuli/<bucket>` paths against that module.
    p.line(&format!("lazuli.dev/runtime {}", lazuli_go_version));
    p.dedent();
    p.line(")");
    p.finish()
}

fn emit_feature_stub(source: &str, feature_name: &str) -> String {
    let mut p = GoPrinter::new();
    p.banner(source, feature_name);
    // E1 stub: imports are recorded but unused because no kinds emit
    // yet. We deliberately do not produce an `import (...)` block
    // until the first kind walker (cell E2) introduces a real use —
    // a leading empty block would fail `gofmt`.
    let _placeholder = ImportSet::new();
    p.finish()
}

/// Lower-snake / lower-kebab caser shared with the CLI helper. Mirrors
/// `lazuli_codegen_go::to_kebab_case` (legacy demo) and the CLI's
/// `to_kebab_case` so the derived module name matches across surfaces.
fn to_kebab(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut prev_lower = false;
    for ch in value.chars() {
        if ch == '_' || ch == ' ' {
            out.push('-');
            prev_lower = false;
            continue;
        }
        if ch.is_ascii_uppercase() {
            if prev_lower && !out.is_empty() {
                out.push('-');
            }
            for low in ch.to_lowercase() {
                out.push(low);
            }
            prev_lower = false;
            continue;
        }
        out.push(ch);
        prev_lower = ch.is_ascii_lowercase() || ch.is_ascii_digit();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_kebab_lower_passthrough() {
        assert_eq!(to_kebab("hostpoint"), "hostpoint");
    }

    #[test]
    fn to_kebab_pascal_inserts_dashes() {
        assert_eq!(to_kebab("HostPoint"), "host-point");
    }

    #[test]
    fn to_kebab_handles_underscores_and_spaces() {
        assert_eq!(to_kebab("hello_world test"), "hello-world-test");
    }
}
