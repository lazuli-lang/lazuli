//! Per-view mobile scaffold writer for `lazuli generate ts`.
//!
//! For every `surface … target: mobile` audience/view tuple in the IR
//! this module writes one `app/clients/mobile/app/<audience>/<expo-route>.tsx`
//! placeholder under the project root. Idempotent: an existing file is
//! left untouched so author edits survive subsequent
//! `lazuli generate ts` invocations. The placeholder body comes from
//! `lazuli_codegen_ts::mobile_view_scaffold::scaffold_body_for_view`;
//! we own only the walk and the I/O guard here.
//!
//! Driven by `docs/proposals/mobile-target.md` §5.2 ("one Expo file
//! per declared view"). Routes are derived from `view_route_string`,
//! which mirrors `lzx::view_route_string` — kept duplicated so the CLI
//! has no inbound dependency on the lzx crate's internals.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

/// Walk every mobile surface and write a per-view scaffold under
/// `app/clients/mobile/app/<audience>/<expo-route>.tsx`. Returns the count
/// of files actually written (excludes already-present files left
/// untouched by the `write_if_absent` guard).
pub(super) fn scaffold_mobile_view_files(
    module: &lazuli_ir::Module,
    out_dir: &Path,
) -> Result<usize> {
    let mut written = 0usize;

    for feature in &module.features {
        for surface in &feature.surfaces {
            if !matches!(surface.target, lazuli_ir::SurfaceTarget::Mobile) {
                continue;
            }
            for audience in &surface.audiences {
                for view in &audience.views {
                    let route = view_route_string(view);
                    let path = lazuli_codegen_ts::mobile_view_scaffold::expo_app_file_path(
                        &audience.name,
                        &route,
                    );
                    let abs_path = out_dir.join(&path);
                    if abs_path.exists() {
                        continue;
                    }
                    let body = lazuli_codegen_ts::mobile_view_scaffold::scaffold_body_for_view(
                        &surface.feature,
                        &audience.name,
                        view,
                    );
                    if let Some(parent) = abs_path.parent() {
                        fs::create_dir_all(parent).with_context(|| {
                            format!("creating {} for mobile scaffold", parent.display())
                        })?;
                    }
                    fs::write(&abs_path, body).with_context(|| {
                        format!("writing mobile scaffold {}", abs_path.display())
                    })?;
                    written += 1;
                }
            }
        }
    }

    Ok(written)
}

/// Extract the `at "<path>"` string from a view declaration. Stored as
/// `route: Option<String>` on each view kind in the IR. Falls back to
/// `/` for views that omit the clause entirely (rare — Expo Router's
/// `app/<audience>/index.tsx` is the natural landing target).
fn view_route_string(view: &lazuli_ir::View) -> String {
    match view {
        lazuli_ir::View::List(v) => v.route.clone().unwrap_or_else(|| "/".to_owned()),
        lazuli_ir::View::Detail(v) => v.route.clone().unwrap_or_else(|| "/".to_owned()),
        lazuli_ir::View::Create(v) => v.route.clone().unwrap_or_else(|| "/".to_owned()),
    }
}
