---
title:   "Errors and i18n: the error contract + the message-key resolver"
slug:    errors-and-i18n
sector:  lazuli-way
tier:    approved
created: 2026-05-30
updated: 2026-05-30
tags: [doctrine, errors, i18n, translation, exposure, gaffe]
---

# Errors and i18n: the error contract + the message-key resolver

When an agent invents error handling, it reaches for ad-hoc shapes: a `messages`
map here, a `status_code:` there, an English string inlined into a `deny`. Lazuli
has exactly one error contract and one i18n surface, and both are small. The
error contract decides **which envelope fields reach the wire** and **which
human string renders for which framework code**; i18n is how every user-facing
string becomes a `@translation.<key>` token instead of a hard-coded literal.
Pin these shapes — the doctor rejects everything else.

The whole surface lives in three sibling feature blocks, in canonical order
(`policies → errors → translation`), plus a `translation` catalog file per
locale on disk.

## The `errors` block: exposure, not text

The `errors` block governs the **wire envelope**. It does not contain messages —
it decides whether the resolved message and which other envelope fields leak to
the client, split by HTTP status class.

```lazuli
  errors
    default hide
    expose client 4xx message, code
    expose client 5xx code

    policy_denied message @translation.article_signin_required
    validation_failed message @translation.article_invalid_input
```

- `default hide` is the canonical floor — the resolved `message` does **not**
  reach the wire unless a status class opts it back in. (`default expose` is the
  other choice; prefer `hide`.)
- `expose client 4xx <fields>` lists the envelope fields that reach the client on
  4xx. Closed catalog: `message`, `code`, `data`, `message_key`.
- `expose client 5xx <fields>` does the same for 5xx. Closed catalog: `code`,
  `data` — **`message` is rejected here**. 5xx errors are framework-internal;
  their text can carry stack traces. Writing `expose client 5xx message` is a
  hard doctor error (`ERR-VOCAB-EXPOSE-5XX-MESSAGE`). The server log still keeps
  the full message for operators.

`message_key` is worth knowing: it exposes the resolved `@translation.<key>`
token itself so a client that ships its own offline catalog (a native mobile app)
can localize without trusting the server's rendered text.

## The closed catalog of framework error codes

The `<code> message @translation.<key>` rows inside `errors` may only name codes
the runtime emits on its own. The catalog is **closed** — invent one and you get
`ERR-VOCAB-CODE-UNKNOWN`. The codes the parser accepts today:

| Code | Status | Fires when |
|---|---|---|
| `policy_denied` | 401/403 | no policy branch matches the active actor |
| `validation_failed` | 400 | a payload fails its validator / shape contract |
| `tenant_mismatch` | 400 | the actor's tenant ≠ the resource's tenant axis |
| `not_found` | 404 | a referenced row is absent |
| `rate_limited` | 429 | a rate-limit throttle rejects the request |
| `bad_request` | 400 | malformed body/headers/path, unknown input field |
| `method_not_allowed` | 405 | transport doesn't match the operation kind |
| `integration_error` | 502 | an adapter call to an external integration fails |
| `unique_violation` / `foreign_key_violation` / `not_null_violation` / `check_violation` | 4xx/5xx | a database constraint rejects the write |

> The hand-written docs still call this "the eight closed codes"; the **live
> parser** also accepts the four DB-constraint codes above (surfaced from
> `runtime/go/lazuli/error.go`). When prose and parser disagree, the parser
> wins — see [the-compiler-is-the-oracle](0006-the-compiler-is-the-oracle.md).
> Run `lazuli doctor .` and read the `ERR-VOCAB-CODE-UNKNOWN` message; it prints
> the current catalog verbatim.

These framework-code rows are *overrides for the built-in message*, not new error
families. To declare a genuinely new error, see "Named typed errors" below.

## The `when_denied` resolver chain

A user-facing "you can't do that" message is never an inline English string. It
is a `@translation.<key>` attached at the most specific layer that should own the
phrasing. The renderer walks most-specific → most-generic and stops at the first
hit:

1. **Command-level** — `policy @policy.<cat>` carries a `when_denied
   @translation.<key>` child. One command's exact phrasing.
