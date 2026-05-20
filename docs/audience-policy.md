# Audience Policy Lists

## TL;DR

Use `policy @policy.<name>` when an audience has one guard.
Use `policy [...]` when the same audience is visible through several guards.
Audience policy lists are OR-only; AND belongs in feature-level `policies`.

## Single form

```lzx
audience host
  policy @policy.role.host
  views [shell, detail]
```

`policy @policy.role.host` means the caller's actor must satisfy that one
policy before the audience surface is admitted.

Use the single form when the surface is role-specific. If only `host` should
see it, keep the guard as one visible atom instead of wrapping it in a list.

## List form

```lzx
audience signed_in
  policy [@policy.role.host, @policy.role.traveler]
  views [shell, detail]
```

The list form has OR semantics: a caller is admitted if their actor satisfies
any listed policy. Either `@policy.role.host` or `@policy.role.traveler` is
enough to enter the `signed_in` audience.

Each listed policy resolves like the single form: in the surface owner's
feature scope, unless the reference is feature-qualified.

## When to use list vs single

- Single: the surface is role-specific, for example only `host` sees it.
- List: the surface is shared between roles with an identical view set.

If roles diverge in screens, actions, redirects, or hosted backend operations,
split the audience blocks. A list says "same surface, several valid callers",
not product-specific branching.

## Why not AND?

Audience policy lists are OR-only. The audience layer decides which actor
classes may enter a surface; it is not a policy-composition language.

Multi-policy AND belongs in the feature-level `policies` dictionary:

```lazuli
policies
  verified_host: @role.host, @scope.profile_verified
```

This keeps the composite rule named, reusable, and inspectable. The audience
still points at one policy category: `policy @policy.verified_host`.

## Diagnostic

| Code | Level | Description |
|---|---|---|
| `AUDIENCE-POLICY-001` | info | A single-item audience policy list should use the single form for readability. |

`policy [@policy.role.host]` and `policy @policy.role.host` mean the same
thing. Prefer the single form when there is only one accepted policy.

## Related

- [Feature policies](canonical-semantics.md#policies)
- [Auth guide](auth-guide.md)
