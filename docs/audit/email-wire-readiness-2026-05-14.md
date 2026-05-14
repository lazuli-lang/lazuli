# Email Bucket Wire-Thin Audit

- **Date:** 2026-05-14
- **Auditor:** Codex
- **Source commit SHA:** `2781f42eb7915f4e35cb20a20748c4d52d99a02d`
- **Bucket path:** `runtime/go/lazuli/email/`

---

## File Table

| File | Eff. LOC | External Imports | Verdict | Target Library | Est. Cell |
|---|---|---|---|---|---|
| `sendgrid.go` | 37 | 2 (`github.com/sendgrid/sendgrid-go`, `.../helpers/mail`) | **wire** | — | — |
| `smtp.go` | 33 | 0 | **wire** | — | — |

---

## Summary

**2/2 files (100.0%) are wire-clean by the CLAUDE.md LOC/import test.** No file is over 100 effective LOC with zero external imports, so there are no `rewrite-as-wire`, `questionable`, or `delete-candidate` calls in this bucket.

### Top 3 risks for the Hostpoint/Pleiades/Atelier/Erudito ports

1. **`sendgrid.go` is a named SaaS adapter in a runtime bucket** — mechanically it is wire-thin because it wraps the official SendGrid SDK in 37 effective LOC, but SendGrid belongs under `@plugin/sendgrid`, not `@runtime/email`. Moved to `@plugin/sendgrid` on consumption; do not expand this adapter inside the core runtime.

2. **`smtp.go` uses the minimal stdlib SMTP surface** — `net/smtp` keeps the adapter thin, but it does not cover provider-specific TLS/auth/deliverability behavior. Product ports that need Gmail, SES, SendGrid, Mailgun, or Postmark specifics should bind plugins instead of growing this file.

3. **Email rendering is intentionally out of bucket scope** — the bucket only dispatches already-rendered text/html bodies. Ports that need localization, template inheritance, attachments, tracking headers, or delivery metadata should keep that logic in generated app code or provider plugins, not in `runtime/go/lazuli/email/`.

---

## Punch List (Codex cells)

No core-runtime rewrite cells. The only follow-up is an orchestrator/plugin-boundary cleanup if the SendGrid adapter is extracted to a separate `@plugin/sendgrid` repo.

---

## Wire-Thin Scorecard

| Metric | Value |
|---|---|
| Files audited | 2 |
| Wire-clean | 2 (100.0%) |
| Rewrite-as-wire | 0 |
| Questionable | 0 |
| Delete-candidate | 0 |
| Codex cells generated | 0 |
| Hostpoint-blocker risk | Low (namespace cleanup only; no wire-thin violation) |
