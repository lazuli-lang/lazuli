# Write targeting

## Reach for this

When a mutating command writes a row that is **not** identified by `route.id`
alone — an ownership-scoped write ("update *my* row"), a composite key, or a
non-`route` discriminator — scope the effect explicitly with
`where <column> = <expr>` inside the `updates`/`deletes` body. Do NOT lean on the
`route.id` by-id inference for these, and do NOT write the scoping column as a
SET (it silently mis-targets the row).

A `where <col> = <expr>` line becomes the WHERE binding, not a SET. Its RHS
lowers through the same closed binding sources as a SET assignment
(`ctx.actor.id` → ctx, `route.id`/`input.x` → input, a literal → const, plus
`let` and `@fn(...)`).

## Before (hand-rolled) / After (idiomatic)

**Before** — relying on by-id inference for an owner-scoped edit lets any caller
who knows the row id mutate it; or the author "scopes" by listing the owner
column as a SET, which writes that column instead of filtering on it:

```
# anti-pattern: owner_id written as a SET, route.id is the only filter
updates Post
  owner_id = ctx.actor.id   # <-- OVERWRITES the owner, does not scope!
  archived_at = ctx.now
```

**After** — both scoping predicates are explicit `where` bindings; the SET block
carries only the columns that actually change
([examples/comment.lzi](../../examples/comment.lzi) `command edit`):

```
updates Comment
  where id = route.id
  where author = ctx.actor.id
  body = input.body
  updated_at = ctx.now
```

`@scope.*` policy bindings still AND into the generated WHERE on top of any
authored `where`.

## Enforced by

- `CODEGEN-UNRESOLVED-BINDING-SOURCE-001`
  ([crates/lazuli_doctor/src/correctness/codegen_unresolved_binding_source_001.rs](../../crates/lazuli_doctor/src/correctness/codegen_unresolved_binding_source_001.rs))
  — a `creates`/`updates`/`deletes` binding RHS (SET **or** authored `where`)
  whose path resolves to none of `{input, ctx, target, route, @fn(), literal,
  let}` is a hard error, instead of the old silent `FromConst("<raw>")` garbage
  string. The legal binding sources are the agent-actionable part of the rule.
- The `where` clause grammar and lowering are covered by
  [crates/lazuli_syntax/src/parser/lzi/command/where_clause_tests.rs](../../crates/lazuli_syntax/src/parser/lzi/command/where_clause_tests.rs)
  — the canonical "update my own row" shape (`where id = ctx.actor.id`) routes
  the line to the WHERE binding with no phantom `"where id"` SET column.

## Related

- [canonical-semantics.md](../canonical-semantics.md) §"Scoping the write with `where`"
- [field-policy](field-policy.md) — the policy delimiter rules (comma `,` = OR;
  pipe `|` = OR; boolean `and`/`or`/`not` only inside `policy <expr>`).
