package lazuli

// FACE-PARITY harness — ctx-path contract (runtime side).
//
// The codegen lowers every `ctx.<head>.<rest>` binding expression to
// `lazuli.FromCtx("<head>.<rest>")` (crates/lazuli_codegen_go/src/emitter/
// command/effects_format.rs, the `"ctx" =>` arm). At execution time the
// runtime resolves that source via readCtx (handle.go). The CONTRACT:
// every canonical ctx path the language recognizes MUST be handled by
// readCtx — otherwise the request 500s with "unknown ctx path: <p>".
// That is exactly how bug #17 shipped: `ctx.actor.id` lowered to
// FromCtx("actor.id") but readCtx had no `actor.id` arm, so it 500'd.
//
// There was NO test asserting this; this is that test. The single source
// of truth is ctx_path_catalog.json (consumed by BOTH this test and the
// Rust ctx_path_parity test). This file enforces both directions so the
// catalog cannot drift from readCtx:
//
//   1. catalog ⊆ readCtx-handled  — TestReadCtxHandlesEveryCatalogPath:
//      for each path in the catalog, readCtx must NOT return the
//      "unknown ctx path" error. THE DANGEROUS DIRECTION (a codegen-
//      emittable path with no runtime arm = a 500). This is the #17 guard.
//   2. readCtx-handled ⊆ catalog  — TestReadCtxArmsAreAllInCatalog:
//      every `case "<lit>":` literal in readCtx's switch must be a catalog
//      path, so a new runtime arm forces a catalog entry (which in turn
//      the Rust side sees). Keeps the SoT honest — no orphan arms.

import (
	"encoding/json"
	"os"
	"regexp"
	"strings"
	"testing"
	"time"
)

// ctxPathCatalog is the parsed single-source-of-truth catalog.
type ctxPathCatalog struct {
	Paths []string `json:"paths"`
}

// loadCatalog parses the single-source-of-truth ctx-path catalog, which
// is checked in beside this test (and beside handle.go, whose readCtx it
// gates). The Rust face reads the same file via the workspace root.
func loadCatalog(t *testing.T) ctxPathCatalog {
	t.Helper()
	// The catalog is checked in beside this test file.
	raw, err := os.ReadFile("ctx_path_catalog.json")
	if err != nil {
		t.Fatalf("cannot read ctx_path_catalog.json (the ctx-path SoT): %v", err)
	}
	var cat ctxPathCatalog
	if err := json.Unmarshal(raw, &cat); err != nil {
		t.Fatalf("ctx_path_catalog.json is not valid JSON: %v", err)
	}
	if len(cat.Paths) == 0 {
		t.Fatal("ctx_path_catalog.json has no paths — the SoT is empty")
	}
	return cat
}

// fullyPopulatedCtx returns a *Ctx with User, Tenant and Now all set, so
// that handled ctx paths return a value (or a benign auth/tenant error)
// rather than a "no user/tenant in context" error. We only care that the
// path is RECOGNIZED, never about the concrete value, so populating every
// dependency isolates the "unknown ctx path" signal.
func fullyPopulatedCtx() *Ctx {
	return &Ctx{
		User:   &User{ID: 1, OrgID: 2},
		Tenant: &Tenant{OrgID: 2},
		Now:    time.Now(),
	}
}

// TestReadCtxHandlesEveryCatalogPath is the #17 regression guard. For
// every canonical ctx path the codegen can emit (the catalog), readCtx
// must recognize it — i.e. it must NOT return the "unknown ctx path"
// sentinel. If this fails, the codegen can emit a FromCtx(path) the
// runtime 500s on.
func TestReadCtxHandlesEveryCatalogPath(t *testing.T) {
	cat := loadCatalog(t)
	ctx := fullyPopulatedCtx()

	var unhandled []string
	for _, p := range cat.Paths {
		_, err := readCtx(ctx, p)
		if err == nil {
			continue // recognized, value returned
		}
		le, ok := err.(*Error)
		// The only failure mode that means "this path is NOT handled" is
		// the readCtx default arm. Any other error (e.g. a 401 "no
		// authenticated user") still means the path WAS recognized — it
		// just had unmet preconditions, which is correct handling.
		if ok && strings.HasPrefix(le.Message, "unknown ctx path:") {
			unhandled = append(unhandled, p)
		}
	}

	if len(unhandled) > 0 {
		t.Fatalf(
			"ctx-path FACE-PARITY violation (bug #17 class): the codegen can emit "+
				"lazuli.FromCtx(%q) for these catalog paths but readCtx (handle.go) "+
				"does NOT handle them — every such path 500s at runtime with "+
				"\"unknown ctx path\". Add a `case` arm to readCtx for each:\n  - %s",
			"<path>", strings.Join(unhandled, "\n  - "),
		)
	}
}

