//! Mobile (Expo Router) frontend scaffold per L0 #1 §6.1.
//!
//! Mobile is always a `clients/` slice (not the default `app/web/`)
//! because it targets a different runtime — per the singular-vs-plural
//! rule in `docs/project-structure.md`, different runtimes always get
//! separate clients. So this writer emits at `app/clients/mobile/`.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

use crate::templates;

use super::internals::{append_manifest_block, ensure_dir, write_if_absent};

/// Scaffold the mobile (Expo Router) frontend skeleton. Idempotent.
///
/// Mobile is always a `clients/` slice (not the default `app/web/`)
/// because it targets a different runtime — per the singular-vs-plural
/// rule in `docs/project-structure.md`, different runtimes always get
/// separate clients. So this writer emits at `app/clients/mobile/`.
///
/// Creates (when missing) under `app/clients/mobile/`:
/// - `app/_layout.tsx` — one-line re-export of the regen body that
///   `lazuli generate ts` writes to `dist/ts-mobile/runtime/layout`
///   (per `docs/proposals/mobile-target.md` §5.4).
/// - `app/index.tsx` — placeholder home; user customizes once a
///   `surface ... mobile` view scaffolds a real route file.
/// - `shell/client.ts` — `LazuliClient` construction (baseUrl from
///   `process.env.EXPO_PUBLIC_API_URL`).
/// - `app.json` — Expo manifest (name, slug, scheme, expo-router plugin).
/// - `babel.config.js`, `metro.config.js`, `tsconfig.json` — Expo
///   project plumbing.
/// - `package.json` — Expo SDK, expo-router, react-native,
///   AsyncStorage, `@lazuli/runtime` workspace dep.
/// - `.gitignore` (project root) gains Expo-specific ignores.
///
/// Appends `[frontends.mobile]` to `Lazurite.toml` if no such block
/// exists.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_cli::cmd_new_frontends::mobile::scaffold_frontend_mobile;
///
/// // scaffold_frontend_mobile(Path::new("."), "the canonical pilot")?;
/// ```
pub fn scaffold_frontend_mobile(project_root: &Path, _app_name: &str) -> Result<()> {
    ensure_dir(project_root)?;

    let mobile_dir = project_root.join("app").join("clients").join("mobile");
    let app_dir = mobile_dir.join("app");
    let shell_dir = mobile_dir.join("shell");
    fs::create_dir_all(&app_dir).with_context(|| format!("creating {}", app_dir.display()))?;
    fs::create_dir_all(&shell_dir).with_context(|| format!("creating {}", shell_dir.display()))?;

    write_if_absent(
        &app_dir.join("_layout.tsx"),
        templates::FRONTEND_MOBILE_APP_LAYOUT_TSX,
    )?;
    write_if_absent(
        &app_dir.join("index.tsx"),
        templates::FRONTEND_MOBILE_APP_INDEX_TSX,
    )?;
    write_if_absent(
        &shell_dir.join("client.ts"),
        templates::FRONTEND_MOBILE_SHELL_CLIENT_TS,
    )?;

    write_if_absent(
        &mobile_dir.join("app.json"),
        templates::FRONTEND_MOBILE_APP_JSON,
    )?;
    write_if_absent(
        &mobile_dir.join("babel.config.js"),
        templates::FRONTEND_MOBILE_BABEL_CONFIG,
    )?;
    write_if_absent(
        &mobile_dir.join("metro.config.js"),
        templates::FRONTEND_MOBILE_METRO_CONFIG,
    )?;
    write_if_absent(
        &mobile_dir.join("tsconfig.json"),
        templates::FRONTEND_MOBILE_TSCONFIG,
    )?;
    write_if_absent(
        &mobile_dir.join("package.json"),
        templates::FRONTEND_MOBILE_PACKAGE_JSON,
    )?;

    // Project-level .gitignore: prefer the shared web template when
    // present (it already covers node_modules/, dist/, .vite/, etc.);
    // append Expo-specific ignores idempotently.
    let gitignore_path = project_root.join(".gitignore");
    if !gitignore_path.exists() {
        fs::write(&gitignore_path, templates::FRONTEND_GITIGNORE)
            .with_context(|| format!("writing {}", gitignore_path.display()))?;
    }
    append_manifest_block(
        &gitignore_path,
        "# Expo",
        templates::FRONTEND_MOBILE_GITIGNORE,
    )?;

    append_manifest_block(
        &project_root.join("Lazurite.toml"),
        "[frontends.mobile]",
        templates::FRONTEND_MANIFEST_MOBILE_SNIPPET,
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use super::super::testing::tempdir;

    #[test]
    fn scaffold_mobile_creates_expected_files_and_appends_manifest() {
        let project = tempdir();
        let root = project.path();

        scaffold_frontend_mobile(root, "demo").unwrap();

        // Expo Router app/ tree.
        assert!(
            root.join("app/clients/mobile/app/_layout.tsx").exists(),
            "mobile app/_layout.tsx missing"
        );
        assert!(
            root.join("app/clients/mobile/app/index.tsx").exists(),
            "mobile app/index.tsx missing"
        );
        // LazuliClient construction lives under shell/.
        assert!(
            root.join("app/clients/mobile/shell/client.ts").exists(),
            "mobile shell/client.ts missing"
        );
        // Expo project plumbing.
        assert!(
            root.join("app/clients/mobile/app.json").exists(),
            "app.json missing"
        );
        assert!(
            root.join("app/clients/mobile/babel.config.js").exists(),
            "babel.config.js missing"
        );
        assert!(
            root.join("app/clients/mobile/metro.config.js").exists(),
            "metro.config.js missing"
        );
        assert!(
            root.join("app/clients/mobile/tsconfig.json").exists(),
            "tsconfig.json missing"
        );
        assert!(
            root.join("app/clients/mobile/package.json").exists(),
            "mobile package.json missing"
        );

        // _layout.tsx is the user-owned re-export of the regen body.
        let layout = fs::read_to_string(root.join("app/clients/mobile/app/_layout.tsx")).unwrap();
        assert!(layout.contains("dist/ts-mobile/runtime/layout"));

        let manifest = fs::read_to_string(root.join("Lazurite.toml")).unwrap();
        assert!(manifest.contains("[frontends.mobile]"));
        assert!(manifest.contains("target = \"expo\""));
        assert!(manifest.contains("source = \"app/clients/mobile\""));
        assert!(manifest.contains("out = \"dist/ts-mobile\""));

        let pkg = fs::read_to_string(root.join("app/clients/mobile/package.json")).unwrap();
        assert!(pkg.contains("\"expo\""));
        assert!(pkg.contains("\"react-native\""));
        assert!(pkg.contains("\"expo-router\""));
        assert!(pkg.contains("\"@react-native-async-storage/async-storage\""));
        assert!(pkg.contains("\"@lazuli/runtime\""));

        // Wave G — M2-M6 mobile Tier-2 picks present.
        assert!(pkg.contains("\"expo-secure-store\""), "M3-secrets missing");
        assert!(
            pkg.contains("\"react-native-reanimated\""),
            "M4 Reanimated missing"
        );
        assert!(
            pkg.contains("\"expo-notifications\""),
            "M5 expo-notifications missing"
        );
        assert!(
            pkg.contains("\"lucide-react-native\""),
            "M2 lucide-react-native missing"
        );
        assert!(
            pkg.contains("\"zustand\""),
            "M6 Zustand (inherits W1) missing"
        );

        // .gitignore covers Expo-specific paths.
        let gitignore = fs::read_to_string(root.join(".gitignore")).unwrap();
        assert!(gitignore.contains(".expo/"));

        // babel.config.js must list `react-native-reanimated/plugin` LAST
        // (M4 pairing rule: Reanimated requires its plugin to be last).
        let babel = fs::read_to_string(root.join("app/clients/mobile/babel.config.js")).unwrap();
        assert!(
            babel.contains("react-native-reanimated/plugin"),
            "babel.config.js must include reanimated plugin"
        );

        // app.json plugin entries — expo-notifications must be present.
        let app_json = fs::read_to_string(root.join("app/clients/mobile/app.json")).unwrap();
        assert!(
            app_json.contains("expo-notifications"),
            "app.json must list expo-notifications plugin (M5 pairing rule)"
        );
    }

    #[test]
    fn scaffold_mobile_is_idempotent() {
        let project = tempdir();
        let root = project.path();

        scaffold_frontend_mobile(root, "demo").unwrap();
        let edited = "// user-customized mobile layout\n";
        fs::write(root.join("app/clients/mobile/app/_layout.tsx"), edited).unwrap();

        scaffold_frontend_mobile(root, "demo").unwrap();

        let after = fs::read_to_string(root.join("app/clients/mobile/app/_layout.tsx")).unwrap();
        assert_eq!(after, edited);

        let manifest = fs::read_to_string(root.join("Lazurite.toml")).unwrap();
        assert_eq!(manifest.matches("[frontends.mobile]").count(), 1);

        // .gitignore Expo block is only appended once.
        let gitignore = fs::read_to_string(root.join(".gitignore")).unwrap();
        assert_eq!(gitignore.matches("# Expo").count(), 1);
    }
}