2. **Per-policy** — a `policies.<category>` entry carries `when_denied`. Every
   command using that category inherits it unless it overrides.
3. **Per-feature** — the `errors` block's `<code> message @translation.<key>`
   rows: the catch-all for any command emitting that code with no closer
   override.
4. **Built-in catalog** — the runtime ships a PT-BR + en-US floor for every
   code, so a zero-authoring app still emits a human string, not evaluator jargon.

Layers 1 and 2 use `when_denied`; layer 3 uses `<code> message`. All four
resolve keys through the single `app.locale.fallbacks` graph — there is no second
fallback chain. Here is `when_denied` at both the policy and command layers:

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

Note the policy categories: `author` / `edit` / `view` / `remove`. Do **not**
write `create` / `read` / `update` / `delete` — those shadow effect verbs and the
doctor rejects them (`POLICY-CATEGORY-SHADOWS-EFFECT-001`). See
[command-and-query-anatomy](0007-command-and-query-anatomy.md).

## The `translation` block + catalog file convention

The `translation` block declares one `catalog` pointer and the per-key strings.
Every `@translation.<key>` referenced anywhere in the feature must resolve to a
`key` here (or in the on-disk catalog):

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

The `catalog "./i18n/<name>.<locale>.json"` path uses a literal `<locale>`
placeholder. The compiler expands it per locale, loading the matching file. The
feature-local catalog convention is `i18n/<name>.<locale>.json`, one file per
locale, each a flat `{"<key>": "<string>"}` map:

```json
{
  "article_edit_staff_only": "Only admins or editors can edit this article.",
  "article_signin_required": "Please sign in to manage articles."
}
```

App-wide strings shared across features live in `i18n/common.<locale>.json` at the
project root; feature-local strings live beside the `.lzi` under
`features/<feature>/i18n/`. Inline `key` blocks and the on-disk catalog merge —
the inline form is convenient for keys authored alongside the code; the JSON file
is where translators work.

## `@translation.*` references everywhere a string surfaces

`@translation.<key>` is the only way a literal becomes user-facing. Beyond
`when_denied` and `<code> message`, it surfaces on `rule` messages and on enum
labels/hints:

```lazuli
    rule "closed tickets cannot be reopened"
      deny Ticket.reopen when self.closed_at != nil
      error TicketAlreadyClosed status 409 expose message, code
      message "Cannot reopen a closed ticket"
```

That `rule` also shows the **named typed error** form — `error <Name> status
<http-status> expose <fields>` declares a *new* error family with its own code and
its own exposure, orthogonal to the framework-code overrides in the `errors`
block. Named typed errors are the escape hatch when none of the closed framework
codes fit your business rule. (A bare-string `message` on a rule is acceptable;
prefer a `@translation.<key>` when the string is user-facing.)

## The gaffes this doc prevents

- `expose client 5xx message` → `ERR-VOCAB-EXPOSE-5XX-MESSAGE`. 5xx text never
  hits the wire.
- A `<code>` that isn't in the closed catalog → `ERR-VOCAB-CODE-UNKNOWN`.
- An inline English string where a user sees it — use `@translation.<key>`.
- `when_denied` on a policy category nothing references, or pointing at a missing
  key → `ERR-VOCAB-WHEN-DENIED-*`. Wire the key into the `translation` block.
- Policy categories named `create`/`read`/`update`/`delete` →
  `POLICY-CATEGORY-SHADOWS-EFFECT-001`. Use `author`/`view`/`edit`/`remove`.

When you are unsure whether a code, field, or exposure spelling is legal, do not
guess — `lazuli doctor .` prints the exact closed catalog in its diagnostic. The
blessed reference is `examples/full-capsule/full-capsule.lzi` (the `customer`
feature's `errors` + `translation` blocks) with its
`examples/full-capsule/i18n/customer.<locale>.json` catalogs.

Authoritative spec: `docs/error-contract.md`, `docs/canonical-semantics.md`
("Feature-level error contracts" + "Resolver chain"), `docs/quickref.md`,
`docs/keyword-reference.md` (`Errors` + `Translation` registry sections).
