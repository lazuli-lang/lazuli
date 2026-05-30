#!/usr/bin/env bash
# dev-check.sh — contributor pre-push orchestrator (SPEC-20 2/n).
#
# Pure ORCHESTRATOR, not logic: it sequences the framework's own quality gates.
# The self-investigation LOGIC is Rust (the doctor engine + the xtask
# projectors); this script only invokes them, in the same order CI does, so a
# contributor can reproduce the CI verdict locally before pushing.
#
#   1. fmt        — `cargo fmt --check`
#   2. clippy     — `cargo clippy --workspace --all-targets -- -D warnings`
#   3. freshness  — `xtask <gen> --check` (tmLanguage / keyword-ref / catalog
#                   are byte-fresh vs the `lazuli_keywords` registry)
#   4. docs       — `cargo test -p lazuli_cli --test docs_hygiene` (every doc
#                   citation + link resolves)
#   5. self-doctor — `lazuli-dev self-doctor` (the framework lints its own Rust
#                   under INTERNAL-* — formerly `lazuli doctor --self`)
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

step() { printf '\n\033[1;34m==>\033[0m %s\n' "$1"; }

step "1/5 cargo fmt --check"
cargo fmt --all --check

step "2/5 cargo clippy --workspace --all-targets -- -D warnings"
cargo clippy --workspace --all-targets -- -D warnings

step "3/5 generated-artifact freshness (xtask --check)"
cargo run -q -p xtask -- gen-tmlanguage --check
cargo run -q -p xtask -- gen-keyword-reference --check
cargo run -q -p xtask -- gen-catalog-reference --check

step "4/5 docs hygiene (citations + links)"
cargo test -q -p lazuli_cli --test docs_hygiene

step "5/5 self-doctor (framework dogfood)"
cargo run -q -p lazuli_cli --bin lazuli-dev -- self-doctor \
  --security-profile strict \
  --fail-on category:internal_hygiene \
  --fail-on category:test_discipline

printf '\n\033[1;32mdev-check: all gates green.\033[0m\n'
