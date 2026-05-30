---
title:   "Resource and field anatomy"
slug:    resources-and-fields
sector:  lazuli-way
tier:    approved
created: 2026-05-30
updated: 2026-05-30
tags: [doctrine, resource, field, scalar, pii, tenancy]
---

# Resource and field anatomy

Every feature that stores data touches this. A `resource` is a persisted table; a
`record` is a non-persisted result shape (DTO) with no tenancy, policy, or CRUD.
Fields are the most repeated line in the whole language, so the shape is rigid on
purpose — once you know the slot order, every field reads the same and an LLM can
copy it without guessing.

## The canonical field line

One field, one line, slots in a fixed order:

```txt
<name>: <Type> [markers...] [required | optional | = <default>] [relation modifiers...]
```

Type first, then markers (`@pii.*`, `@key.*`), then **exactly one** of presence
(`required` / `optional`) or a default (`= value`) — never both — then relation
modifiers like `on_delete restrict`. Keeping the order stable is what makes field
lines diffable and machine-editable.

```lazuli
feature widget
  purpose "Widget catalog: ownership, tiering, and sensitive credentials."

  defaults
    tenancy org
    timestamps

  uses org, account

  domain
    enum WidgetTier
      free = 10
      pro = 20

    resource Widget
      owner: User optional
      name: Text required
      email: Email @pii.contact required
      tier: WidgetTier = free
```

## The scalar catalog is CLOSED — and spelled bare

A `<Type>` resolves to one of: a closed built-in scalar, a closed semantic
scalar, a capability type, or a local `enum` / `resource` / `record`. Inventing a
bare PascalCase type name the catalog does not know is an error.

The **plain scalars** (no framework behavior): `ID`, `Text`, `Boolean`,
`Integer`, `Decimal`, `Date`, `DateTime`, `JSON`.

The **semantic scalars** (validation + formatting): `Email`, `Phone`, `Url`,
`Uuid`, `Currency`, `GeoPoint`, `HexColor`, `Percentage`, `Money`.

Both are spelled **bare** — `email: Email`, `revenue: Money`, not
`@semantic.Email`. The `@semantic.X` spelling survives as a *deprecated alias* for
core types (`lazuli fmt` rewrites it to bare) and stays live only for an
*open, plugin-declared* scalar (`@semantic.BrazilianCPF`). For the closed core,
write bare. Likewise the aliases `Id`/`String`/`Bool`/`Int`/`Float`/`Json` parse
but are non-canonical — `VOCAB-SCALAR-ALIAS-001` flags them; write
`ID`/`Text`/`Boolean`/`Integer`/`Decimal`/`JSON`.

## Capability types carry runtime behavior

`@cap.*` types (also spelled bare) change how the value is stored and handled —
upload storage, hashing, encryption, token expiry. They take a closed
mini-grammar of arguments, not free strings:

```lazuli
  domain
    resource Vault
      password_hash: Hashed(algorithm:argon2id) optional
      api_key: Encrypted(key:@key.tenant) optional
      reset_token: Token(ttl:1h,single_use:true,store:hashed) optional
      spec: File(max_size:25mb,accept:text/csv) optional
```

`@key.*` declares the encryption blast radius: `@key.app`, `@key.tenant`,
`@key.user`, `@key.record`. The crypto tiers are explicit — `Hashed` is one-way,
`Encrypted` is server-readable, `Token` is generated single-use material.

## `@pii.*` and `@cap.*` fields demand field-level read/write policy

This is the rule that bites cold authors. Any field marked `@pii.*` or one of
`@cap.Encrypted` / `@cap.Hashed` / `@cap.Token` / `@cap.E2ee` **must** declare a
field-level `read` and `write` policy under `policies fields <Resource>`. Skip it
and `lazuli check --security-profile strict` errors (`field-security-policy`);
under `prototype` it is a warning you should still clear.

```lazuli
  policies
    author: @role.admin
    view: @scope.same_org

    fields Widget
      email
        read: @scope.same_org
        write: @role.admin
      api_key
        read: @actor.system
        write: @actor.system
```

