---
id: 0009
title: Split hostpoint god-files — catalog.lzi + platform.lzi via FEATURE-COHESION-001
type: techspec
status: ready
created: 2026-05-31
depends_on: [0001, 0008]
parallel_safe: false
track: pilot
test_gate: "lazuli check . && lazuli doctor . && go build ./..."
agent: unassigned
---

# TechSpec — Split hostpoint god-files

## Approach
A pure, behavior-preserving re-partition of two hostpoint `.lzi` files, driven by the `FEATURE-COHESION-001` cluster report (spec 0008) and verified by `lazuli inspect` equality. No declaration is rewritten — declarations are *moved* (with their types, policies, rate_limits, audit, translations, field-policies, and `extensions fn` lines intact) into new or existing feature files. `uses` edges and `registry.lzi` / `.lzx` references are fixed to match. The split lands as `parallel_safe: false` because it mutates hostpoint `.lzi`; serialize against any other hostpoint-touching spec.

Repo: `C:\Users\lucas\hostpoint\app`. Line ranges below are from the current `catalog.lzi` (692 LOC) / `platform.lzi` (170 LOC).

## Surface
**Create (new feature dirs under `features/`):**
- `assets/assets.lzi` — the app-wide upload capability.
- `geocoding/geocoding.lzi` — the google-maps adapter call.
- `legal/legal.lzi` — legal documents.
- `data_requests/data_requests.lzi` — GDPR-style export/deletion requests.

**Modify:**
- `catalog/catalog.lzi` — remove the assets / geocoding / dashboard clusters; keep Property/Service/CustomServiceCategory + their CRUD + catalog-only views/queries. Remove the `# doctor:allow LZI-FILE-SIZE-001` waiver (line 5) once slimmed.
- `intelligence/intelligence.lzi` — receive the dashboard read cluster (records + getter commands + `extensions fn`); it already `uses operations, payments, catalog`.
- `platform/platform.lzi` — **delete** (its three resources move out; nothing cohesive remains). If a `feature_flags` home is created for `PlatformConfig`, do it explicitly; default is delete `PlatformConfig`.
- `traveler/traveler.lzi` — add `uses assets` and repoint any upload reference to the new feature.
- Handler dirs: move `catalog/handlers/{request_asset_upload,confirm_asset_upload}.go` → `assets/handlers/`; `catalog/handlers/lookup_geocode.go` → `geocoding/handlers/`; `catalog/handlers/{get_service_dashboard,get_property_dashboard}.go` → `intelligence/handlers/`; `platform/handlers/request_data_action.go` → `data_requests/handlers/`. Update `RegisterFn("<feature>.<name>")` namespaces accordingly.
- `registry.lzi` — update any feature references; `object_store`/`google_maps` bindings now logically serve `assets`/`geocoding` (binding names unchanged; only the consuming feature moved).
- Any `.lzx` surface referencing the moved commands/records — repoint to the new feature namespace.

## Contracts

**`catalog.lzi` → split map (cite line ranges):**
| New home | Declarations | catalog.lzi ranges |
|----------|--------------|--------------------|
| `assets` | `AssetKind`/`AssetStatus` enums + `AssetUploadIntent` record + `UploadedAsset` resource + `request_asset_upload`/`confirm_asset_upload` commands + `UploadedAsset` field-policy + their `fn` | enums/record ~48-64; resource ~115-130; field policy ~385-391; commands ~592-610; extensions ~684-685 |
| `geocoding` | `GeocodeResult` record + `lookup_geocode` command + `fn` | record ~255-259; command ~620-627; extension ~687 |
| `intelligence` (existing) | `ServiceDashboardView` / `PropertyDashboardTopService` / `PropertyDashboardSnapshot` records + `get_service_dashboard`/`get_property_dashboard` commands + their `fn` | records ~302-333; commands ~647-662 |
| `catalog` (kept) | Property / Service / CustomServiceCategory + CRUD + catalog views (`PropertyDetailView`/`PropertyCardView`/`ServiceDetailView`) + catalog queries | remainder |

**`platform.lzi` → split map:**
| New home | Declarations |
|----------|--------------|
| `legal` | `LegalDocKind` enum + `LegalDocSection` record + `LegalDoc` resource + `legal_doc_current` query |
| `data_requests` | `DataRequestKind`/`DataRequestStatus` enums + `DataRequestPayload` record + `DataRequest` resource + `request_data_action` command + `mine_data_requests` query + `fn` |
| (deleted) | `PlatformConfig` — orphaned write-target, no query reads it; delete (or move to a real `feature_flags` only if a reader exists) |

**Behavior-equality oracle:** capture `lazuli inspect` (full projection: commands, queries, policies, errors, routes) before and after. The diff must be **only** feature-name re-attribution — no added/removed/changed command, query, policy, error code, or route. Any other diff blocks the split.

