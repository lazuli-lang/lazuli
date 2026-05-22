//! Doctor rule `type-duplicate`.
//!
//! Flags user-authored `.ts(x)` files that declare an `interface X {}`
//! or `type X = ...` whose name `X` is already exported by a generated
//! `dist/ts-web/<feature>/<feature>.gen.ts` or
//! `dist/ts-mobile/<feature>/<feature>.gen.ts` module. Duplicating the
//! generated type is drift-prone: the user copy does not update when
//! `lazuli generate ts` regenerates.
//!
//! Detection is intentionally regex-free: generated and user-authored files
//! are scanned line-by-line for leading `interface` / `type` declarations.
//!
//! The line-walker is **import-block aware**: lines that appear inside a
//! multi-line `import { ... } from "..."` block are skipped so that named
//! type imports written as `type Foo` (TS 4.5+ syntax) are not mistaken
//! for local re-declarations. Wave F01 surfaced this false positive on
//! the hostpoint canonical pilot: 20 of 31 reported diagnostics were
//! framework noise from multi-line SDK imports of generated types.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

pub struct Finding {
    pub user_file: PathBuf,
    pub type_name: String,
    pub generated_origin: PathBuf,
    pub message: String,
}

impl Finding {
    pub const CODE: &'static str = "type-duplicate";

    pub fn message(type_name: &str, user_file: &Path, generated_origin: &Path) -> String {
        format!(
            "`{}` redeclares type `{}` which is already exported from `{}`. \
             Import the generated type instead of duplicating; the user copy \
             will drift on next `lazuli generate ts`.",
            user_file.display(),
            type_name,
            generated_origin.display(),
        )
    }
}

/// Walk `root`, collect generated type names from `dist/ts-web/` and
/// `dist/ts-mobile/`, scan user code for redeclarations, emit Findings.
pub fn check(root: &Path) -> Vec<Finding> {
    let generated = collect_generated_types(root);
    if generated.is_empty() {
        return Vec::new();
    }

    let mut findings = Vec::new();
    for user_file in collect_user_ts_files(root) {
        let Ok(source) = fs::read_to_string(&user_file) else {
            continue;
        };

        for type_name in extract_declared_type_names(&source) {
            if type_name.len() < 2 {
                continue;
            }
            let Some(generated_origin) = generated.get(&type_name) else {
                continue;
            };

            findings.push(Finding {
                message: Finding::message(&type_name, &user_file, generated_origin),
                user_file: user_file.clone(),
                type_name,
                generated_origin: generated_origin.clone(),
            });
        }
    }

    findings.sort_by(|a, b| {
        a.user_file
            .cmp(&b.user_file)
            .then_with(|| a.type_name.cmp(&b.type_name))
    });
    findings
}

// --- private helpers ---

fn collect_generated_types(root: &Path) -> HashMap<String, PathBuf> {
    let mut out = HashMap::new();
    for target in ["ts-mobile", "ts-web"] {
        let base = root.join("dist").join(target);
        let mut files = Vec::new();
        collect_generated_files(&base, &mut files);
        files.sort();

        for path in files {
            let Ok(source) = fs::read_to_string(&path) else {
                continue;
            };

            for name in extract_declared_type_names(&source) {
                if name.len() >= 2 {
                    out.entry(name).or_insert_with(|| path.clone());
                }
            }
        }
    }
    out
}

fn collect_generated_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = sorted_entries(dir) else {
        return;
    };

    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            collect_generated_files(&path, out);
        } else if is_feature_gen_file(&path) {
            out.push(path);
        }
    }
}

fn is_feature_gen_file(path: &Path) -> bool {
    if matches!(
        path.file_name().and_then(|n| n.to_str()),
        Some("routes.gen.ts" | "routes.gen.tsx")
    ) {
        return false;
    }

    let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    let Some(feature_dir) = path
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
    else {
        return false;
    };

    file_name == format!("{feature_dir}.gen.ts")
}

fn collect_user_ts_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    collect_user_ts_files_under(&root.join("features"), true, &mut out);
    collect_user_ts_files_under(&root.join("app"), false, &mut out);
    out.sort();
    out
}