`read` / `write` here are access *directions* (a different closed catalog from
policy categories) — those are fine. The category names above
(`author`/`view`) still obey the
[the-three-operators](0003-the-three-operators.md) rule: never
`create`/`read`/`update`/`delete`.

## Relations: `has_many`, `many_through`, refs, and `on_delete`

A field whose type is another resource is a foreign key. A simple one-to-many is
`has_many ... inverse`; a many-to-many that carries its own payload (assigned-by,
timestamps, a role) is `many_through ... to`, with at least one payload field. A
payload-free join would just be a `has_many`.

```lazuli
  domain
    resource Product
      owner: User optional on_delete restrict
      name: Text required
      has_many notes: ProductNote inverse product
      many_through ProductManager to User
        role_in_product: Text required

    resource ProductNote
      product: Product required on_delete cascade
      body: Text required
```

`on_delete` governs *hard* delete and defaults to `restrict`. Reach for
`cascade` or `nullify` only when that behavior is part of the product contract.
Cross-feature FKs annotate the target with `target @feature.<f>.<Resource>` (the
feature must be in `uses`), and polymorphic refs use
`polymorphic_ref <type_field> <id_field> targets [A, B]`.

## Computed, derived, and uniqueness

Fields can be computed instead of stored: `derived from <expr>`,
`computed_date from <field> offset <n>`, and rule-driven
`schedule_rule from @fn.X(<arg>) offset <n>`. Per-resource uniqueness uses a
parenthesized list or a partial-unique `when`; tenant-scoped uniqueness lives in a
`domain`-level `constraints` block (a *sibling* of `resource`, not a child):

```lazuli
  domain
    resource Sku
      code: Text required
      name: Text required
      is_default: Boolean = false
      unique (code, name)
      unique is_default when is_default == true

    constraints
      unique code per org
```

## `defaults` factors out shared resource traits — don't restate them

Feature-level `defaults` injects traits into every resource so you write them
once. Its children are a **narrow closed set**: `tenancy`, `timestamps`, and
`policy_for <kinds>: <atom>`. (Despite older prose, `no_timestamps` /
`soft_delete` / `retention` are *not* valid `defaults` children — the parser
rejects them.)

```lazuli
  defaults
    tenancy org
    timestamps
    policy_for jobs, webhooks: @actor.system
```

`tenancy org` injects a required `org: Org` field plus the default query scope
`org == ctx.org`; `soft_delete` (declared per resource) injects
`deleted_at == nil`. **Do not restate inherited scope** in queries — the analyzer
flags redundant `org == ctx.org` / `deleted_at == nil` lines.

## Per-resource overrides and opt-outs

A resource overrides the feature default by declaring its own trait. The valid
*resource* children are `tenancy`, `timestamps`, `soft_delete`, `append_only`,
`retention`, `previously`, `validates`, `has_many`, `lifecycle`, plus fields and
`unique`/`index`/`fts`. To break the inherited tenancy on a resource that is
genuinely global, write `tenancy none`. `append_only` makes an insert-only ledger
(`RESOURCE-APPEND-ONLY-001` rejects update/delete commands against it), and
resources storing `@pii.*` should declare `retention <duration> then
delete|anonymize|archive` (or inherit it):

```lazuli
  domain
    resource AuditEntry
      append_only
      tenancy none
      actor: User required
      kind: Text required

    resource Session
      provider_token: Encrypted(key:@key.tenant) @pii.credential optional
      retention 30d then delete
```

See [justified-opt-outs](0005-justified-opt-outs.md) for when `tenancy none` and
friends are legitimate versus a smell.

## Validators and the line you should not write

Single-use, resource-bound validation attaches inline with `validates`; reusable
validators live under `extensions` as `validator <name>: Validator[...]` and are
referenced by `@validator.<name>`. Field types are checked, queries read records
back, and `lazuli inspect <feature> --expand=resources` shows the fully-expanded
resource (injected tenancy field, timestamps, derived columns) without you
restating any of it — let
[the-compiler-is-the-oracle](0006-the-compiler-is-the-oracle.md) prove the shape
instead of duplicating it.

Authoritative spec: `docs/canonical-semantics.md` (Resources And Relations),
`docs/grammar.lzi.md` §6, `docs/closed-catalogs.md`.
