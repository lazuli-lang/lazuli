---
title:   "Resource and field anatomy"
slug:    resources-and-fields
sector:  lazuli-way
tier:    approved
created: 2026-05-30
updated: 2026-05-30
tags: [doctrine, resource, field, scalar, pii, tenancy]
read_when: "declaring a resource or field — types, @cap/@pii, tenancy, relations"
---

# Resource and field anatomy

`resource` = persisted table. `record` = non-persisted result shape (DTO): no tenancy, policy, or CRUD. The field slot order is rigid so every field reads the same and stays diffable/machine-editable.

## The canonical field line

One field, one line, fixed slot order:

```txt
<name>: <Type> [markers...] [required | optional | = <default>] [relation modifiers...]
```

Type, then markers (`@pii.*`, `@key.*`), then **exactly one** of presence (`required`/`optional`) or default (`= value`) — never both — then relation modifiers (`on_delete restrict`).

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

## Scalar catalog is CLOSED — spelled bare

`<Type>` resolves to: a closed built-in scalar, a closed semantic scalar, a capability type, or a local `enum`/`resource`/`record`. A bare PascalCase name the catalog doesn't know is an error.

- **Plain scalars** (no framework behavior): `ID`, `Text`, `Boolean`, `Integer`, `Decimal`, `Date`, `DateTime`, `JSON`.
- **Semantic scalars** (validation + formatting): `Email`, `Phone`, `Url`, `Uuid`, `Currency`, `GeoPoint`, `HexColor`, `Percentage`, `Money`.

Both **bare** — `email: Email`, not `@semantic.Email`. `@semantic.X` is a deprecated alias for core types (`lazuli fmt` rewrites to bare), live only for *open, plugin-declared* scalars (`@semantic.BrazilianCPF`). Aliases `Id`/`String`/`Bool`/`Int`/`Float`/`Json` parse but are non-canonical — `VOCAB-SCALAR-ALIAS-001` flags them; write `ID`/`Text`/`Boolean`/`Integer`/`Decimal`/`JSON`.

## Capability types carry runtime behavior

`@cap.*` types (also bare) change storage/handling — upload storage, hashing, encryption, token expiry. They take a closed mini-grammar of arguments, not free strings:

```lazuli
  domain
    resource Vault
      password_hash: Hashed(algorithm:argon2id) optional
      api_key: Encrypted(key:@key.tenant) optional
      reset_token: Token(ttl:1h,single_use:true,store:hashed) optional
      spec: File(max_size:25mb,accept:text/csv) optional
```

`@key.*` declares encryption blast radius: `@key.app`, `@key.tenant`, `@key.user`, `@key.record`. Crypto tiers are explicit: `Hashed` one-way, `Encrypted` server-readable, `Token` generated single-use.

## `@pii.*` + sensitive `@cap.*` demand field-level read/write policy

Bites cold authors: any field marked `@pii.*` or one of `@cap.Encrypted` / `@cap.Hashed` / `@cap.Token` / `@cap.E2ee` **must** declare field-level `read` and `write` under `policies fields <Resource>`. Skip it → `lazuli check --security-profile strict` errors (`field-security-policy`); `prototype` warns (still clear it).

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

`read`/`write` here are access *directions* (a separate closed catalog) — fine. Category names (`author`/`view`) still obey [the-three-operators](0003-the-three-operators.md): never `create`/`read`/`update`/`delete`.

## Relations: `has_many`, `many_through`, refs, `on_delete`

A field typed as another resource is a foreign key. One-to-many: `has_many ... inverse`. Many-to-many carrying its own payload (assigned-by, timestamps, role): `many_through ... to` with ≥1 payload field (a payload-free join is just `has_many`).

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

`on_delete` governs *hard* delete, defaults to `restrict`; use `cascade`/`nullify` only when part of the product contract. Cross-feature FKs annotate the target: `target @feature.<f>.<Resource>` (`<f>` must be in `uses`). Polymorphic refs: `polymorphic_ref <type_field> <id_field> targets [A, B]`.

## Computed, derived, uniqueness

Fields can be computed instead of stored: `derived from <expr>`, `computed_date from <field> offset <n>`, `schedule_rule from @fn.X(<arg>) offset <n>`. Per-resource uniqueness: parenthesized list or partial-unique `when`. Tenant-scoped uniqueness lives in a `domain`-level `constraints` block — a *sibling* of `resource`, not a child:

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

## `defaults` factors out shared traits — don't restate

Feature-level `defaults` injects traits into every resource (write once). Children are a **narrow closed set**: `tenancy`, `timestamps`, `policy_for <kinds>: <atom>`. (`no_timestamps` / `soft_delete` / `retention` are *not* valid `defaults` children — parser rejects them.)

```lazuli
  defaults
    tenancy org
    timestamps
    policy_for jobs, webhooks: @actor.system
```

`tenancy org` injects a required `org: Org` field + default query scope `org == ctx.org`. `soft_delete` (per resource) injects `deleted_at == nil`. **Don't restate inherited scope** in queries — the analyzer flags redundant `org == ctx.org` / `deleted_at == nil`.

## Per-resource overrides and opt-outs

A resource overrides a feature default by declaring its own trait. Valid *resource* children: `tenancy`, `timestamps`, `soft_delete`, `append_only`, `retention`, `previously`, `validates`, `has_many`, `lifecycle`, plus fields and `unique`/`index`/`fts`. Break inherited tenancy on a genuinely global resource with `tenancy none`. `append_only` makes an insert-only ledger (`RESOURCE-APPEND-ONLY-001` rejects update/delete commands against it). Resources storing `@pii.*` should declare (or inherit) `retention <duration> then delete|anonymize|archive`:

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

See [justified-opt-outs](0005-justified-opt-outs.md) for when `tenancy none` and friends are legitimate vs. a smell.

## Validators and the line you shouldn't write

Single-use, resource-bound validation attaches inline with `validates`; reusable validators live under `extensions` as `validator <name>: Validator[...]`, referenced by `@validator.<name>`. Field types are checked and queries read records back; `lazuli inspect <feature> --expand=resources` shows the fully-expanded resource (injected tenancy field, timestamps, derived columns) — let [the-compiler-is-the-oracle](0006-the-compiler-is-the-oracle.md) prove the shape instead of duplicating it.

Authoritative spec: `docs/canonical-semantics.md` (Resources And Relations), `docs/grammar.lzi.md` §6, `docs/closed-catalogs.md`.
