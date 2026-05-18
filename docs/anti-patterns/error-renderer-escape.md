# Anti-pattern — `lazuli.RegisterErrorRendererEscape`

Status: documented escape hatch, **not** an authoring surface.
See `docs/proposals/ir-error-messages-vocab.md` §9.4 for the design
rationale; Cell RUNTIME-3 ships the implementation.

## What it is

`lazuli.RegisterErrorRendererEscape(fn func(*lazuli.Error, string) string)`
is a process-global, locale-aware hook installed at boot. It runs
**before** the four-layer resolver chain (proposal §2.E). If `fn`
returns a non-empty string, that text wins on the wire; if it returns
empty, the chain proceeds normally. Idempotent — the last
registration wins; pass `nil` to clear.

The escape is one Go function and a single registration point. No
`.lzi` surface, no codegen output, no plugin namespace. By design it
is **invisible** to a cold-readable audit of `.lzi` files and to
every doctor diagnostic.

## When to use it

Three legitimate use cases. All three share one trait: the wire text
**cannot** be declared declaratively because it varies per-process,
per-tenant, or per-deployment in ways the `.lzi` source cannot
express.

- **White-label SaaS hosting.** A single binary serves many tenants;
  each tenant brands every error string differently. The tenant
  identity comes from request context, not from the `.lzi` file.
- **COTS / OEM rebranding.** A product is licensed to a partner who
  ships under their own brand. Every framework-emitted string must
  pass through the partner's voice / legal review before it reaches
  end users. The partner ships its own boot binary.
- **Legal-team-mandated wording overrides.** A regulator (e.g. a
  banking authority) requires specific wording for every 4xx error
  in a jurisdiction. The wording is binary policy: the `.lzi` is the
  same across jurisdictions, only the wire bytes differ at runtime.

If your use case is not one of these three, **do not use the
escape**. The `.lzi` `errors` block + `when_denied @translation.<key>`
surface is the canonical path.

## When NOT to use it — `[do | don't]`

| Symptom / intent                                                       | Don't (escape)                                          | Do (canonical surface)                                                                            |
|------------------------------------------------------------------------|---------------------------------------------------------|---------------------------------------------------------------------------------------------------|
| "I want to change one error message on one command."                   | Don't intercept it from `RegisterErrorRendererEscape`.  | Author `command X policy ... when_denied @translation.<key>` in the feature's `.lzi`.             |
| "There's a typo in the built-in PT-BR copy."                           | Don't shim it via the escape.                           | Edit `runtime/go/lazuli/i18n/builtin.pt-BR.json` and open a PR against the framework.             |
| "I want to add a new language (fr-FR / ja-JP / …)."                    | Don't translate inside the escape on the fly.           | Ship `@plugin/lazuli-i18n-locales-<lang>` per proposal §9.4 footnote, or contribute a builtin.    |
| "I want a 4xx field exposed that the contract currently hides."        | Don't reformat the message to smuggle the field inline. | Adjust `expose client 4xx <fields>` in the feature's `errors` block (§2.G).                       |
| "I want to inject HTML / Markdown into the error message."             | Don't return HTML from the escape.                      | The wire envelope is JSON. HTML in the `message` field is a content-type lie. Use a UI component. |
| "I want different copy per environment (dev / staging / prod)."        | Don't branch on env inside the escape.                  | Use `app.observability.error_source` + per-env `.lzi` overrides; keep the wire string stable.     |
| "I want to log every resolved error before it ships."                  | Don't piggy-back logging on the escape.                 | Add a middleware that observes responses, or use `app.observability` policy.                      |
| "I want to redact a PII field from the message."                       | Don't string-rewrite in the escape.                     | Tag the field `@pii.*` and let the framework's exposure rules do the redaction.                   |

## What you lose by using it

The escape overrides the resolved wire text **opaquely**. The
framework cannot tell what your `fn` returned, only that it returned
non-empty. As a consequence, every doctor diagnostic and audit
projection that depends on knowing the resolved text goes **silent
or stale** for any error your escape covers:

- `ERR-VOCAB-001` (missing `when_denied` on a user-clickable command)
  — silent. The escape's output bypasses the catalog entirely; the
  doctor cannot prove the wire string came from a declared key.
- `ERR-VOCAB-002` (cross-check `@translation.<key>` against
  `Translation.keys[]`) — silent. The escape may render text that
  was never declared in any catalog.
- `ERR-VOCAB-005` (built-in coverage check) — silent for codes the
  escape overrides.
- `lazuli inspect --expand=error-resolution-table` — the projection
  shows the catalog row, but the runtime wire is decoupled from it.
  Audits become misleading: the catalog row is *correct* and *unused*.
- `LSP hover-resolve` — the hover preview of an error code shows the
  catalog text, but the deployed runtime ships the escape's text.
  Authors and reviewers see one thing; users see another.
- `lazuli doctor exposure check` — cannot verify that the
  `expose client 4xx <fields>` contract is honoured for the wire
  payload your escape produced.

In short: **the escape decouples the source-of-truth `.lzi` audit
from the runtime wire bytes**. That is its purpose for the three
legitimate use cases above; for any other use it is a hidden
inconsistency that no static tool can catch.

## Reference

- Design rationale and rejection of the escape as a primary surface:
  `docs/proposals/ir-error-messages-vocab.md` §9.4.
- Implementation cell: `docs/proposals/ir-error-messages-vocab.md` §11
  Cell RUNTIME-3.
- Resolver chain the escape pre-empts: same proposal §2.E.
- Canonical authoring surface this escape is meant to replace
  (do **not** skip it): same proposal §2.A — §2.D.
