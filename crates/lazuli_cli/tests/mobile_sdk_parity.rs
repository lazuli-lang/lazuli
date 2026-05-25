//! Integration test for the **MOBILE-SDK-PARITY** invariant.
//!
//! Per `docs/proposals/mobile-target.md` §7:
//!
//! > `dist/ts-mobile/<feat>/<feat>.gen.ts` must be byte-identical to
//! > `dist/ts-web/<feat>/<feat>.gen.ts` for any (audience, feature)
//! > tuple where both targets carry the same audience set.
//!
//! The walker calls a single audience-agnostic emitter
//! (`emit_feature_sdk_ts`) for every target prefix; this test guards
//! against future drift that would silently break the invariant — e.g.,
//! a per-target branch that emits subtly different bodies for the
//! same feature when audience scoping coincides.
//!
//! The fixture `examples/lazurite-multifrontend/` declares both an
//! Expo frontend (`audiences = ["traveler", "host"]`) and a TanStack
//! Vite frontend (`audiences = ["host"]`). For shared audiences
//! ("host"), the SDK + zod outputs must match byte-for-byte across
//! the two target directories.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;

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

/// Copy a fixture into a temp dir so the test can run `generate ts`
/// without polluting the repo's working tree.
fn copy_dir(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).unwrap();
    for entry in std::fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        let name = entry.file_name();
        let target = dst.join(&name);
        // Skip generated dirs so we measure fresh emission.
        if name == "dist" || name == ".lazuli" || name == "node_modules" {
            continue;
        }
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&path, &target);
        } else {
            std::fs::copy(&path, &target).unwrap();
        }
    }
}

/// Walk every `dist/<target_prefix>/<feature>/<file>` under `dist_root`
/// and bucket the bytes by `(feature, file)` so siblings across targets
/// can be compared.
fn collect_sdk_files(dist_root: &Path) -> BTreeMap<(String, String), Vec<(String, Vec<u8>)>> {
    let mut buckets: BTreeMap<(String, String), Vec<(String, Vec<u8>)>> = BTreeMap::new();
    if !dist_root.exists() {
        return buckets;
    }
    for target_dir in std::fs::read_dir(dist_root).unwrap() {
        let target_dir = target_dir.unwrap();
        if !target_dir.file_type().unwrap().is_dir() {
            continue;
        }
        let target_name = target_dir.file_name().to_string_lossy().to_string();
        if !target_name.starts_with("ts-") {
            continue;
        }
        for feature_dir in std::fs::read_dir(target_dir.path()).unwrap() {
            let feature_dir = feature_dir.unwrap();
            if !feature_dir.file_type().unwrap().is_dir() {
                continue;
            }
            let feature_name = feature_dir.file_name().to_string_lossy().to_string();
            for file in std::fs::read_dir(feature_dir.path()).unwrap() {
                let file = file.unwrap();
                if !file.file_type().unwrap().is_file() {
                    continue;
                }
                let file_name = file.file_name().to_string_lossy().to_string();
                // Only `.gen.ts` and `.zod.ts` are governed by the
                // parity invariant; per-view files live under `views/`
                // and legitimately diverge (router-adapter imports).
                if !file_name.ends_with(".gen.ts") && !file_name.ends_with(".zod.ts") {
                    continue;
                }
                let bytes = std::fs::read(file.path()).unwrap();
                buckets
                    .entry((feature_name.clone(), file_name))
                    .or_default()
                    .push((target_name.clone(), bytes));
            }
        }
    }
    buckets
}

#[test]
fn multifrontend_sdk_files_are_byte_identical_across_targets() {
    let tmp = TempDir::new().expect("create temp dir");
    let fixture_dst = tmp.path().join("fixture");
    copy_dir(
        &repo_root().join("examples").join("lazurite-multifrontend"),
        &fixture_dst,
    );

    let status = Command::new(cli_bin())
        .current_dir(&fixture_dst)
        .args(["generate", "ts", "."])
        .status()
        .expect("run lazuli generate ts");
    assert!(
        status.success(),
        "lazuli generate ts must exit 0 on multifrontend fixture; got {status:?}"
    );

    let buckets = collect_sdk_files(&fixture_dst.join("dist"));
    assert!(
        !buckets.is_empty(),
        "expected at least one SDK file under dist/ts-*; none found"
    );

    let mut compared_pairs = 0usize;
    for ((feature, file), entries) in &buckets {
        if entries.len() < 2 {
            continue;
        }
        let (first_target, first_bytes) = &entries[0];
        for (other_target, other_bytes) in &entries[1..] {
            assert_eq!(
                first_bytes, other_bytes,
                "MOBILE-SDK-PARITY violation for `{feature}/{file}`: \
                 `dist/{first_target}/{feature}/{file}` differs from \
                 `dist/{other_target}/{feature}/{file}`",
            );
            compared_pairs += 1;
        }
    }

    // Sanity: the fixture should actually exercise the invariant. If
    // no pairs were compared, the test is silently passing and a future
    // refactor could break parity unnoticed.
    assert!(
        compared_pairs > 0,
        "no cross-target SDK pairs were available to compare; \
         the multifrontend fixture should declare overlapping audiences \
         across an Expo and a TanStack frontend"
    );
}
