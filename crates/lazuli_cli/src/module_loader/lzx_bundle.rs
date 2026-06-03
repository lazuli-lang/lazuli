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
/// `Feature` and attach the lowered `Surface` records. A missing file is
/// not an error (a feature need not declare a surface); read / parse /
/// lower failures are reported to stderr but do not fail the build.
///
/// `.{web,mobile}.lzx` files are authored in one of two surface dialects:
///
/// * the **feature-ViewModel dialect** (`uses feature`, self-contained
///   `view list|detail|create <name>` with a `source <feature>.query.<name>`
///   line) — parsed by [`lazuli_syntax::parse_surface_document`], lowered by
///   [`lazuli_analyzer::lower_surface`], and emitted as per-view React
///   components by `lazuli_codegen_ts::lzx::emit_surface_views`. THIS is the
///   collection this function fills (`feature.surfaces`).
/// * the **experience/app dialect** (`uses experience`, `audience <name>
///   [qualifiers]`, thin `view <name> <type>` projections) — parsed by
///   [`lazuli_syntax::parse_lzx_document`] and consumed by the *app* pipeline
///   (routes, route-guards, Playwright fixtures) via `collect_lzx_bundle` /
///   `collect_lzx_experience_module`. Those surfaces do NOT carry the per-view
///   `source`/`submit` binding that component emission requires, so they are
///   not attached here.
///
/// Before this method threaded the dialect apart, an experience-dialect file
/// fed to the feature-dialect parser failed with a misleading `uses feature`
/// Pest error and was skipped to stderr — a SILENT surface loss for anyone who
/// authored components in the experience dialect by mistake. Now the mismatch
/// is diagnosed precisely (which dialect the file is in + what the
/// component-emitting dialect needs) instead of vanishing.
pub(super) fn attach_lzx_surfaces(input: &Path, module: &mut lazuli_ir::Module) {
    // Honor `[lazurite] app_dir` (e.g. pauta's `app_dir = "app"` puts
    // features at `app/features`, not `./features`). `resolve_in_app_dir`
    // falls back to `input/features` when no manifest / app_dir is set, so
    // root-layout projects are unchanged. Without this, the whole surface
    // attachment was a silent no-op for every app-dir project — no per-view
    // component ever generated, regardless of dialect.
    let features_root = crate::lazurite_manifest::resolve_in_app_dir(input, "features");
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
                    eprintln!("{}", classify_surface_parse_failure(&path, &source, &err));
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

/// Build the diagnostic shown when the feature-ViewModel parser
/// ([`lazuli_syntax::parse_surface_document`]) rejects a `.{web,mobile}.lzx`.
///
/// If the file actually parses as the **experience/app dialect**
/// ([`lazuli_syntax::parse_lzx_document`] yields >= 1 `surface`), the original
/// "`uses feature` …" Pest error is misleading — the file is simply in the
/// other dialect. Emit a precise, actionable message that (a) names the
/// dialect, (b) confirms the surface is NOT lost for routing/guards/fixtures,
/// and (c) explains what the component-emitting dialect requires. Otherwise the
/// file is genuinely malformed in BOTH dialects, so surface the feature-dialect
/// parse error verbatim (it points at the offending span).
fn classify_surface_parse_failure(
    path: &Path,
    source: &str,
    feature_dialect_err: &lazuli_syntax::ParseError,
) -> String {
    match lazuli_syntax::parse_lzx_document(source) {
        Ok(doc) if !doc.surfaces.is_empty() => format!(
            "lazuli: {p}: no per-view components generated — this file is authored in the \
             experience/app `.lzx` dialect (`uses experience` + `audience` + thin `view <name> \
             <type>` projections), which the app pipeline already consumes for routes, \
             route-guards, and Playwright fixtures, but which carries no per-view `source` / \
             `submit` binding to emit React view hooks from.\n  \
             To ALSO generate per-view components for this feature, author the views in the \
             feature-ViewModel dialect: `uses feature {feat}` and self-contained \
             `view list|detail|create <name>` blocks, each with a `source {feat}.query.<name>` \
             (list/detail) or `submit {feat}.command.<name>` (create) line.\n  \
             (Routing/guards for this surface are unaffected; only the per-view component hooks \
             under `dist/ts-*/{feat}/views/` are skipped.)",
            p = path.display(),
            feat = feature_name_of(path),
        ),
        _ => format!(
            "lazuli: skipping {}: surface parse failed: {:?}",
            path.display(),
            feature_dialect_err
        ),
    }
}

/// `features/<feat>/<feat>.web.lzx` -> `<feat>` (the file stem before the
/// `.<target>.lzx` suffix). Falls back to the parent dir name, then the stem.
fn feature_name_of(path: &Path) -> String {
    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
        // Strip the trailing `.web.lzx` / `.mobile.lzx`; the feature name is
        // everything before the first dot.
        if let Some((stem, _)) = name.split_once('.') {
            return stem.to_owned();
        }
    }
    path.parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("<feature>")
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal, known-good **feature-ViewModel** `.web.lzx` — the dialect
    /// `parse_surface_document` + `lower_surface` + `emit_surface_views`
    /// understand. Mirrors the `parse_surface_document` doctest shape.
    const FEATURE_DIALECT_WEB: &str = "surface widget web\n  uses feature widget\n  audience admin\n    view list all\n      source widget.query.mine\n      columns name\n";

    /// The **experience/app** dialect the pauta pilots author (`uses
    /// experience`, `audience <name> [qualifiers]`, thin `view <name> <type>`
    /// projections without a per-view `source`). Parses via
    /// `parse_lzx_document` but NOT via `parse_surface_document`. Shape copied
    /// from `pauta media_vehicles.web.lzx`.
    const EXPERIENCE_DIALECT_WEB: &str = "surface widget web\n  uses experience widget\n\n  audience agency_admin as @role.ADMIN\n    view list Table\n      columns trade_name, category\n      search trade_name\n    view create Form\n      fields trade_name\n";

    /// Positive control: the feature-ViewModel dialect still parses with
    /// `parse_surface_document` and lowers with `lower_surface` — i.e. the
    /// happy path `attach_lzx_surfaces` drives (parse -> lower -> push) is
    /// intact, so the diagnostic cases below are about the *other* dialect, not
    /// a regression in component emission.
    #[test]
    fn feature_dialect_web_lzx_still_parses_and_lowers() {
        let ast = lazuli_syntax::parse_surface_document(FEATURE_DIALECT_WEB)
            .expect("feature-dialect .web.lzx must parse");
        let surface = lazuli_analyzer::lower_surface(&ast)
            .expect("feature-dialect .web.lzx must lower to a component Surface");
        assert_eq!(surface.audiences.len(), 1);
        assert_eq!(surface.audiences[0].views.len(), 1);
    }

    /// The gap: an experience-dialect `.web.lzx` is NOT silently dropped.
    /// `classify_surface_parse_failure` returns the precise, actionable
    /// dialect-mismatch diagnostic instead of the misleading `uses feature`
    /// Pest error the feature-dialect parser produces.
    #[test]
    fn experience_dialect_web_lzx_yields_precise_diagnostic_not_silent_skip() {
        // Sanity: the experience dialect really does fail the feature-dialect
        // parser (that's the trigger for the classifier) while parsing fine as
        // an experience/app document with a surface.
        let err = lazuli_syntax::parse_surface_document(EXPERIENCE_DIALECT_WEB)
            .expect_err("experience dialect must fail the feature-dialect parser");
        let doc = lazuli_syntax::parse_lzx_document(EXPERIENCE_DIALECT_WEB)
            .expect("experience dialect must parse as a .lzx document");
        assert_eq!(doc.surfaces.len(), 1, "the document carries one surface");

        let path = std::path::PathBuf::from("features/widget/widget.web.lzx");
        let msg = classify_surface_parse_failure(&path, EXPERIENCE_DIALECT_WEB, &err);
        assert!(
            msg.contains("experience/app `.lzx` dialect"),
            "diagnostic must name the experience dialect, got: {msg}"
        );
        assert!(
            msg.contains("uses feature widget"),
            "diagnostic must tell the author the feature-ViewModel form to switch to, got: {msg}"
        );
        assert!(
            !msg.contains("surface parse failed"),
            "experience-dialect file must NOT fall back to the misleading raw parse error, got: {msg}"
        );
    }

    /// A file that is malformed in BOTH dialects keeps the verbatim
    /// feature-dialect parse error (which points at the offending span) —
    /// the classifier only special-cases genuine experience-dialect files.
    #[test]
    fn garbage_web_lzx_keeps_feature_dialect_parse_error() {
        let garbage = "surface widget web\n  this is not any dialect\n";
        let path = std::path::PathBuf::from("features/widget/widget.web.lzx");
        let err = lazuli_syntax::parse_surface_document(garbage)
            .expect_err("garbage must fail the feature-dialect parser");
        let msg = classify_surface_parse_failure(&path, garbage, &err);
        assert!(
            msg.contains("surface parse failed"),
            "genuinely-malformed file must surface the raw parse error, got: {msg}"
        );
    }

    #[test]
    fn feature_name_of_strips_target_suffix() {
        assert_eq!(
            feature_name_of(std::path::Path::new(
                "features/media_vehicles/media_vehicles.web.lzx"
            )),
            "media_vehicles"
        );
        assert_eq!(
            feature_name_of(std::path::Path::new("a/b/supplier.mobile.lzx")),
            "supplier"
        );
    }
}
