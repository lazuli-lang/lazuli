# Field policy

## Reach for this

A resource field carries a **field policy** when reading or writing it must
be gated by role/scope independently of the feature-level `policies`
categories — typically PII or finance fields (`@pii.*`, `@cap.Encrypted` /
`Hashed` / `Token`). Declare per-field `read`/`write` allow-lists under
`policies fields <Resource>`:

```text
policies
  fields Customer
    cnpj
      read: @role.ADMIN | @role.MANAGER
      write: @role.ADMIN
```

When the read and write allow-lists are **identical** (the common case),
reach for the `access:` symmetric shorthand instead of spelling the same
policy twice:

```text
policies
  fields Customer
    legal_name
      access: @role.ADMIN | @role.MANAGER
```

- **`access: P`** desugars to `read: P` + `write: P` at parse time. The
  resulting field-policy IR — and therefore the emitted Go — is
  **byte-identical** to the explicit two-line form. `access:` makes the
  *symmetry intent* explicit instead of leaving it implied by two matching
  lines.
- **One form per field.** A field uses `access:` (symmetric) **or**
  `read:`/`write:` (asymmetric), never both — mixing them on the same
  field is a parse error (no merging, no policy algebra).
- **Keep `read:`/`write:` for the asymmetric minority.** A field whose read
  and write genuinely differ (e.g. anyone-in-org may *read* a tax id but
  only an admin may *write* it) must stay explicit.

> **Scope:** `access:` is a Pauta-driven convenience. Hostpoint expresses
> field sensitivity with `@cap.PII` field markers, not `policies fields`
> blocks, so it has zero `access:` sites — and that is fine. See
> `.specs/changes/0005-access-field-shorthand/adr.md`.

## Before (hand-rolled) / After (idiomatic)

**Before** — Pauta spelled the same policy on both axes for every symmetric
field. In `customer_management` all 4 `Contact` fields and 5/6 `Customer`
fields were byte-identical read/write pairs; `supplier` had 6/8 fields the
same way (`app/features/customer_management/customer_management.lzi:184-216`):

```text
fields Customer
  legal_name
    read: @role.ADMIN | @role.MANAGER
    write: @role.ADMIN | @role.MANAGER
  cnpj
    read: @role.ADMIN | @role.MANAGER
    write: @role.ADMIN
```

**After** — the symmetric field collapses to one line; the asymmetric
`cnpj` (read ADMIN|MANAGER, write ADMIN-only) keeps its explicit pair:

```text
fields Customer
  legal_name
    access: @role.ADMIN | @role.MANAGER
  cnpj
    read: @role.ADMIN | @role.MANAGER
    write: @role.ADMIN
```

Across `customer_management` (9 pairs) and `supplier` (6 pairs), 15
symmetric pairs collapsed to one line each. `lazuli inspect . --expand=security`
and the generated Go are **byte-identical** pre/post migration — the
desugaring is the migration's safety net.

## Enforced by

`field-security-policy` — fires (under the Strict/Production profile) on a
sensitive field (`@pii.*` / `@cap.Encrypted` / `@cap.E2ee` / `@cap.Hashed` /
`@cap.Token`) that lacks **both** a `read` and a `write` policy under
`policies fields <Resource>`. The `access:` shorthand satisfies both axes
(it desugars to `read:` + `write:`), so migrating a symmetric field from the
explicit pair to `access:` keeps the field compliant — the rule stays
silent. A field that declares only one axis (e.g. `read:` with no `write:`)
still fires.

A dedicated "read == write, prefer `access:`" advisory hint is a tracked
follow-up (the migrated Pauta features already demonstrate the idiom); see
`.specs/changes/0005-access-field-shorthand/techspec.md` §ENFORCE.
