#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SMOKE_DIR="${SMOKE_DIR:-/tmp/lazuli-smoke}"
FIXTURE="${FIXTURE:-examples/hostpoint-mini}"
SMOKE_VET="${SMOKE_VET:-0}"

cd "$REPO_ROOT"

PASSED_STEPS=0
TOTAL_STEPS=5
if [[ "$SMOKE_VET" == "1" ]]; then
  TOTAL_STEPS=6
fi
CURRENT_STEP="initialization"

fail() {
  local exit_code=$?
  echo "✗ smoke failed"
  echo "  step: $CURRENT_STEP"
  echo "  summary: $PASSED_STEPS/$TOTAL_STEPS steps passed"
  exit "$exit_code"
}
trap fail ERR

run_step() {
  local label="$1"
  shift

  CURRENT_STEP="$label"
  echo "▶ $label"
  "$@"
  PASSED_STEPS=$((PASSED_STEPS + 1))
}

generate_smoke() {
  rm -rf "$SMOKE_DIR"
  cargo run -q -p lazuli_cli -- generate go "$FIXTURE" --out "$SMOKE_DIR"
}

append_runtime_replace() {
  printf '\nreplace lazuli.dev/runtime => %s/runtime/go\n' "$REPO_ROOT" >> "$SMOKE_DIR/go.mod"
}

go_tidy_build() {
  go -C "$SMOKE_DIR" mod tidy
  go -C "$SMOKE_DIR" build ./...
}

go_vet() {
  go -C "$SMOKE_DIR" vet ./...
}

run_step "cargo test -p lazuli_codegen_go" cargo test -p lazuli_codegen_go
run_step "cargo test -p lazuli_codegen_go --features smoke_e2e" cargo test -p lazuli_codegen_go --features smoke_e2e
run_step "generate $FIXTURE → $SMOKE_DIR" generate_smoke
run_step "append runtime replace directive" append_runtime_replace
run_step "go mod tidy + build" go_tidy_build

if [[ "$SMOKE_VET" == "1" ]]; then
  run_step "go vet ./..." go_vet
else
  echo "▶ go vet ./... (skipped; set SMOKE_VET=1 to enable)"
fi

trap - ERR
echo "✓ smoke passed"
echo "  summary: $PASSED_STEPS/$TOTAL_STEPS steps passed"
