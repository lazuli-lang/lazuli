//! Template-rendering scaffolders for `lazuli new`.
//!
//! Two scaffold flavors:
//!
//! - `scaffold_bare` — writes a minimal `app.lzi` + `registry.lzi` +
//!   `README.md` + `.gitignore` + `features/.gitkeep`. Intended for
//!   experienced users who want to author the project skeleton by
//!   hand.
//! - `scaffold_from_template` — walks an embedded `include_dir::Dir`
//!   (currently `templates::DEFAULT_TEMPLATE`) and writes each file,
//!   substituting `{{app_name}}`, `{{app_slug}}`, and `{{module}}` in
//!   `.tmpl` files (the `.tmpl` suffix is stripped on output).
//!
//! `pascal_case_project_name` derives the project's PascalCase app
//! name from its directory name (`my-app` → `MyApp`); used by both
//! flavors and re-exported through `commands::new::mod` for tests.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::{
    GITIGNORE_TEMPLATE, REGISTRY_TEMPLATE,
    casing::{pascal_case, to_kebab_case, to_snake_case},
};

/// Write the minimal four-file project layout. No Go scaffolding, no
/// frontend, no migrations — just the IR-side anchor files.
pub(crate) fn scaffold_bare(project: &Path, app_name: &str) -> Result<()> {
    let features_dir = project.join("features");
    fs::create_dir_all(&features_dir)
        .with_context(|| format!("failed to create directory {}", features_dir.display()))?;

    write_scaffold_file(&project.join("app.lzi"), &app_template(app_name))?;
    write_scaffold_file(&project.join("registry.lzi"), REGISTRY_TEMPLATE)?;
    write_scaffold_file(&project.join("README.md"), &readme_template(app_name))?;
    write_scaffold_file(&project.join(".gitignore"), GITIGNORE_TEMPLATE)?;
    write_scaffold_file(&features_dir.join(".gitkeep"), "")?;
    Ok(())
}

/// Walk an embedded template directory and write every file (with
/// `.tmpl` substitution) into `target`. Recursive on `Dir` entries.
pub(crate) fn scaffold_from_template(
    template: &include_dir::Dir<'_>,
    target: &Path,
    app_name: &str,
    module: &str,
) -> Result<()> {
    // Snake_case slug for contexts that demand IDENT_LOWER (design
    // names, feature names, etc.). Templates emit verbatim — they can't
    // call helpers — so we precompute and substitute via `{{app_slug}}`.
    let app_slug = to_snake_case(app_name);
    for entry in template.entries() {
        match entry {
            include_dir::DirEntry::File(file) => {
                let mut out_path = target.join(file.path());
                let contents = if out_path.extension().and_then(|ext| ext.to_str()) == Some("tmpl")
                {
                    out_path.set_extension("");
                    file.contents_utf8()
                        .with_context(|| {
                            format!(
                                "template file is not valid UTF-8: {}",
                                file.path().display()
                            )
                        })?
                        .replace("{{app_name}}", app_name)
                        .replace("{{app_slug}}", &app_slug)
                        .replace("{{module}}", module)
                        .into_bytes()
                } else {
                    file.contents().to_vec()
                };

                if let Some(parent) = out_path.parent() {
                    fs::create_dir_all(parent).with_context(|| {
                        format!("failed to create directory {}", parent.display())
                    })?;
                }
                fs::write(&out_path, contents)
                    .with_context(|| format!("failed to write {}", out_path.display()))?;
            }
            include_dir::DirEntry::Dir(dir) => {
                let out_path = target.join(dir.path());
                fs::create_dir_all(&out_path).with_context(|| {
                    format!("failed to create directory {}", out_path.display())
                })?;
                scaffold_from_template(dir, target, app_name, module)?;
            }
        }
    }
    Ok(())
}

/// Derive the project's Go module name from its directory name,
/// kebab-cased under the `lazuli/` namespace. Fallback when callers
/// don't pass `--module`.
pub(crate) fn default_module_name(project: &Path) -> String {
    let name = project
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("app");
    format!("lazuli/{}", to_kebab_case(name))
}

/// Convert the project's directory name into a PascalCase app name.
/// Tolerates kebab- and snake-case inputs (`my-app`/`my_app` →
/// `MyApp`). Returns an error when the directory name is non-UTF-8
/// or contains zero ASCII alphanumerics.
pub(crate) fn pascal_case_project_name(project: &Path) -> Result<String> {
    let Some(name) = project.file_name().and_then(|name| name.to_str()) else {
        bail!("project path must end in a valid UTF-8 project name");
    };

    let app_name = pascal_case(name);
    if app_name.is_empty() {
        bail!("project name must contain at least one ASCII alphanumeric character");
    }

    Ok(app_name)
}

/// `app.lzi` boilerplate emitted by `scaffold_bare`. Kept as a tiny
/// inline template so the bare flavor stays free of `include_dir`
/// dependencies.
pub(crate) fn app_template(app_name: &str) -> String {
    format!("app {app_name}\n  urls\n    dev: \"http://localhost:3000\"\n")
}

/// `README.md` boilerplate emitted by `scaffold_bare`. Pointer-only:
/// the canonical docs live in the lazuli repo; the bare scaffold isn't
/// a tutorial.
pub(crate) fn readme_template(app_name: &str) -> String {
    format!(
        "# {app_name}\n\nGenerated with `lazuli new`.\n\nSee the Lazuli docs: https://github.com/lazuli-lang/lazuli/tree/main/docs\n"
    )
}

/// Write a single scaffold file. Thin wrapper over `fs::write` that
/// formats the error to include the file path. Intentionally does NOT
/// `create_dir_all` — callers are expected to have already ensured
/// the parent exists (avoids accidental directory creation in places
/// like `scaffold_bare` where the layout is fixed).
pub(crate) fn write_scaffold_file(path: &Path, contents: &str) -> Result<()> {
    fs::write(path, contents).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}
