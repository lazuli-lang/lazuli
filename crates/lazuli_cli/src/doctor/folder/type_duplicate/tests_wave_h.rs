//! Post-Wave-H 7+6 closed-catalog coverage for `type_duplicate`.
//!
//! Covers both the plural topology (`app/clients/<name>/src/...`) and
//! the singular topology (`app/web/...`) defined by
//! `[[client_src_canonical_architecture_2026-05-17]]` §3 — mirrors the
//! the canonical pilot redeclaration shapes.
//!
//! Pre-Wave-H legacy coverage lives in `tests_basic.rs`; Wave S2
//! import-block awareness in `tests_import_blocks.rs`.

#![cfg(test)]

use super::*;
use super::test_support::{write, TempDir};

/// Wave H canon shape: `app/clients/<name>/src/cells/<feature>/...`
/// redeclaring a generated type fires. Mirrors the the canonical pilot
/// `cells/messaging/ChatExperience.tsx` redeclaring `Chat`.
#[test]
fn post_wave_h_plural_cell_redeclaration_fires() {
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
    let dir = TempDir::new().unwrap();
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
    let dir = TempDir::new().unwrap();
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
    let dir = TempDir::new().unwrap();
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
