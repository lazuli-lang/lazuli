//! `.lzx` bundling + per-feature surface attachment.
//!
//! `LzxBundle` is the lifted view returned by `collect_lzx_bundle` —
//! aggregates every `.lzx` file under a project root into the four
//! surface-IR collections (`app`, `routes`, `experiences`,
//! `surfaces`). `attach_lzx_surfaces` is the L0 #3 wiring that pairs
//! `features/<feat>/<feat>.web.lzx` and `.mobile.lzx` with the matching
//! `Feature` IR.
//!
//! Lifted out of the `module_loader` god-file in the rails-style R9
//! split.

use std::fs;
use std::path::Path;

use super::collectors::collect_package_lzx_files;

#[derive(Default)]
pub(crate) struct LzxBundle {
    pub(crate) app: Option<lazuli_ir::AppManifest>,
    pub(crate) routes: Vec<lazuli_ir::AppRoute>,
    pub(crate) experiences: Vec<lazuli_ir::Experience>,
    pub(crate) surfaces: Vec<lazuli_ir::PlatformSurface>,
}

pub(crate) fn collect_lzx_bundle(input: &Path) -> LzxBundle {
    let mut files = Vec::new();
    if input.is_dir() {
        let _ = collect_package_lzx_files(input, &mut files);
    } else if input.extension().and_then(|s| s.to_str()) == Some("lzx") {
        files.push(input.to_path_buf());
    }
    files.sort();

    let mut bundle = LzxBundle::default();
    for path in files {
        let source = match fs::read_to_string(&path) {
            Ok(source) => source,
            Err(err) => {
                eprintln!(
                    "lazuli: skipping {}: read failed: {:?}",
                    path.display(),
                    err
                );
                continue;
            }
        };
        let document = match lazuli_syntax::parse_lzx_document(&source) {
            Ok(document) => document,
            Err(err) => {
                eprintln!(
                    "lazuli: skipping {}: lzx parse failed: {:?}",
                    path.display(),
                    err
                );
                continue;
            }
        };
        let lowered = lazuli_analyzer::lower_lzx_document(&document);
        if bundle.app.is_none() {
            bundle.app = lowered.app;
        }
        bundle.routes.extend(lowered.routes);
        bundle.experiences.extend(lowered.experiences);
        bundle.surfaces.extend(lowered.surfaces);
    }
    bundle
}

/// L0 #3 — look for `features/<feature>/<feature>.web.lzx` and
/// `features/<feature>/<feature>.mobile.lzx` next to each parsed
/// `Feature` and attach the lowered `Surface` records. Missing files
/// are silently skipped; parse / lower errors are reported but do not
/// fail the build.
pub(super) fn attach_lzx_surfaces(input: &Path, module: &mut lazuli_ir::Module) {
    let features_root = input.join("features");
    if !features_root.is_dir() {
        return;
    }
    for feature in module.features.iter_mut() {
        let feat_dir = features_root.join(&feature.name);
        if !feat_dir.is_dir() {
            continue;
        }
        for (target_label, parsed_target) in [
            ("web", lazuli_syntax::SurfaceTargetAst::Web),
            ("mobile", lazuli_syntax::SurfaceTargetAst::Mobile),
        ] {
            let path = feat_dir.join(format!("{}.{}.lzx", feature.name, target_label));
            if !path.is_file() {
                continue;
            }
            let source = match fs::read_to_string(&path) {
                Ok(s) => s,
                Err(err) => {
                    eprintln!(
                        "lazuli: skipping {}: read failed: {:?}",
                        path.display(),
                        err
                    );
                    continue;
                }
            };
            let ast = match lazuli_syntax::parse_surface_document(&source) {
                Ok(ast) => ast,
                Err(err) => {
                    eprintln!(
                        "lazuli: skipping {}: surface parse failed: {:?}",
                        path.display(),
                        err
                    );
                    continue;
                }
            };
            if ast.target != parsed_target {
                eprintln!(
                    "lazuli: skipping {}: surface target `{:?}` does not match filename target `{}`",
                    path.display(),
                    ast.target,
                    target_label,
                );
                continue;
            }
            match lazuli_analyzer::lower_surface(&ast) {
                Ok(surface) => feature.surfaces.push(surface),
                Err(err) => eprintln!(
                    "lazuli: skipping {}: surface lower failed: {:?}",
                    path.display(),
                    err
                ),
            }
        }
    }
}
