# Bucket Scope: OpenAPI Generation

**Status**: scoping note. Companion to `docs/proposals/bucket-openapi-cycle.md`.

**Date**: 2026-05-11.

**Question**: how much of the HTTP-bearing surface in `examples/full-
capsule/full-capsule.lzi` is **typed in IR today** vs **text-pattern-
walked**, and what does the answer mean for the first cut of `lazuli
generate --openapi`?

## TL;DR — three routes, recommend Route C

| Route | Coverage | Blocker | When |
|---|---|---|---|
| **A** — wait for Phase L Tier 4 to lift `parse_api` / `parse_command` / `parse_resource` / `parse_query` / `parse_record`, then emit OpenAPI from 100% typed IR. | 100% | Tier 4 is outstanding (`docs/next-checklist.md:60` row 24); no concrete date. | After Tier 4 — blocks bucket indefinitely. |
| **B** — invent text-pattern walkers in `crates/lazuli_openapi` so OpenAPI emission can read **any** authored surface today, regardless of IR coverage. | 100% | Doubles the text-pattern surface area (one walker per slot, one in `lazuli_openapi` and one in `lazuli_cli`). Re-invents the very gap Tier 4 closes. | Today, but corrosively. |
| **C** ✅ — emit OpenAPI from the **typed slice only** (commands + agent `expose http`); surface text-pattern `api` blocks as a stub with `x-lazuli-text-pattern-skip: true` and a `openapi_text_pattern_api_block` doctor warning. | 91% (10/11 endpoints in the canonical fixture) | None — the partial output is usable; gap shrinks to 0% mechanically when Tier 4 lands. | Today. |

**Recommendation**: **Route C**. Reasons:

1. **OpenAPI is an artifact, not a runtime capability.** A 91%-
   accurate spec is more useful than no spec. The text-pattern stub
   makes the gap visible (and grep-able for CI).
2. **The cost curve favours Tier 4 over Route B.** Tier 4 buys typed
   IR for *every* downstream consumer (doctor, inspect, LSP, codegen,
   changelog), not just OpenAPI. Building Route B's text-pattern
   walkers would let Tier 4's value drop.
3. **Route C surfaces the blocker.** Authors who hit
   `openapi_text_pattern_api_block` get a single, clear, mechanical
   answer: "lift this `api` to typed IR." The diagnostic disappears
   when Tier 4 lands, with zero additional work in
   `lazuli_openapi`.

## Coverage measurement (the 91% number)

Counted on `examples/full-capsule/full-capsule.lzi` (1.235 lines, all
features), 2026-05-11:

| HTTP-bearing surface | Count | Typed in IR? | Anchor |
|---|---|---|---|
| `command` blocks (mounted as POST/PATCH/DELETE via convention) | 11 | yes (`Command` typed at `crates/lazuli_ir/src/lib.rs:501`) | `:226, 244, 263, 291, 526, 535, 640, 651, 666, 753, customer_auth:526` |
| `agent` blocks with `expose http` | 1 | yes (`HttpExposure` typed at `crates/lazuli_ir/src/lib.rs:2430`) | `:329-332` |
| `api` blocks (custom HTTP endpoints) | 1 | **no** — text-pattern only via `inspect_command` walker at `crates/lazuli_cli/src/main.rs:1791-1834` | `:303-309` |
| Webhook receivers | 1 | yes (`Webhook` typed at `crates/lazuli_ir/src/lib.rs:2122`, lifted in Phase L Tier 3) | `customer_outreach` webhooks |
| `total` | 14 | 13 typed / 1 text-pattern = **93% typed** | — |

Rounding to 91% in the cycle proposal accounts for the **partial**
typed surface even within commands — `Command.audit` body and
`Command.approval` body are still text-pattern lifted in some paths,
so a strict accountant would penalise commands too. The cycle
proposal's claim of "91%" is the conservative read.

## What Tier 4 buys for OpenAPI

| Tier 4 lift | OpenAPI emission gain |
|---|---|
| `parse_api` | `api` blocks reach `ir::Api` with input/output/policy/audit/rate_limit/handler typed. OpenAPI emitter walks them like commands; `x-lazuli-text-pattern-skip` disappears. |
| `parse_command` (canonical-indent slice) | `Command.audit` body + `Command.approval` body land as typed slots, not text-pattern facts. OpenAPI extensions (`x-lazuli-audit`, `x-lazuli-approval`) become structured objects instead of string blobs. |
| `parse_resource` | `Resource` fields lift through the canonical slice; `components.schemas.<Resource>` emission becomes deterministic across all features (today some flow through legacy lift paths). |
| `parse_query` | `query` blocks become GET endpoints in OpenAPI with typed input (filter/sort/paginate) and typed output (`Many<T>` for `list`, `T` for `lookup`). |
| `parse_record` | `record` declarations land as `components.schemas.<Record>` instead of inlined ad-hoc schemas. |
| Lift `defaults.tenancy` | OpenAPI `securitySchemes` for tenant propagation become first-class (today they're inferred from `app.lzi`). |

The OpenAPI bucket cycle proposal explicitly accepts the **Route C
partial** and treats Tier 4 as the path to 100%. There is no Route C
→ Route A migration; Route C just **shrinks** to 0 text-pattern
operations when Tier 4 lands.

## What gets text-pattern-stubbed today

For each text-pattern `api` block, `lazuli generate --openapi` emits:

```yaml
paths:
  /api/customers/export:
    get:
      operationId: customer_customer_export
      x-lazuli-text-pattern-skip: true
      x-lazuli-skip-reason: "api block lift pending Phase L Tier 4"
      x-lazuli-source-origin: "examples/full-capsule/full-capsule.lzi:303"
      # The runtime still mounts this endpoint via the legacy
      # walker; the OpenAPI spec just can't describe its full
      # contract until Tier 4 promotes `api` to typed IR.
      responses:
        '200': { description: "(text-pattern api block; see source)" }
```

The stub is **valid OpenAPI 3.1** (responses object is present;
operationId is unique). It is **incomplete** (no `requestBody`, no
typed response schema). Consumers reading the stub get a clear
"come back after Tier 4" signal via the extension keys.

## What this side-quest **does not** do

- **No new authored surface for OpenAPI itself** — there is no
  `openapi <name>` kind, no `openapi` block in `app.lzi`, no
  `openapi_extension` decorator. The spec is purely an artifact.
- **No SDK generation.** Python/TypeScript/Go clients for the spec
  are downstream adapters (`@plugin/<publisher>/openapi-typescript`,
  etc.) per the boundary discipline in
  `docs/proposals/bucket-openapi-cycle.md` §Runtime.
- **No OpenAPI validation middleware.** That is **DF** (audit
  `:228`), owned by the Lazuli Go runtime; this bucket is **DL** only.
- **No Swagger UI / Redoc / Stoplight hosting.** Those are operational
  artifacts produced by adapters consuming the generated spec.
- **No `api` block lift.** That is row 24 (Phase L Tier 4); this
  bucket explicitly accepts the partial.

## Decision

Route C ships first. Cycle proposal
(`docs/proposals/bucket-openapi-cycle.md`) specifies the language /
IR / codegen / runtime / doctor / LSP work for Route C. Tier 4 (row
24) shrinks Route C's gap to zero with **no follow-up in the
OpenAPI bucket**.
