Lazuli intermediate representation.

Shape governance lives in `docs/ir-abi.md`. This crate exposes types only;
it has no public mutator. Producers live in `lazuli_analyzer` lowering
parsed `.lzi` and `.lzx` source.
All consumers (codegens, planner, LSP, MCP, CLI) read this data and never
write back. Re-authoring means rewriting source.

Phase 1a foundation: `Module` / `Feature` / `Resource` / `Field` (with
`TypeRef` enum), `EnumDecl`, `Command` (with `Effect`), `Query` (List /
Lookup / Sql), and a minimal `Predicate` AST. Workflows, rules, events,
surfaces, jobs, webhooks, auth, escape routes, and extension contracts are
reserved for later phases.
