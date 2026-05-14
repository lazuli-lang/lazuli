//! Design-token lint rules (`design-token-*`).
//!
//! Each rule is a sub-module exposing a `check` function. The orchestrator
//! wires each `check` into `DoctorPackage::diagnostics()` post-merge.
//!
//! All rules are line-scan based and operate over `.tsx` files under the
//! project root. They consult the allowlist emitted by the design-tokens
//! codegen at `dist/ts-web/design/allowlist.json`. When the allowlist is
//! missing (no `design.lzi` authored yet), every rule in this sub-namespace
//! suppresses — design-token enforcement only fires once the catalog exists.
//!
//! Reference: docs/proposals/design-tokens.md §6 (rules + escape hatch).

pub mod fontfamily_leak;
pub mod helpers;
pub mod hex_leak;
pub mod hygiene;
pub mod px_leak;
pub mod shadow_leak;
pub mod token_undefined;

pub use helpers::{Allowlist, read_allowlist, scan_lines, walk_tsx_files};

#[cfg(test)]
mod integration_tests {
    use super::*;
    use std::collections::HashMap;
    use std::path::Path;

    /// A full `.tsx` file with one trigger for each of the 5 rules.
    /// Each rule must fire only its own concern.
    const MULTI_VIOLATION_TSX: &str = r##"import { useState } from "react";

export function MultiViolation() {
  return (
    <button
      className="bg-purple-500 text-foreground rounded-md p-3"
      style={{
        color: "#7c3aed",
        padding: "12px",
        fontFamily: "Comic Sans MS",
        boxShadow: "0 1px 3px rgba(0,0,0,0.1)",
      }}
    >
      click me
    </button>
  );
}
"##;

    fn allowlist() -> Allowlist {
        let mut buckets = HashMap::new();
        buckets.insert(
            "bg".to_string(),
            vec!["primary".to_string(), "success".to_string()],
        );
        buckets.insert(
            "text".to_string(),
            vec!["foreground".to_string(), "primary-foreground".to_string()],
        );
        buckets.insert(
            "rounded".to_string(),
            vec!["md".to_string(), "lg".to_string()],
        );
        buckets.insert(
            "p".to_string(),
            vec!["1".to_string(), "2".to_string(), "3".to_string(), "4".to_string()],
        );
        buckets.insert("font".to_string(), vec!["sans".to_string(), "mono".to_string()]);
        Allowlist { buckets }
    }

    fn lines() -> Vec<&'static str> {
        MULTI_VIOLATION_TSX.lines().collect()
    }

    #[test]
    fn token_undefined_fires_only_on_undeclared_classes() {
        let al = allowlist();
        let lines = lines();
        let f = token_undefined::check_file(Path::new("x.tsx"), &lines, &al);
        assert_eq!(f.len(), 1, "expected exactly bg-purple-500; got: {:?}", f);
        assert_eq!(f[0].class_token, "bg-purple-500");
    }

    #[test]
    fn hex_leak_fires_only_on_hex_literals() {
        let lines = lines();
        let f = hex_leak::check_file(Path::new("x.tsx"), MULTI_VIOLATION_TSX, &lines);
        assert_eq!(f.len(), 1, "expected exactly #7c3aed; got: {:?}", f);
        assert_eq!(f[0].hex, "7c3aed");
        assert_eq!(f[0].site, hex_leak::Site::InlineStyle);
    }

    #[test]
    fn px_leak_fires_only_on_px_literals() {
        let lines = lines();
        let f = px_leak::check_file(Path::new("x.tsx"), MULTI_VIOLATION_TSX, &lines);
        assert_eq!(f.len(), 1, "expected exactly 12px; got: {:?}", f);
        assert_eq!(f[0].literal, "12px");
    }

    #[test]
    fn fontfamily_leak_fires_only_on_raw_family_string() {
        let al = allowlist();
        let lines = lines();
        let f = fontfamily_leak::check_file(Path::new("x.tsx"), MULTI_VIOLATION_TSX, &lines, &al);
        assert_eq!(f.len(), 1, "expected exactly Comic Sans MS; got: {:?}", f);
        assert_eq!(f[0].value, "Comic Sans MS");
    }

    #[test]
    fn shadow_leak_fires_only_on_box_shadow_literal() {
        let lines = lines();
        let f = shadow_leak::check_file(Path::new("x.tsx"), MULTI_VIOLATION_TSX, &lines);
        assert_eq!(f.len(), 1, "expected exactly one boxShadow; got: {:?}", f);
        assert!(f[0].value.contains("rgba"));
    }

    #[test]
    fn all_rules_together_attribute_one_finding_each() {
        // Cross-check: the union of all rule findings is exactly 5 — no
        // rule is silently swallowing another's territory and no rule is
        // over-firing.
        let al = allowlist();
        let lines = lines();
        let total = token_undefined::check_file(Path::new("x.tsx"), &lines, &al).len()
            + hex_leak::check_file(Path::new("x.tsx"), MULTI_VIOLATION_TSX, &lines).len()
            + px_leak::check_file(Path::new("x.tsx"), MULTI_VIOLATION_TSX, &lines).len()
            + fontfamily_leak::check_file(Path::new("x.tsx"), MULTI_VIOLATION_TSX, &lines, &al).len()
            + shadow_leak::check_file(Path::new("x.tsx"), MULTI_VIOLATION_TSX, &lines).len();
        assert_eq!(total, 5, "expected exactly one finding per rule");
    }
}
