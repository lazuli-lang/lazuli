# Payments Bucket Wire-Thin Audit

- **Date:** 2026-05-14
- **Auditor:** Codex
- **Source commit:** `2781f42eb7915f4e35cb20a20748c4d52d99a02d`
- **Bucket:** `runtime/go/lazuli/payments/`

---

## File Table

| File | Eff. LOC | External Imports | Verdict | Target Library | Est. Cell |
|---|---|---|---|---|---|
| `contract.go` | 38 | _none_ | **wire** | — | — |

---

## Summary

**1/1 files (100.0%) are wire-clean.** The bucket is currently just a 38-eff-LOC contract surface with no external imports and no homegrown payment-provider logic. MercadoPago, Stripe, Pagar.me, PayPal, Pix-direct, and similar provider implementations are moved to `@plugin/<name>` on consumption.

### Top 3 risks for the Pleiades/Atelier/Erudito or Hostpoint ports

1. **Contract still experimental** — The interface is intentionally minimal and has no in-repo consumer yet, so the first real product port may need additive fields for installments, idempotency headers, split payments, or refund flows.

2. **Provider semantics live outside runtime** — This is the right boundary, but product ports must ensure the chosen `@plugin/<name>` adapter owns webhook signature validation, payload mapping, and provider retry semantics instead of pushing that logic back into `runtime/go/lazuli/payments/`.

3. **No runtime caller coverage** — There are no callers elsewhere in `runtime/go/lazuli/`, so the contract can drift unnoticed until a downstream plugin or product port compiles against it. The first plugin should add conformance tests in its own repo and keep this core contract thin.

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
| Product-port blocker risk | Low (contract-only; provider implementation belongs in `@plugin/<name>`) |
