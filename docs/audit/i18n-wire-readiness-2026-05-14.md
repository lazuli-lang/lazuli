# I18n Bucket Wire-Thin Audit

- **Date:** 2026-05-14
- **Auditor:** Codex
- **Source commit:** `2781f42eb7915f4e35cb20a20748c4d52d99a02d`
- **Bucket:** `runtime/go/lazuli/i18n/`

---

## File Table

| File | Eff. LOC | External Imports | Verdict | Target Library | Est. Cell |
|---|---|---|---|---|---|
| `context.go` | 12 | _none_ | **wire** | — | — |
| `contract.go` | 56 | _none_ | **wire** | — | — |
| `negotiate.go` | 36 | _none_ | **wire** | — | — |

---

## Summary

**3/3 files (100.0%) are wire-clean.** The bucket is currently small contract/glue code: context propagation, locale contract resolution, and a lightweight Accept-Language negotiation helper. No file crosses the wire-thin violation threshold, and no file should be deleted because these APIs are the runtime surface generated handlers and HTTP middleware consume.

### Top 3 risks for downstream product ports

1. **`negotiate.go` simplified matching** — The current parser is intentionally small and does not provide CLDR-aware matching, q-value sorting, or canonicalization. Pleiades/Atelier/Erudito or the Hostpoint port can start with it, but richer locale negotiation should eventually wire `golang.org/x/text/language` if product behavior depends on exact language priority.

2. **`contract.go` fallback graph is string-based** — The fallback walker is fine at this size, but invalid tags or misspelled fallback edges are only resolved at runtime. Codegen or doctor checks should keep validating `app.locale` so downstream apps do not silently fall back to the default locale.

3. **No renderer/catalog loader in this bucket** — `Catalog` only carries `embed.FS` metadata; ICU message formatting, plural selection, and external translation tooling are intentionally absent. If a product needs Lokalise, Crowdin, Phrase, or another vendor workflow, that adapter belongs in `@plugin/<name>` on consumption rather than in this runtime bucket.

---

## Punch List (Codex cells)

None.

---

## Wire-Thin Scorecard

| Metric | Value |
|---|---|
| Files audited | 3 |
| Wire-clean | 3 (100.0%) |
| Rewrite-as-wire | 0 |
| Questionable | 0 |
| Delete-candidate | 0 |
| Codex cells generated | 0 |
| Hostpoint-blocker risk | Low |
