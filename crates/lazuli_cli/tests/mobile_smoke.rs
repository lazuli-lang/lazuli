//! End-to-end smoke test for the mobile-target pipeline.
//!
//! Per `docs/proposals/mobile-target.md` §8.1 + §11 cell E.2. Two
//! gates: the `smoke` Cargo feature + `LAZULI_TS_SMOKE=1` env var.
//! Default `cargo test` is unaffected.
//!
//! When both gates are set, the test:
//!
//! 1. Copies `examples/marketplace-mini-mobile/` into a fresh tempdir.
//! 2. Runs `lazuli generate ts` against the copy.
//! 3. Asserts every expected path is present under
//!    `dist/ts-mobile/` and `frontends/mobile/`.
//! 4. Asserts the regen body at `dist/ts-mobile/runtime/layout.tsx`
//!    matches the canonical template byte-for-byte.
//! 5. (Future) Shells out to `tsc --noEmit` against the fixture's
//!    `tsconfig.json`. Tracked separately because it requires `pnpm
//!    install` over the Expo SDK; current gating ships the path-level
//!    smoke so CI can adopt the test without yet wiring Node.

#[cfg(feature = "smoke")]
mod mobile_smoke {
    use std::env;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    fn cli_bin() -> PathBuf {
        PathBuf::from(env!("CARGO_BIN_EXE_lazuli"))
    }

    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf()
    }

    fn copy_dir(src: &Path, dst: &Path) {
        fs::create_dir_all(dst).unwrap();
        for entry in fs::read_dir(src).unwrap() {
            let entry = entry.unwrap();
            let name = entry.file_name();
            // Skip generated dirs so we measure fresh emission.
            if name == "dist" || name == ".lazuli" || name == "node_modules" {
                continue;
            }
            let path = entry.path();
            let target = dst.join(&name);
            if entry.file_type().unwrap().is_dir() {
                copy_dir(&path, &target);
            } else {
                fs::copy(&path, &target).unwrap();
            }
        }
    }

    #[test]
    fn marketplace_mini_mobile_emits_expected_paths() {
        if env::var("LAZULI_TS_SMOKE").ok().as_deref() != Some("1") {
            eprintln!("skipping mobile smoke; set LAZULI_TS_SMOKE=1 to run");
            return;
        }

        // Working copy in target/ so we don't pollute the repo's
        // gitignored dist for the on-disk fixture.
        let work = repo_root()
            .join("target")
            .join("lazuli-mobile-smoke")
            .join(format!("{}", std::process::id()));
        let _ = fs::remove_dir_all(&work);
        copy_dir(
            &repo_root().join("examples").join("marketplace-mini-mobile"),
            &work,
        );

        let status = Command::new(cli_bin())
            .current_dir(&work)
            .args(["generate", "ts", "."])
            .status()
            .expect("run lazuli generate ts");
        assert!(
            status.success(),
            "lazuli generate ts must exit 0; got {status:?}"
        );

        // Layout body must match the canonical template byte-for-byte
        // (the regen body is deterministic — no project substitutions).
        let layout = fs::read_to_string(work.join("dist/ts-mobile/runtime/layout.tsx"))
            .expect("dist/ts-mobile/runtime/layout.tsx must exist");
        assert_eq!(
            layout,
            lazuli_codegen_ts::mobile_runtime::MOBILE_RUNTIME_LAYOUT_TSX,
            "regen layout.tsx must match canonical template byte-for-byte"
        );

        // SDK + Zod artifacts for both features.
        for feature in ["catalog", "booking"] {
            let sdk = work.join(format!("dist/ts-mobile/{feature}/{feature}.gen.ts"));
            let zod = work.join(format!("dist/ts-mobile/{feature}/{feature}.zod.ts"));
            assert!(sdk.exists(), "missing {}", sdk.display());
            assert!(zod.exists(), "missing {}", zod.display());
        }

        // Frontend scaffold — pre-committed user-owned wrappers.
        for path in [
            "frontends/mobile/app/_layout.tsx",
            "frontends/mobile/app/index.tsx",
            "frontends/mobile/shell/client.ts",
            "frontends/mobile/app.json",
            "frontends/mobile/babel.config.js",
            "frontends/mobile/metro.config.js",
            "frontends/mobile/tsconfig.json",
            "frontends/mobile/package.json",
        ] {
            assert!(
                work.join(path).exists(),
                "scaffold {} missing — Wave 4 fixture expects pre-committed mobile shell",
                path
            );
        }

        // The user-owned `_layout.tsx` is the one-line re-export of
        // the regen body. Asserts the canonical wrapper form is what
        // landed.
        let app_layout = fs::read_to_string(work.join("frontends/mobile/app/_layout.tsx"))
            .expect("frontends/mobile/app/_layout.tsx must exist");
        assert!(
            app_layout.contains(r#"export { default } from "@/dist/ts-mobile/runtime/layout";"#),
            "_layout.tsx must re-export the regen body"
        );
    }
}