fn collect_user_ts_files_under(dir: &Path, require_platform_dir: bool, out: &mut Vec<PathBuf>) {
    let Ok(entries) = sorted_entries(dir) else {
        return;
    };

    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            if should_skip_dir(entry.file_name().to_str().unwrap_or("")) {
                continue;
            }
            collect_user_ts_files_under(&path, require_platform_dir, out);
        } else if is_ts_source(&path)
            && (!require_platform_dir || has_web_or_mobile_component(&path))
        {
            out.push(path);
        }
    }
}

fn sorted_entries(dir: &Path) -> std::io::Result<Vec<fs::DirEntry>> {
    let mut entries = fs::read_dir(dir)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|e| e.file_name());
    Ok(entries)
}

fn is_ts_source(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("ts") | Some("tsx")
    )
}

fn has_web_or_mobile_component(path: &Path) -> bool {
    path.components().any(|component| {
        let value = component.as_os_str().to_str().unwrap_or("");
        value == "web" || value == "mobile"
    })
}

fn should_skip_dir(name: &str) -> bool {
    matches!(
        name,
        "dist" | "node_modules" | "target" | ".git" | ".lazuli" | ".next" | ".expo"
    )
}

/// Extract exported type/interface names from a TS source string.
/// Handles `export interface X`, `export type X =`, `interface X`,
/// `type X =`. Stops at the identifier; does not parse the body.
///
/// Import-block aware: lines inside `import { ... } from "..."` blocks
/// (whether single-line or multi-line) are skipped so that the TS 4.5+
/// inline-`type` named-import syntax (`import { type Foo, ... }`) is not
/// misread as a local re-declaration. See Wave F01 on the hostpoint pilot.
fn extract_declared_type_names(source: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut in_import_block = false;

    for line in source.lines() {
        let trimmed = line.trim_start();

        // Single-line imports start and end on the same line.
        // Multi-line imports span lines from `import {` through `} from "..."`.
        if !in_import_block {
            if is_import_block_open(trimmed) {
                // Skip this line entirely; if the block doesn't close on
                // this line, enter multi-line mode until the close.
                if !is_import_block_close(trimmed) {
                    in_import_block = true;
                }
                continue;
            }
        } else {
            if is_import_block_close(trimmed) {
                in_import_block = false;
            }
            continue;
        }

        let after_export = trimmed.strip_prefix("export ").unwrap_or(trimmed);
        for prefix in &["interface ", "type "] {
            if let Some(rest) = after_export.strip_prefix(prefix) {
                if let Some(name) = first_identifier(rest) {
                    if !name.is_empty() {
                        out.push(name.to_owned());
                    }
                }
            }
        }
    }
    out
}

/// True if `trimmed` (already left-trimmed) begins a `import { ... }` block.
/// Matches `import {`, `import type {`, with optional whitespace between
/// the keyword and the brace. Bare `import "..."` (side-effect imports)
/// and `import Default from "..."` (default-only, no brace) do not need
/// type-line filtering and return false.
fn is_import_block_open(trimmed: &str) -> bool {
    let Some(rest) = trimmed.strip_prefix("import") else {
        return false;
    };
    // Require word boundary after `import` to avoid matching e.g. `importance`.
    let Some(first) = rest.chars().next() else {
        return false;
    };
    if !first.is_whitespace() {
        return false;
    }
    // Drop optional `type ` modifier: `import type { ... }`.
    let rest = rest.trim_start();
    let after_optional_type = match rest.strip_prefix("type") {
        Some(after) if after.starts_with(char::is_whitespace) => after.trim_start(),
        _ => rest,
    };
    after_optional_type.starts_with('{')
}

