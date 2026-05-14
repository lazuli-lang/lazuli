# Registry Top-Level Wire-Thin Audit

- **Date:** 2026-05-14
- **Auditor:** Codex
- **Source commit:** `55e117555a1db8c4ed960a8fcfa52f9e064419b4`
- **Topic:** `runtime/go/lazuli/` top-level registry files for plugin adapter lookup, typed declaration registration, and legacy dispatcher registration.

---

## File Table

| File | Eff. LOC | External Imports | Verdict | Target Library | Est. Cell |
|---|---|---|---|---|---|
| `plugin_registry.go` | 46 | _none_ | **wire** | — | — |
| `registry_typed.go` | 214 | _none_ | **questionable** | see note | — |
| `register.go` | 149 | _none_ | **questionable** | see note | — |

### registry_typed.go note

`registry_typed.go` is over the wire-thin LOC threshold with zero external imports, but the code is framework-specific boot registry glue: typed snapshots, duplicate-registration guards, API registration, and compatibility syncing into the legacy erased dispatcher registry. There is no obvious OSS library that should own Lazuli's declaration vocabulary or generic `Resource` / `Command` / `Query` registration shape. Verdict: **questionable but acceptable**; the main cleanup risk is reducing duplication with `register.go`, not replacing it with a third-party package.

### register.go note

`register.go` is also over the LOC threshold with zero external imports, but it is the process-global declaration registry and dispatcher bridge used by generated Lazuli Go code. Its JSON decoding is a small boundary adapter around `encoding/json`, not a custom serialization framework, and the HTTP layer consumes its command/query handler lookup directly. Verdict: **questionable but acceptable**; future work should converge this legacy erased registry with `GlobalRegistry` once codegen no longer needs both paths.

---

## Summary

**1/3 files (33.3%) are wire-clean.** The two non-wire-clean files are both framework-specific registries rather than reimplementations of known OSS libraries, so there are no rewrite-as-wire targets in this topic. Test coverage exists for plugin adapter registration and typed mismatch handling in `plugin_registry_test.go`; the larger typed/legacy registry bridge relies mostly on downstream HTTP, retention, and generated-code integration coverage.

### Top 3 risks for downstream product ports

1. **Dual registry surfaces** — `registry_typed.go` keeps `GlobalRegistry` in sync with the legacy `registry` from `register.go`. Pleiades/Atelier/Erudito will all consume the top-level package, so drift between typed snapshots and erased dispatcher maps could create boot-time or route-registration discrepancies.

2. **Panic-only duplicate handling** — resource, command, query, and API registration all fail via `panic` on duplicate names. That is normal for generated `init()` paths, but product ports with multiple plugin imports or feature packages may get hard process crashes instead of actionable diagnostics.

3. **Sparse direct tests for the bridge** — plugin adapter lookup has focused tests, but the typed-to-legacy sync path and `Register(...)` dispatcher behavior are not covered in this topic's own test file. Regressions would likely be caught indirectly by HTTP/runtime tests rather than by a small registry-local suite.

---

## Wire-Thin Scorecard

| Metric | Value |
|---|---|
| Files audited | 3 |
| Wire-clean | 1 (33.3%) |
| Rewrite-as-wire | 0 |
| Questionable | 2 (`registry_typed.go`, `register.go`) |
| Delete-candidate | 0 |
| Codex cells generated | 0 |
| Downstream-blocker risk | Medium (framework-specific dual registry surface; no wire-thin rewrite required) |