// readCtxCaseLiterals scrapes the string literals from the `case "...":`
// arms of readCtx in handle.go. This is the runtime face made enumerable
// for the reverse-direction check. We slice handle.go from `func readCtx`
// to the next top-level `func ` so we only read readCtx's own switch.
func readCtxCaseLiterals(t *testing.T) []string {
	t.Helper()
	src, err := os.ReadFile("handle.go")
	if err != nil {
		t.Fatalf("cannot read handle.go: %v", err)
	}
	text := string(src)
	start := strings.Index(text, "func readCtx(")
	if start < 0 {
		t.Fatal("could not locate `func readCtx(` in handle.go")
	}
	rest := text[start+len("func readCtx("):]
	end := strings.Index(rest, "\nfunc ")
	if end >= 0 {
		text = text[start : start+len("func readCtx(")+end]
	} else {
		text = text[start:]
	}

	// Match `case "a", "b":` lines and pull every quoted literal.
	caseLine := regexp.MustCompile(`(?m)^\s*case\s+(.+):`)
	lit := regexp.MustCompile(`"([^"]*)"`)
	var out []string
	for _, m := range caseLine.FindAllStringSubmatch(text, -1) {
		for _, l := range lit.FindAllStringSubmatch(m[1], -1) {
			out = append(out, l[1])
		}
	}
	if len(out) == 0 {
		t.Fatal("scraped zero `case` literals from readCtx — the scraper is broken")
	}
	return out
}

// TestReadCtxArmsAreAllInCatalog asserts the reverse direction: every
// path readCtx handles is registered in the catalog. This keeps the SoT
// honest — a new readCtx arm cannot be added without also registering it
// in ctx_path_catalog.json, which is what the Rust face reads. Prevents
// the catalog from silently lagging behind readCtx.
func TestReadCtxArmsAreAllInCatalog(t *testing.T) {
	cat := loadCatalog(t)
	inCatalog := make(map[string]bool, len(cat.Paths))
	for _, p := range cat.Paths {
		inCatalog[p] = true
	}

	var orphan []string
	for _, lit := range readCtxCaseLiterals(t) {
		if !inCatalog[lit] {
			orphan = append(orphan, lit)
		}
	}

	if len(orphan) > 0 {
		t.Fatalf(
			"readCtx handles ctx path(s) absent from the SoT catalog "+
				"(ctx_path_catalog.json). Add them so both faces agree (and the "+
				"Rust side sees them):\n  - %s",
			strings.Join(orphan, "\n  - "),
		)
	}
}

// --- Source-kind FACE-PARITY (bug #10 / #18-adjacent) -------------------
//
// The codegen emits a fixed set of Source constructors (From* in
// effect.go). Each lowers to a `sourceKind`. resolveSource (handle.go)
// must have a non-stub arm for every kind the codegen can emit, or an
// emittable binding 500s/501s at runtime. Today `sourceTarget` returns a
// 501 "not yet implemented" — and the codegen CAN emit FromTarget (the
// `"target" =>` arm in effects_format.rs). That is a KNOWN reachable
// stub (audit bug #10), so this test documents it as an expected gap
// rather than failing the build; flip allowStub to false once
// sourceTarget is implemented (or codegen stops emitting FromTarget).

// emittableSourceKinds are the sourceKinds the codegen `From*`
// constructors produce. Kept beside the constructors they mirror; the
// companion Rust test (source_kind_parity) asserts the codegen really
// only emits these From* constructors, so this list cannot silently lag.
func emittableSourceKinds() map[string]Source {
	return map[string]Source{
		"FromInput":         FromInput("x"),
		"FromInputOptional": FromInputOptional("x"),
		"FromCtx":           FromCtx("user.id"),
		"FromTarget":        FromTarget("x"),
		"FromConst":         FromConst("x"),
		"FromFn":            FromFn("f", nil),
		"FromCtxOwnedVia":   FromCtxOwnedVia("rel", "owner", "user.id"),
	}
}

// knownStubKinds maps a Source constructor to the reason its runtime arm
// is a documented stub. Empty target means "must not be a stub".
var knownStubKinds = map[string]string{
	"FromTarget": "sourceTarget returns 501 'not yet implemented in runtime spike' " +
		"(handle.go resolveSource) — audit bug #10. Flip to a hard failure once implemented.",
}

// TestResolveSourceHandlesEveryEmittableKind asserts resolveSource has a
// non-stub arm for every codegen-emittable Source kind, EXCEPT the
// documented stubs in knownStubKinds. A new From* constructor with no
// resolveSource arm hits the `default` ("unknown source kind") and is
// caught here.
func TestResolveSourceHandlesEveryEmittableKind(t *testing.T) {
	ctx := fullyPopulatedCtx()
	type in struct{ X string }
	probe := in{X: "v"}

	for name, src := range emittableSourceKinds() {
		_, err := resolveSource(ctx, src, probe)
		le, isLazErr := err.(*Error)

		unknownKind := isLazErr && strings.HasPrefix(le.Message, "unknown source kind:")
		isStub := isLazErr && le.Status == 501

		if unknownKind {
			t.Errorf("source-kind FACE-PARITY violation: codegen emits %s but "+
				"resolveSource has no arm (hit `default`: %s). Add a case to "+
				"resolveSource.", name, le.Message)
			continue
		}

		reason, expectedStub := knownStubKinds[name]
		if isStub && !expectedStub {
			t.Errorf("source-kind FACE-PARITY violation: codegen emits %s but "+
				"resolveSource returns a 501 stub (%s). Implement it or add it to "+
				"knownStubKinds.", name, le.Message)
		}
		if !isStub && expectedStub {
			// The documented stub got implemented — tighten the test.
			t.Errorf("%s is listed in knownStubKinds (%q) but resolveSource no "+
				"longer returns a 501 stub. Remove it from knownStubKinds so the "+
				"guard tightens.", name, reason)
		}
	}
}