/// True if `trimmed` contains the closing `} from "..."` (or single-quoted)
/// of an `import { ... } from "..."` statement. Tolerates trailing
/// whitespace/semicolons; only checks that the close-brace and `from`
/// clause coexist on the line.
fn is_import_block_close(trimmed: &str) -> bool {
    let Some(brace_idx) = trimmed.find('}') else {
        return false;
    };
    let after = &trimmed[brace_idx + 1..];
    let after = after.trim_start();
    // Allow `} from "X"` or just `}` followed eventually by `from "X"`.
    // Also accept lone `}` on its own line where `from` is on the next
    // line — but in real TS that is rare; we conservatively also treat
    // a lone trailing `}` as closing the block to avoid getting stuck.
    if after.is_empty() || after.starts_with(';') || after.starts_with(',') {
        return true;
    }
    let after = after.strip_prefix("from").unwrap_or(after);
    let after = after.trim_start();
    after.starts_with('"') || after.starts_with('\'')
}

/// Walk leading characters of `s` while they match `[A-Za-z_][A-Za-z0-9_]*`.
fn first_identifier(s: &str) -> Option<&str> {
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return None;
    }
    let first = bytes[0];
    if !(first.is_ascii_alphabetic() || first == b'_') {
        return None;
    }
    let mut end = 1;
    while end < bytes.len() {
        let b = bytes[end];
        if b.is_ascii_alphanumeric() || b == b'_' {
            end += 1;
        } else {
            break;
        }
    }
    Some(&s[..end])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;
    use std::time::{SystemTime, UNIX_EPOCH};

    mod tempfile {
        use super::*;

        pub struct TempDir {
            path: PathBuf,
        }

        impl TempDir {
            pub fn new() -> io::Result<Self> {
                let mut path = std::env::temp_dir();
                let unique = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos();
                path.push(format!(
                    "lazuli-type-duplicate-{}-{unique}",
                    std::process::id()
                ));
                fs::create_dir_all(&path)?;
                Ok(Self { path })
            }

            pub fn path(&self) -> &Path {
                &self.path
            }
        }

        impl Drop for TempDir {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.path);
            }
        }
    }

    fn write(root: &Path, rel: &str, contents: &str) {
        let path = root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    #[test]
    fn redeclared_interface_fires() {
        let dir = tempfile::TempDir::new().unwrap();
        write(
            dir.path(),
            "dist/ts-web/slug/slug.gen.ts",
            "export interface Slug { id: string }\n",
        );
        write(
            dir.path(),
            "features/foo/web/views/admin/list.tsx",
            "interface Slug { name: string }\n",
        );

        let findings = check(dir.path());
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].type_name, "Slug");
        assert!(findings[0].message.contains("redeclares type `Slug`"));
    }

    #[test]
    fn redeclared_type_alias_fires() {
        let dir = tempfile::TempDir::new().unwrap();
        write(
            dir.path(),
            "dist/ts-web/slug/slug.gen.ts",
            "export type Slug = { id: string }\n",
        );
        write(
            dir.path(),
            "app/lib/types.ts",
            "export type Slug = { name: string }\n",
        );

        let findings = check(dir.path());
        assert_eq!(findings.len(), 1);
        assert_eq!(
            findings[0].generated_origin,
            dir.path().join("dist/ts-web/slug/slug.gen.ts")
        );
    }

    #[test]
    fn unique_user_type_does_not_fire() {
        let dir = tempfile::TempDir::new().unwrap();
        write(
            dir.path(),
            "dist/ts-web/slug/slug.gen.ts",
            "export interface Slug { id: string }\n",
        );
        write(
            dir.path(),
            "app/lib/types.ts",
            "type MyLocal = { id: string }\n",
        );

        assert!(check(dir.path()).is_empty());
    }

    #[test]
    fn import_of_generated_does_not_fire() {
        let dir = tempfile::TempDir::new().unwrap();
        write(
            dir.path(),
            "dist/ts-web/slug/slug.gen.ts",
            "export interface Slug { id: string }\n",
        );
        write(
            dir.path(),
            "features/foo/web/views/admin/list.tsx",
            "import type { Slug } from \"@/dist/ts-web/slug/slug.gen\";\n",
        );

        assert!(check(dir.path()).is_empty());
    }

    #[test]
    fn dist_is_not_scanned_as_user_source() {
        let dir = tempfile::TempDir::new().unwrap();
        write(
            dir.path(),
            "dist/ts-web/slug/slug.gen.ts",
            "export interface Slug { id: string }\n",
        );

        assert!(check(dir.path()).is_empty());
    }

    #[test]
    fn nested_features_scanned() {
        let dir = tempfile::TempDir::new().unwrap();
        write(
            dir.path(),
            "dist/ts-mobile/slug/slug.gen.ts",
            "export interface Slug { id: string }\n",
        );
        write(
            dir.path(),
            "features/foo/web/cells/bar.tsx",
            "export interface Slug { name: string }\n",
        );

        let findings = check(dir.path());
        assert_eq!(findings.len(), 1);
        assert_eq!(
            findings[0].user_file,
            dir.path().join("features/foo/web/cells/bar.tsx")
        );
    }

    #[test]
    fn node_modules_not_scanned() {
        let dir = tempfile::TempDir::new().unwrap();
        write(
            dir.path(),
            "dist/ts-web/slug/slug.gen.ts",
            "export interface Slug { id: string }\n",
        );
        write(
            dir.path(),
            "node_modules/x/y.d.ts",
            "export interface Slug { name: string }\n",
        );

        assert!(check(dir.path()).is_empty());
    }

    #[test]
    fn deterministic_order() {
        let dir = tempfile::TempDir::new().unwrap();
        write(
            dir.path(),
            "dist/ts-web/slug/slug.gen.ts",
            "export interface Slug { id: string }\nexport type Widget = { id: string }\n",
        );
        write(
            dir.path(),
            "app/lib/z.ts",
            "type Widget = {}\ninterface Slug {}\n",
        );
        write(
            dir.path(),
            "features/foo/web/cells/a.tsx",
            "interface Slug {}\n",
        );

        let first = check(dir.path())
            .into_iter()
            .map(|f| (f.user_file, f.type_name))
            .collect::<Vec<_>>();
        let second = check(dir.path())
            .into_iter()
            .map(|f| (f.user_file, f.type_name))
            .collect::<Vec<_>>();
        assert_eq!(first, second);
    }

    #[test]
    fn extract_export_interface() {
        assert_eq!(
            extract_declared_type_names("export interface Slug { id: string }"),
            vec!["Slug"]
        );
    }

    #[test]
    fn extract_type_alias() {
        assert_eq!(
            extract_declared_type_names("type Slug = { id: string }"),
            vec!["Slug"]
        );
    }

    #[test]
    fn extract_ignores_mixed_line_not_matching() {
        assert!(extract_declared_type_names("const x: Slug = value").is_empty());
    }

    // ---------------------------------------------------------------
    // Post-Wave-H 7+6 closed catalog — plural topology coverage
    // ---------------------------------------------------------------

    /// Wave H canon shape: `app/clients/<name>/src/cells/<feature>/...`
    /// redeclaring a generated type fires. Mirrors the hostpoint pilot
    /// `cells/messaging/ChatExperience.tsx` redeclaring `Chat`.
    #[test]
    fn post_wave_h_plural_cell_redeclaration_fires() {
        let dir = tempfile::TempDir::new().unwrap();
        write(
            dir.path(),
            "dist/ts-web/messaging/messaging.gen.ts",
            "export interface Chat { id: string }\n\
             export interface ChatMessage { id: string }\n",
        );
        write(
            dir.path(),
            "app/clients/web-app/src/cells/messaging/ChatExperience.tsx",
            "interface Chat { name: string }\n\
             interface ChatMessage { body: string }\n",
        );

        let findings = check(dir.path());

        assert_eq!(findings.len(), 2, "found: {:?}", findings.len());
        let names: Vec<String> = findings.iter().map(|f| f.type_name.clone()).collect();
        assert!(names.contains(&"Chat".to_string()));
        assert!(names.contains(&"ChatMessage".to_string()));
    }

    /// Wave H canon shape: `app/clients/<name>/src/routes/...`
    /// redeclaring a generated type fires.
    #[test]
    fn post_wave_h_plural_route_redeclaration_fires() {
        let dir = tempfile::TempDir::new().unwrap();
        write(
            dir.path(),
            "dist/ts-web/host/host.gen.ts",
            "export type HostHomePending = { id: string }\n",
        );
        write(
            dir.path(),
            "app/clients/web-app/src/routes/HostHome.tsx",
            "type HostHomePending = { local: true }\n",
        );

        let findings = check(dir.path());

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].type_name, "HostHomePending");
        assert_eq!(
            findings[0].user_file,
            dir.path()
                .join("app/clients/web-app/src/routes/HostHome.tsx")
        );
    }

    /// A clean Wave H plural client tree (no type collisions) emits
    /// zero findings even with generated types present.
    #[test]
    fn post_wave_h_plural_clean_tree_is_silent() {
        let dir = tempfile::TempDir::new().unwrap();
        write(
            dir.path(),
            "dist/ts-web/messaging/messaging.gen.ts",
            "export interface Chat { id: string }\n",
        );
        // Canon-shape files that DON'T redeclare any generated type.
        write(
            dir.path(),
            "app/clients/web-app/src/shell/App.tsx",
            "import type { Chat } from \"@/sdk/messaging/messaging.gen\";\n\
             export function App() { return null }\n",
        );
        write(
            dir.path(),
            "app/clients/web-app/src/cells/messaging/ChatExperience.tsx",
            "import type { Chat } from \"@/sdk/messaging/messaging.gen\";\n\
             export function ChatExperience() { return null }\n",
        );
        write(
            dir.path(),
            "app/clients/web-app/src/ui/forms/Button.tsx",
            "type LocalButtonProps = { label: string }\n",
        );
        write(
            dir.path(),
            "app/clients/web-app/src/state/toast.store.ts",
            "export type ToastMessage = { id: string; body: string }\n",
        );

        assert!(check(dir.path()).is_empty());
    }

    /// Wave H singular topology (`app/web/...`) — same redeclaration
    /// detection.
    #[test]
    fn post_wave_h_singular_cell_redeclaration_fires() {
        let dir = tempfile::TempDir::new().unwrap();
        write(
            dir.path(),
            "dist/ts-web/messaging/messaging.gen.ts",
            "export interface Chat { id: string }\n",
        );
        write(
            dir.path(),
            "app/web/cells/messaging/ChatExperience.tsx",
            "interface Chat { name: string }\n",
        );

        let findings = check(dir.path());

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].type_name, "Chat");
    }

    // ---------------------------------------------------------------
    // Wave S2 — import-block awareness (false-positive fix)
    //
    // Wave F01 surfaced that the canonical hostpoint pilot raised 31
    // type-duplicate diagnostics, but 20 were framework noise from
    // multi-line `import { type X, ... } from "..."` blocks being
    // misread as local re-declarations. These tests lock the fix.
    // ---------------------------------------------------------------

    /// A multi-line `import { type Foo, type Bar } from "..."` block
    /// must not produce type-duplicate diagnostics for `Foo`/`Bar`.
    /// Mirrors the hostpoint `HostHome.tsx` real-world false positive.
    #[test]
    fn import_block_type_keyword_not_flagged() {
        let dir = tempfile::TempDir::new().unwrap();
        write(
            dir.path(),
            "dist/ts-web/messaging/messaging.gen.ts",
            "export interface Chat { id: string }\n\
             export interface ChatMessage { id: string }\n",
        );
        write(
            dir.path(),
            "app/clients/web-app/src/cells/messaging/ChatExperience.tsx",
            "import {\n  \
               type Chat,\n  \
               type ChatMessage,\n\
             } from \"@hostpoint/sdk/messaging/messaging.gen\";\n\
             \n\
             export function ChatExperience() { return null }\n",
        );

        assert!(
            check(dir.path()).is_empty(),
            "import-block type-keywords must not be flagged"
        );
    }

    /// `import { type Foo as Bar } from "..."` — the `as` alias form
    /// inside a multi-line import block must not flag the underlying
    /// `Foo` identifier as a re-declaration.
    #[test]
    fn import_block_with_aliases_not_flagged() {
        let dir = tempfile::TempDir::new().unwrap();
        write(
            dir.path(),
            "dist/ts-web/host/host.gen.ts",
            "export type HostHomePending = { id: string }\n",
        );
        write(
            dir.path(),
            "app/clients/web-app/src/routes/HostHome.tsx",
            "import {\n  \
               type HostHomePending as SdkHostHomePending,\n\
             } from \"@hostpoint/sdk/host/host.gen\";\n\
             \n\
             export function HostHome() { return null }\n",
        );

        assert!(
            check(dir.path()).is_empty(),
            "aliased type imports must not be flagged"
        );
    }

    /// Single-line `import { type Foo, type Bar } from "..."` — same
    /// rule applies on one line.
    #[test]
    fn single_line_import_type_not_flagged() {
        let dir = tempfile::TempDir::new().unwrap();
        write(
            dir.path(),
            "dist/ts-web/messaging/messaging.gen.ts",
            "export interface Chat { id: string }\n\
             export interface ChatMessage { id: string }\n",
        );
        write(
            dir.path(),
            "app/clients/web-app/src/cells/messaging/ChatExperience.tsx",
            "import { type Chat, type ChatMessage } from \"@hostpoint/sdk/messaging/messaging.gen\";\n\
             export function ChatExperience() { return null }\n",
        );

        assert!(
            check(dir.path()).is_empty(),
            "single-line inline-type imports must not be flagged"
        );
    }

    /// After a legitimate import block closes, a real local
    /// `type Foo = { ... }` redeclaration on a subsequent line MUST
    /// still be flagged. Guards against the fix going too far and
    /// permanently disabling the rule.
    #[test]
    fn local_type_after_import_still_flagged() {
        let dir = tempfile::TempDir::new().unwrap();
        write(
            dir.path(),
            "dist/ts-web/host/host.gen.ts",
            "export type HostHomePending = { id: string }\n",
        );
        write(
            dir.path(),
            "app/clients/web-app/src/routes/HostHome.tsx",
            "import {\n  \
               getHostHome,\n  \
               type HostHomeSnapshot,\n\
             } from \"@hostpoint/sdk/host/host.gen\";\n\
             \n\
             type HostHomePending = { local: true };\n\
             \n\
             export function HostHome() { return null }\n",
        );

        let findings = check(dir.path());
        assert_eq!(
            findings.len(),
            1,
            "found: {findings:?}",
            findings = findings.iter().map(|f| &f.type_name).collect::<Vec<_>>()
        );
        assert_eq!(findings[0].type_name, "HostHomePending");
    }

    /// Regression: hostpoint pilot pattern — a mix of legitimate
    /// SDK type imports (must be silent) and legitimate local
    /// re-declarations (must fire) in the same file. The 7 + 4
    /// remaining pilot bugs after the false-positive fix should
    /// still surface. This synthetic fixture mirrors the shape:
    /// imports first, redeclarations after.
    #[test]
    fn regression_existing_legitimate_redeclaration() {
        let dir = tempfile::TempDir::new().unwrap();
        write(
            dir.path(),
            "dist/ts-web/messaging/messaging.gen.ts",
            "export interface Chat { id: string }\n\
             export interface ChatMessage { id: string }\n\
             export type ChatSummary = { id: string }\n",
        );
        write(
            dir.path(),
            "app/clients/web-app/src/cells/messaging/ChatExperience.tsx",
            "import {\n  \
               type Chat,\n  \
               type ChatMessage as SdkChatMessage,\n\
             } from \"@hostpoint/sdk/messaging/messaging.gen\";\n\
             \n\
             // Legitimate redeclaration — drift-prone, should fire.\n\
             interface ChatMessage { body: string }\n\
             type ChatSummary = { snippet: string };\n\
             \n\
             export function ChatExperience() { return null }\n",
        );

        let findings = check(dir.path());
        let names: Vec<&str> = findings.iter().map(|f| f.type_name.as_str()).collect();
        assert_eq!(findings.len(), 2, "names: {names:?}");
        assert!(names.contains(&"ChatMessage"), "names: {names:?}");
        assert!(names.contains(&"ChatSummary"), "names: {names:?}");
        // The aliased import of `Chat` must NOT fire.
        assert!(!names.contains(&"Chat"), "names: {names:?}");
    }
}
