# Proposal: Cut A.11 — CORS declaration in `app.lzi`

**Status**: Draft. Independent of the agent cuts but shipped alongside
to keep the operational-contract surface coherent.

**Owner**: TBD. **Target version**: `LZI_LANG` minor bump.

## Motivation

Every Lazuli-generated web app needs CORS configured. Today there's
no source-of-truth for the allowlist — generated apps would carry
hand-edited middleware or external YAML, which:

- Drifts from the `urls per environment per target` block that
  `app.lzi` already declares.
- Is invisible to `lazuli inspect` (LLMs editing the capsule can't
  see CORS — they'd add an `expose http` endpoint without knowing
  which origins must reach it).
- Bypasses doctor cross-checks (no way to catch
  `allow_credentials true` + wildcard `*`, or origins that don't
  match any declared `url`).

CORS is **language-light**: it shapes how the runtime configures
HTTP transport, but it's checkable, declarative, observable, and
shares the `urls`-block pattern that already exists.

## Scope

- New `cors` child of `app.lzi` with:
  - `allow_origins <environment> "<origin>"[, "<origin>"]+` — one or
    more lines per environment; merged.
  - `allow_credentials true | false` — optional, defaults `false`.
  - `max_age "<duration>"` — optional, defaults `"1h"` at adapter level.
- IR: `AppManifest.cors: Option<AppCors>`.
- Doctor: 3 diagnostics (unknown environment, credentials/wildcard
  conflict, origin not documented in `urls`).
- LSP: 1 file-local (shape check on the `cors` block children).
- Inspect: `cors` surfaces as part of the always-on app projection
  (no `--expand` flag needed; the block is small).

`allow_methods` is intentionally **not** declared. The closed
runtime catalog applies (`GET POST PUT PATCH DELETE`); per-endpoint
methods come from `expose http method` (Cut A.7) and `api method`.
Adapters serve those exact methods on the matching path.

## Syntax

```lazuli
app MyApp
  environments
    local
    production

  urls
    web production "https://app.example.com"
    api production "https://api.example.com"

  cors
    allow_origins production "https://app.example.com", "https://*.example.com"
    allow_origins local "http://localhost:3000"
    allow_credentials true
    max_age "1h"
```

## Rules (normative)

- **Environment match**: every `allow_origins <env>` must reference
  an environment declared in `environments`. Doctor:
  `cors_unknown_environment_diagnostics` (error).
- **Wildcard + credentials**: per CORS spec, `allow_origins ... "*"`
  is incompatible with `allow_credentials true`. Doctor:
  `cors_credentials_wildcard_conflict_diagnostics` (error).
- **Origin documented**: every non-wildcard origin should appear as
  some `url <target> <env> "<origin>..."` in the same environment.
  Doctor: `cors_origin_undocumented_diagnostics` (warning — the
  origin may legitimately be a separate consumer not represented in
  `urls`, but the common case is "I forgot to update urls").
- **Origin shape**: each origin is a quoted string. Either a fully
  qualified URL (`https://app.example.com`), a wildcard subdomain
  (`https://*.example.com`), or the wildcard `*`. LSP:
  `cors_contract_diagnostics` (error) on malformed shapes.
- **Credentials default**: `false` (per CORS spec safe default).
- **Methods**: implicit closed catalog. The runtime allows whatever
  `expose http`/`api` declare on the matching path.

## Diagnostics

| Id | Severity | Pipeline | Source |
|---|---|---|---|
| `cors_unknown_environment_diagnostics` | error | doctor | A11 |
| `cors_credentials_wildcard_conflict_diagnostics` | error | doctor | A11 |
| `cors_origin_undocumented_diagnostics` | warning | doctor | A11 |
| `cors_contract_diagnostics` | error | LSP | A11 |

## IR delta

```rust
pub struct AppManifest {
    // ...existing fields...
    pub cors: Option<AppCors>,
}

pub struct AppCors {
    pub allow_origins: Vec<AppCorsOriginRule>,
    pub allow_credentials: bool,
    pub max_age: Option<String>,
    pub span_ref: Option<SpanRef>,
}

pub struct AppCorsOriginRule {
    pub environment: String,
    pub origins: Vec<String>,
}
```

`LZIR_SCHEMA`: minor bump (additive).

## Inspect

`app.cors` surfaces in the always-on app projection (same tier as
`urls`). No new `--expand` flag — the block is small enough that
hiding it costs more than emitting it.

## Layer placement

**Language-light**. Lazuli owns:
- The shape of the declaration.
- IR registration.
- Doctor cross-checks against `environments` and `urls`.
- Inspect surfacing.

Runtime owns:
- Materialising CORS middleware (Go gin/chi config, Express headers,
  etc.) from the IR.
- Default `allow_methods` from the runtime catalog.

Adapters own:
- Provider-specific CORS quirks (e.g., AWS API Gateway has its own
  CORS knobs that map onto the same declared origins).

## Non-goals

- **Per-endpoint CORS overrides** (`expose http cors origins ...`).
  Defer until pilot evidence shows the global allowlist is
  insufficient.
- **`allow_methods` customisation**. The runtime catalog is the
  source of truth; declaring methods at CORS-layer creates a
  contradiction surface.
- **`allow_headers` customisation**. Adapter concern; runtime
  derives from `expose http` / `api` declared inputs.
- **`expose_headers` for response headers**. Adapter concern.
- **Profile-level overrides**. Profiles already override `urls`;
  CORS naturally follows. If a pilot needs profile-specific CORS,
  add it then.

## Acceptance criteria

- `cors` block parses + lowers to IR.
- All four diagnostics implemented + tested.
- `examples/full-capsule/app.lzi` exercises the surface.
- `cargo run -q -p lazuli_cli -- check`, `doctor`, `inspect` all
  pass with the new shape visible.
- `docs/invariants.md` documents the `cors` block as an `app.lzi`
  invariant.
- `docs/quickref.md` adds a `## CORS` subsection (~15 lines).
- `docs/design-decisions.md` records: *CORS lives in `app.lzi` as a
  language-light declaration alongside `urls`. The boundary test —
  "does it change static analysis, policy reachability, tenancy, or
  generated API shape?" — answers "yes, generated API shape" because
  the runtime materialises CORS middleware from this declaration.
  Per-endpoint overrides defer until pilot evidence; the 80% case is
  a global allowlist matching declared URLs.*

## Changelog

- Initial draft. Surfaced from the post-Cut A architectural
  discussion: CORS is ground-floor (every web product needs it),
  shares the `urls`-block shape, and observability — letting LLMs
  editing the capsule see the allowlist when adding new endpoints —
  is the load-bearing reason for declaring in source vs runtime
  config.
