# note — feature context

The `note` feature is the seed example shipped by `lazuli new`. It exists to
*teach the Lazuli Way by example*: a single resource that uses
`conventions [crud]` instead of hand-rolled create/update/delete commands, a
`defaults` block that hoists shared settings, and a typed `query.list` instead
of a `@fn` read. New apps copy this shape; see `docs/lazuli_way.md` for the full
idiom canon and `docs/lazuli_way/crud-by-convention.md` for the rule that backs
the CRUD convention.
