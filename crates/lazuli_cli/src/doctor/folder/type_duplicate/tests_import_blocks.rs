//! Wave S2 — import-block awareness (false-positive fix) for
//! `type_duplicate`.
//!
//! Wave F01 surfaced that the canonical hostpoint pilot raised 31
//! type-duplicate diagnostics, but 20 were framework noise from
//! multi-line `import { type X, ... } from "..."` blocks being misread
//! as local re-declarations. These tests lock the fix in.
//!
//! Pre-Wave-H legacy coverage lives in `tests_basic.rs`; Wave H
//! plural+singular coverage in `tests_wave_h.rs`.

#![cfg(test)]

use super::*;
use super::test_support::{write, TempDir};

/// A multi-line `import { type Foo, type Bar } from "..."` block
/// must not produce type-duplicate diagnostics for `Foo`/`Bar`.
/// Mirrors the hostpoint `HostHome.tsx` real-world false positive.
#[test]
fn import_block_type_keyword_not_flagged() {
    let dir = TempDir::new().unwrap();
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
    let dir = TempDir::new().unwrap();
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
    let dir = TempDir::new().unwrap();
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
    let dir = TempDir::new().unwrap();
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
    let dir = TempDir::new().unwrap();
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
