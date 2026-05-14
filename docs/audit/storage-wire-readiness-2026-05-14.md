# Storage Bucket Wire-Thin Audit

- **Date:** 2026-05-14
- **Auditor:** Codex
- **Source commit:** `2781f42eb7915f4e35cb20a20748c4d52d99a02d`
- **Bucket:** `runtime/go/lazuli/storage/`

---

## File Table

| File | Eff. LOC | External Imports | Verdict | Target Library | Est. Cell |
|---|---|---|---|---|---|
| `builders.go` | 42 | _none_ | **wire** | - | - |
| `contract.go` | 62 | _none_ | **wire** | - | - |
| `fetch_private.go` | 23 | _none_ | **wire** | - | - |
| `minio.go` | 107 | `minio-go/v7`, `minio-go/v7/pkg/credentials` | **wire** | - | - |
| `signed.go` | 50 | _none_ | **wire** | - | - |
| `upload.go` | 312 | `aws-sdk-go-v2` x3, `smithy-go` | **wire** | - | - |

### Namespace note

`minio.go` is wire-thin because it delegates to `github.com/minio/minio-go/v7` instead of reimplementing S3-compatible storage. It is still a named product/client and should be moved to `@plugin/minio` on consumption rather than expanded inside `@runtime/storage`. This audit does not flag it as `rewrite-as-wire` because the target library is already present and the plugin-boundary issue is namespace placement, not homegrown logic.

---

## Summary

**6/6 files (100.0%) are wire-clean by the CLAUDE.md wire-thin test.** No file satisfies the hard violation rule of >100 effective LOC, zero external imports, and an available mature Go library. The bucket is mostly adapter glue: contract/value helpers, visibility guards, local-dev storage, AWS SDK S3 wiring, and a MinIO client wrapper that should remain plugin-bound when consumed.

### Top 3 risks for the Hostpoint storage port

1. **`upload.go` concentration risk** - The file is 312 effective LOC because it houses `ObjectStore`, `LocalStore`, `S3Store`, MIME parsing, key minting, token minting, and a small byte reader. This is not a wire-thin violation because the S3 surface wires `aws-sdk-go-v2`, but future storage work should avoid adding more behavior here.

2. **`minio.go` namespace boundary** - The MinIO adapter has no callers outside its own package tests and is a named provider/client. If Hostpoint needs MinIO specifically, consume it through `@plugin/minio`; if it only needs S3-compatible storage, prefer the existing S3 endpoint hook in `S3Store`.

3. **Local signed URL tokens are dev-only semantics** - `LocalStore.Sign` returns deterministic in-memory tokens rather than real URLs. That is acceptable for tests and local development, but generated product download paths must bind production stores with real presigning before relying on signed visibility behavior.

---

## Punch List (Codex cells)

No rewrite cells generated. Storage has no `rewrite-as-wire` files under the current audit rule.

---

## Wire-Thin Scorecard

| Metric | Value |
|---|---|
| Files audited | 6 |
| Wire-clean | 6 (100.0%) |
| Rewrite-as-wire | 0 |
| Questionable | 0 |
| Delete-candidate | 0 |
| Codex cells generated | 0 |
| Hostpoint-blocker risk | Low (watch MinIO namespace placement and production signed URL binding) |
