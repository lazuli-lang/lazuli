---
title:   "Errors and i18n: the error contract + the message-key resolver"
slug:    errors-and-i18n
sector:  lazuli-way
tier:    approved
created: 2026-05-30
updated: 2026-05-30
tags: [doctrine, errors, i18n, translation, exposure, gaffe]
read_when: "errors block, error codes, translations / i18n"
---

# Errors and i18n: the error contract + the message-key resolver

Lazuli has exactly one error contract and one i18n surface; the doctor rejects everything else. The `errors` block decides **which envelope fields reach the wire** per HTTP status class. i18n turns every user-facing string into a `@translation.<key>` token, never a hard-coded literal.

The whole surface is three sibling feature blocks in canonical order (`policies → errors → translation`), plus one `translation` catalog file per locale on disk.

## The `errors` block: exposure, not text

Governs the **wire envelope** only — no messages. Decides whether the resolved message and other fields leak to the client, split by status class.

```lazuli
  errors
    default hide
    expose client 4xx message, code
    expose client 5xx code

    policy_denied message @translation.article_signin_required
    validation_failed message @translation.article_invalid_input
```

- `default hide` — canonical floor; resolved `message` does **not** reach the wire unless a status class opts it back in. (`default expose` exists; prefer `hide`.)
- `expose client 4xx <fields>` — fields reaching the client on 4xx. Closed catalog: `message`, `code`, `data`, `message_key`.
- `expose client 5xx <fields>` — closed catalog: `code`, `data`. **`message` is rejected** (`ERR-VOCAB-EXPOSE-5XX-MESSAGE`): 5xx is framework-internal, text can carry stack traces. Server log still keeps the full message.
- `message_key` exposes the resolved `@translation.<key>` token itself, so a client shipping its own offline catalog (native mobile) localizes without trusting server-rendered text.

## Closed catalog of framework error codes

`<code> message @translation.<key>` rows may only name codes the runtime emits itself. Closed — invent one → `ERR-VOCAB-CODE-UNKNOWN`. Codes the parser accepts:

| Code | Status | Fires when |
|---|---|---|
| `policy_denied` | 401/403 | no policy branch matches the active actor |
| `validation_failed` | 400 | a payload fails its validator / shape contract |
| `tenant_mismatch` | 400 | actor's tenant ≠ resource's tenant axis |
| `not_found` | 404 | a referenced row is absent |
| `rate_limited` | 429 | a rate-limit throttle rejects the request |
| `bad_request` | 400 | malformed body/headers/path, unknown input field |
| `method_not_allowed` | 405 | transport doesn't match the operation kind |
| `integration_error` | 502 | an adapter call to an external integration fails |
| `unique_violation` / `foreign_key_violation` / `not_null_violation` / `check_violation` | 4xx/5xx | a database constraint rejects the write |

> Hand docs say "eight closed codes"; the **live parser** also accepts the four DB-constraint codes (from `runtime/go/lazuli/error.go`). Parser wins — see [the-compiler-is-the-oracle](0006-the-compiler-is-the-oracle.md). `lazuli doctor .` prints the current catalog verbatim in `ERR-VOCAB-CODE-UNKNOWN`.

These rows *override the built-in message*; they don't declare new error families (see Named typed errors below).

## The `when_denied` resolver chain

A "you can't do that" message is never an inline English string — it's a `@translation.<key>` attached at the most specific layer that owns the phrasing. Renderer walks most-specific → most-generic, stops at first hit:

1. **Command-level** — `policy @policy.<cat>` with a `when_denied @translation.<key>` child. One command's exact phrasing.
2. **Per-policy** — a `policies.<category>` entry with `when_denied`. Inherited by every command using that category unless overridden.
3. **Per-feature** — the `errors` block's `<code> message @translation.<key>` rows: catch-all for a code with no closer override.
4. **Built-in catalog** — runtime ships a PT-BR + en-US floor for every code; zero-authoring apps still emit a human string.

