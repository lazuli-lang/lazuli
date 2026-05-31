---
id: 0009
title: Split hostpoint god-files — catalog.lzi + platform.lzi via FEATURE-COHESION-001
type: adr
status: accepted
created: 2026-05-31
supersedes: —
---

# ADR — Split by disconnected cluster, preserve behavior via `lazuli inspect` equality; delete the orphan

## Context
- `FEATURE-COHESION-001` (spec 0008) gives a precise, per-cluster decomposition target. `catalog.lzi` resolves into four clusters (catalog CRUD; the `UploadedAsset` upload cluster; the `GeocodeResult` geocoding call; the dashboard read records). `platform.lzi` resolves into three isolated nodes (`LegalDoc`, `DataRequest`, `PlatformConfig`) with zero edges.
- Hostpoint is the canonical shape; a behavior-changing refactor here is unacceptable. But `lazuli inspect` projects the full effect/policy/error surface from the IR, independent of file partitioning — so a move-only refactor that keeps every declaration intact must produce a byte-stable (modulo feature-name attribution) inspect projection. That makes `inspect` the equality oracle for "no behavior change."
- `PlatformConfig` is a write-target with no query reading it — an orphan. Keeping it "somewhere" just relocates dead weight; the honest options are delete or give it a real home (`feature_flags`) only if a reader is actually intended.
- The dashboard read cluster (`ServiceDashboardView`, `PropertyDashboardSnapshot`, their getters) is analytics over operations + payments + catalog data — which is precisely what `intelligence` already owns (`uses operations, payments, catalog`). It belongs there, not in a new feature.

## Decision
- **Split along the cohesion clusters, not arbitrarily.** Each new feature = one connected component:
  - `assets` ← `UploadedAsset` + `AssetKind`/`AssetStatus` + `AssetUploadIntent` record + `request_asset_upload`/`confirm_asset_upload` + the `UploadedAsset` field-policy (bucket/object_key system-only). Cited ranges: enums/record ~48-64; resource ~115-130; field policy ~385-391; commands ~592-610; extensions ~684-685.
  - `geocoding` ← `GeocodeResult` record + `lookup_geocode` command + the `geocoder` binding usage. Cited ranges: record ~255-259; command ~620-627; extension ~687.
  - `catalog` (slimmed) ← Property / Service / CustomServiceCategory + their CRUD + the catalog-specific views/queries.
  - **Move dashboard reads into `intelligence`** (not a new feature): `ServiceDashboardView` / `PropertyDashboardTopService` / `PropertyDashboardSnapshot` + `get_service_dashboard` / `get_property_dashboard`. Cited ranges: records ~302-333; commands ~647-662.
  - `legal` ← `LegalDoc` + `LegalDocSection` + `legal_doc_current`.
  - `data_requests` ← `DataRequest` + `DataRequestPayload` + `DataRequestKind`/`DataRequestStatus` + `request_data_action` + `mine_data_requests`.
- **Delete `PlatformConfig`** (default): it has no reader, so deleting it is behavior-preserving (nothing reads it; the write-target is dead). Promote to a `feature_flags` feature *only if* a reader is found during execution — checked, not assumed.
- **Behavior equality is gated by `lazuli inspect` diff.** Before/after the splits, `lazuli inspect` must show the same command/query/policy/error/route surface. Any diff that isn't pure feature-name re-attribution blocks the split.
- **Do not touch the cohesive large files.** `account` / `payments` / `operations` / `media_price_tables` are single connected components; `FEATURE-COHESION-001` is silent on them; their size is incidental. Explicit non-goal so the refactor stays surgical.

## Alternatives considered
- **Leave the waivers, ship more features** — rejected: the canonical app would keep teaching that bundling + waiving is acceptable, undercutting 0008's whole point.
- **Put dashboard reads in a new `dashboards` feature** — rejected: `intelligence` already owns analytics over the exact same upstream features; a new feature would itself need `uses operations, payments, catalog` and would just duplicate intelligence's seams. Move into the existing home.
- **Keep `PlatformConfig` in a slimmed `platform`** — rejected: that re-creates a single-resource feature for a resource nothing reads; dead weight with a name. Delete (or give it a reader).
- **Split LOC-evenly** — rejected: the split must follow the relation graph (the cohesion clusters), not line balance; otherwise the new features aren't single capabilities either.

## Consequences
**We accept:** more, smaller feature files (assets, geocoding, legal, data_requests added; catalog + platform slimmed/removed) and the `uses` graph grows a few edges (traveler → assets; catalog → assets/geocoding; intelligence gains records). One-time churn across `registry.lzi` + any `.lzx` surfaces that referenced moved names.
**We gain:** a `FEATURE-COHESION-001`-clean hostpoint; reusable `assets` capability for traveler; honestly-named features a cold reader can trust; four `# doctor:allow LZI-FILE-SIZE-001` waivers removed; the canonical app demonstrates the idiom instead of waiving it.
**We watch:** `lazuli inspect` diff is the safety net — if it shows any surface change, a declaration was rewritten rather than moved; stop and fix before merge.
