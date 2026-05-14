# Maps Bucket Wire-Thin Audit

- **Date:** 2026-05-14
- **Auditor:** Codex
- **Source commit:** `2781f42eb7915f4e35cb20a20748c4d52d99a02d`
- **Bucket:** `runtime/go/lazuli/maps/`

---

## File Table

| File | Eff. LOC | External Imports | Verdict | Target Library | Est. Cell |
|---|---|---|---|---|---|
| `contract.go` | 28 | _none_ | **wire** | - | - |

---

## Summary

**1/1 files (100.0%) are wire-clean.** `contract.go` is a small framework contract for geocoding providers, not a provider implementation. Concrete Google Maps, Mapbox, HERE, Nominatim, or similar adapters should stay out of `@runtime/maps` and move to `@plugin/<name>` on consumption.

### Top 3 risks for downstream product ports

1. **Provider adapter boundary** - Pleiades or the Hostpoint port will likely need Google Maps direct, but that implementation must land in `@plugin/google-maps` rather than expanding this runtime bucket.

2. **Contract may be too narrow** - The current interface covers geocode and reverse-geocode only. Product ports may need static maps, routes, autocomplete, distance matrix, or place details; those should be added as narrow contracts only when consumed.

3. **No runtime call sites yet** - The bucket is currently a stable adapter seam with zero callers in `runtime/go/lazuli/`. First consumption should add a small integration test or fixture proving generated code can bind a maps plugin without importing vendor SaaS into core.

---

## Wire-Thin Scorecard

| Metric | Value |
|---|---|
| Files audited | 1 |
| Wire-clean | 1 (100.0%) |
| Rewrite-as-wire | 0 |
| Questionable | 0 |
| Delete-candidate | 0 |
| Codex cells generated | 0 |
| Downstream-blocker risk | Low (provider implementation belongs in `@plugin/<name>` on consumption) |