Layers 1–2 use `when_denied`; layer 3 uses `<code> message`. All four resolve keys through the single `app.locale.fallbacks` graph — no second fallback chain. `when_denied` at policy and command layers:

```lazuli
  policies
    author: @role.admin, @role.editor
    edit: @role.admin, @role.editor
      when_denied @translation.article_edit_staff_only
    view: @scope.same_org

  command edit_article
    route id: ID
    input
      body: Text required
    policy @policy.edit
      when_denied @translation.article_edit_staff_only
    rate_limit "30 per minute per user"
    updates Article
      body = input.body
```

Policy categories are `author` / `edit` / `view` / `remove`. Do **not** use `create` / `read` / `update` / `delete` — they shadow effect verbs (`POLICY-CATEGORY-SHADOWS-EFFECT-001`). See [command-and-query-anatomy](0007-command-and-query-anatomy.md).

## The `translation` block + catalog file convention

Declares one `catalog` pointer plus per-key strings. Every `@translation.<key>` referenced in the feature must resolve to a `key` here (or in the on-disk catalog):

```lazuli
  translation
    catalog "./i18n/article.<locale>.json"

    key article_edit_staff_only
      pt-BR "Apenas administradores ou editores podem editar este artigo."
      en-US "Only admins or editors can edit this article."

    key article_signin_required
      pt-BR "Para gerenciar artigos, entre na sua conta primeiro."
      en-US "Please sign in to manage articles."
```

The `catalog` path's `<locale>` is a literal placeholder the compiler expands per locale, loading the matching file. Convention: `i18n/<name>.<locale>.json`, one file per locale, each a flat `{"<key>": "<string>"}` map:

```json
{
  "article_edit_staff_only": "Only admins or editors can edit this article.",
  "article_signin_required": "Please sign in to manage articles."
}
```

App-wide shared strings → `i18n/common.<locale>.json` at project root; feature-local strings → `features/<feature>/i18n/`. Inline `key` blocks and the on-disk catalog merge — inline is for keys authored alongside code; the JSON file is where translators work.

## `@translation.*` references everywhere a string surfaces

`@translation.<key>` is the only way a literal becomes user-facing. Beyond `when_denied` and `<code> message`, it surfaces on `rule` messages and enum labels/hints:

```lazuli
    rule "closed tickets cannot be reopened"
      deny Ticket.reopen when self.closed_at != nil
      error TicketAlreadyClosed status 409 expose message, code
      message "Cannot reopen a closed ticket"
```

This also shows the **named typed error** form — `error <Name> status <http-status> expose <fields>` declares a *new* error family with its own code and exposure, orthogonal to the framework-code overrides in `errors`. It's the escape hatch when no closed framework code fits the business rule. A bare-string `message` on a rule is allowed; prefer `@translation.<key>` when the string is user-facing.

## The gaffes this doc prevents

- `expose client 5xx message` → `ERR-VOCAB-EXPOSE-5XX-MESSAGE`. 5xx text never hits the wire.
- A `<code>` not in the closed catalog → `ERR-VOCAB-CODE-UNKNOWN`.
- An inline English string where a user sees it — use `@translation.<key>`.
- `when_denied` on a category nothing references, or pointing at a missing key → `ERR-VOCAB-WHEN-DENIED-*`. Wire the key into `translation`.
- Policy categories named `create`/`read`/`update`/`delete` → `POLICY-CATEGORY-SHADOWS-EFFECT-001`. Use `author`/`view`/`edit`/`remove`.

Unsure whether a code, field, or exposure spelling is legal? Don't guess — `lazuli doctor .` prints the exact closed catalog. Blessed reference: `examples/full-capsule/full-capsule.lzi` (`customer` feature's `errors` + `translation` blocks) with `examples/full-capsule/i18n/customer.<locale>.json`.

Authoritative spec: `docs/error-contract.md`, `docs/canonical-semantics.md` ("Feature-level error contracts" + "Resolver chain"), `docs/quickref.md`, `docs/keyword-reference.md` (`Errors` + `Translation` registry sections).
