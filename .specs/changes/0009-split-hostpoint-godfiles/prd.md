---
id: 0009
title: Split hostpoint god-files — catalog.lzi + platform.lzi via FEATURE-COHESION-001
type: prd
stage: 9 of 17
status: ready
created: 2026-05-31
---

# PRD — Split hostpoint god-files

## Problem
Hostpoint has two `.lzi` files that `FEATURE-COHESION-001` (spec 0008) flags as bundling independent capabilities:
- **`catalog.lzi` (692 LOC).** Beyond the actual catalog (Property / Service / CustomServiceCategory CRUD), it carries an app-wide **upload** capability (`UploadedAsset` + `AssetKind`/`AssetStatus` + `request_asset_upload`/`confirm_asset_upload`) that the traveler feature also needs, a **geocoding** adapter call (`GeocodeResult` + `lookup_geocode`, a google-maps escape), and a **dashboard** read cluster (`ServiceDashboardView` / `PropertyDashboardSnapshot` + their getters) that is analytics, not catalog. Four concerns in one file — which is exactly why it carries the `# doctor:allow LZI-FILE-SIZE-001` waiver at line 5.
- **`platform.lzi` (170 LOC).** Three disconnected resources: `LegalDoc` (+ `legal_doc_current`), `DataRequest` (+ `request_data_action` + `mine_data_requests`), and an orphaned `PlatformConfig` (a write-target with **no query reading it**). No FK / `has_many` / `on_delete` edge links any pair. The worst cohesion violation in the pilot despite being small.

These are not LOC problems — they're capability-bundling problems, now mechanically detectable.

## Why now (or why ever)
Hostpoint is the canonical shape (the Basecamp-to-Rails pilot). A god-file in the canonical app teaches every future app that bundling is fine, and the `# doctor:allow LZI-FILE-SIZE-001` waivers normalize silencing the decomposition signal. 0008 just shipped the precise rule; 0009 is the proof that the rule drives a real, behavior-preserving refactor on the reference codebase — and it unblocks the `traveler` feature's reuse of the upload capability and the `intelligence` feature's ownership of the dashboard reads.

## Outcome — done means
1. **`catalog.lzi` splits into:**
   - `assets` — `UploadedAsset` + `AssetKind`/`AssetStatus` + `request_asset_upload`/`confirm_asset_upload` (the app-wide upload capability, reusable by traveler).
   - `geocoding` — `GeocodeResult` + `lookup_geocode` (the google-maps adapter call).
   - `catalog` (slimmed) — Property / Service / CustomServiceCategory CRUD only.
   - the **dashboard read cluster** (`ServiceDashboardView` / `PropertyDashboardSnapshot` + getters) moves into the existing `intelligence` feature (which already `uses operations, payments, catalog`).
2. **`platform.lzi` splits into:**
   - `legal` — `LegalDoc` + `legal_doc_current`.
   - `data_requests` — `DataRequest` + `request_data_action` + `mine_data_requests`.
   - `PlatformConfig` (orphaned write-target, no reader) → **deleted, or** moved to a real `feature_flags` feature if a reader is intended.
3. After every split: `lazuli check . && lazuli doctor . && go build ./...` is green; `FEATURE-COHESION-001` no longer fires on any resulting feature; the `# doctor:allow LZI-FILE-SIZE-001` waivers on the affected files are removed (no longer needed).
4. `lazuli inspect` diff shows **no behavior change** — same commands, queries, policies, error catalog, route surface; only the feature partitioning changed.
5. `registry.lzi` and any `.lzx` surfaces referencing the moved declarations are updated.

## Non-goals
- **Splitting the legit-large cohesive files.** `account.lzi` (591), `payments.lzi`, `operations.lzi`, and Pauta's `media_price_tables.lzi` (686) are single connected capabilities — `FEATURE-COHESION-001` does not fire on them and their LOC is incidental. **Do not split them.** This is called out explicitly so a zealous executor doesn't "while I'm here" them.
- Any behavior change. This is a pure re-partition; `lazuli inspect` is the behavior-equality oracle.
- Pauta-web. This spec is hostpoint-only.
- Building new dashboard/analytics logic — the dashboard cluster *moves* to `intelligence` as-is.

## User stories
- As the traveler feature author, after the split I `uses assets` and reuse `request_asset_upload` instead of reaching across into `catalog`.
- As a cold reader auditing hostpoint, opening `catalog.lzi` shows only catalog — the upload, geocoding, and analytics concerns live in honestly-named features.
- As the framework maintainer, `lazuli doctor .` on hostpoint is `FEATURE-COHESION-001`-clean, proving the rule drives a real refactor on the canonical app.

## Constraints
- Move declarations, don't rewrite them — keep types, policies, rate_limits, audit, translations intact so `lazuli inspect` stays equal.
- Fix every `uses` edge the moves imply (catalog → assets/geocoding; traveler → assets; intelligence gains the dashboard records).
- `parallel_safe: false` — this contends on hostpoint `.lzi` files; serialize against any other hostpoint-touching spec.

## Open questions
- `PlatformConfig`: delete vs. promote to `feature_flags`? Resolve in the ADR — default to delete unless a reader exists.