**Cohesion oracle:** after the split, `lazuli doctor .` reports **zero** `FEATURE-COHESION-001` findings on `assets`/`geocoding`/`catalog`/`intelligence`/`legal`/`data_requests`.

**Non-goal guard (explicit):** `account.lzi` (591), `payments.lzi`, `operations.lzi`, and Pauta `media_price_tables.lzi` (686) are single connected components — `FEATURE-COHESION-001` does not fire on them. **Do not split them.** Their LOC is incidental cohesion, not a violation.

## Plan — for the executing agent
For **each** of the six target features (assets, geocoding, intelligence-receive, catalog-slim, legal, data_requests):
1. Capture `lazuli inspect` baseline (commit the JSON to compare).
2. Move the declarations (with all modifiers intact) into the destination `.lzi`; move the matching Go handlers and fix `RegisterFn` namespace.
3. Fix `uses` on both sides (e.g. `catalog` no longer needs whatever only assets/geocoding used; `traveler uses assets`; `intelligence` keeps its existing uses).
4. Run `lazuli check . && lazuli doctor . && go build ./...` → green.
5. Diff `lazuli inspect` against the baseline → only feature-name re-attribution.
6. Update `registry.lzi` + any `.lzx` surfaces referencing moved names.
7. Once `catalog.lzi`/`platform.lzi` are slimmed/removed, delete the `# doctor:allow LZI-FILE-SIZE-001` waiver on the affected file(s).
Then a final full-repo `lazuli check . && lazuli doctor . && go build ./...` and one consolidated `lazuli inspect` diff.

## Tests first (TDD)
- [ ] `inspect_baseline_captured` — `lazuli inspect` JSON snapshotted before any move.
- [ ] `catalog_split_inspect_equal` — after the catalog split, inspect diff is feature-name-only.
- [ ] `platform_split_inspect_equal` — after the platform split, inspect diff is feature-name-only (PlatformConfig removal shows as a dropped *write-target with no reader*, confirming it was dead).
- [ ] `cohesion_clean` — `lazuli doctor .` reports no `FEATURE-COHESION-001` on any resulting feature.
- [ ] `go_build_green` — `go build ./...` compiles after handler moves + `RegisterFn` namespace fixes.
- [ ] `waivers_removed` — no `# doctor:allow LZI-FILE-SIZE-001` remains on catalog/platform-derived files, and doctor stays green without them.
- [ ] `cohesive_files_untouched` — `git diff` shows zero changes to `account.lzi`/`payments.lzi`/`operations.lzi`.

## Gate

### Definition of Done (Lazuli feature gate)
1. BUILD: implemented; `cargo test -p <crate>` green for the new rule/grammar/codegen.
2. MIGRATE: every pilot that needed it is on it; `lazuli check && lazuli doctor && go build ./...` clean in hostpoint and/or pauta-web.
3. TEACH: docs/lazuli_way/<slug>.md filled (idiom → before/after pilot excerpt → enforcing doctor rule); scaffold CLAUDE.md.tmpl + AGENTS.md.tmpl bullet added.
4. ENFORCE: a doctor rule fires on the old hand-rolled shape OR the scaffold seed demonstrates the idiom. The rule code is named in the idiom doc.
A spec that skips gate 3 or 4 is NOT done. The RULE-team grader blocks it.

**Four concrete gates:**
1. **BUILD** — n/a for new language code; the "build" here is the pilot compiling: `go build ./...` green in hostpoint after all moves.
2. **MIGRATE** — `lazuli check . && lazuli doctor . && go build ./...` clean in hostpoint; `lazuli inspect` diff is feature-name-only (behavior preserved); four `# doctor:allow LZI-FILE-SIZE-001` waivers removed.
3. **TEACH** — the `one-feature-one-capability.md` idiom doc (filled by 0008) now cites this split as its concrete "After"; no new doc owned here, but the before/after excerpt is this PR's diff (cross-link from the idiom doc to `features/assets`, `features/legal`).
4. **ENFORCE** — `FEATURE-COHESION-001` fires on the pre-split `catalog.lzi`/`platform.lzi` (proven in 0008's MIGRATE gate) and is **silent** on all six resulting features (proven here).

## Risks & rollback
- **A "move" silently becomes a rewrite** (e.g. dropping a `rate_limit` or `when_denied`) → caught by the `lazuli inspect` diff; that's the whole point of the oracle. Never merge with a non-attribution diff.
- **`RegisterFn` namespace drift** after handler moves → `go build` + a runtime smoke check catch unregistered handlers.
- **`PlatformConfig` actually has a reader somewhere** (e.g. a `.lzx` surface or Go handler) → the inspect diff / `go build` will surface it; promote to `feature_flags` instead of deleting if so.
- **`.lzx` surfaces reference moved commands** → grep for the moved command/record names across `.lzx`; repoint before the final doctor run.

**Rollback:** `git revert` the split commit(s) — every change is a move within hostpoint; reverting restores the god-files (and their waivers) exactly. No framework code changes here.
