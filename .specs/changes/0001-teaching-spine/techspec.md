---
id: 0001
title: Teaching Spine — lazuli_way idioms canon + scaffold + DoD gate
type: techspec
status: ready
created: 2026-05-31
depends_on: []
parallel_safe: true
test_gate: "lazuli check lazurite/templates/default/app && lazuli doctor lazurite/templates/default/app && cargo test -p lazuli_cli docs_lazuli_way"
agent: unassigned
---

# TechSpec — Teaching Spine

## Approach
Pure docs + templates + one seed feature. No Rust/grammar change. The only "code" is a tiny test asserting the lazuli_way index links resolve and the DoD doc exists, so the teaching surface can't silently rot. Everything else is Markdown + `.lzi`/`.lzx`. This spec's job is to FREEZE the seam: create every idiom-file stub and the DoD block so the other 16 specs drop content into pre-cut slots.

## Surface
**Create:**
- `docs/lazuli_way.md` — index: intro + table linking every idiom slug.
- `docs/lazuli_way/definition-of-done.md` — the 4-gate DoD (verbatim block below).
- `docs/lazuli_way/crud-by-convention.md` — FILLED now (idiom shipped today).
- `docs/lazuli_way/escape-hatch-decision-tree.md` — FILLED now (decision tree: typed effect → query.sql/compose → @fn; raw SQL must be declared, never buried in @fn Go).
- `docs/lazuli_way/feature-defaults.md` — STUB `<!-- filled by spec 0004 -->`
- `docs/lazuli_way/field-policy.md` — STUB `<!-- filled by spec 0005 -->`
- `docs/lazuli_way/one-feature-one-capability.md` — STUB `<!-- filled by spec 0008/0009 -->`
- `docs/lazuli_way/referential-guards.md` — STUB `<!-- filled by spec 0014 -->`
- `docs/lazuli_way/soft-delete.md` — STUB `<!-- filled by spec 0015 -->`
- `docs/lazuli_way/money.md` — STUB `<!-- filled by spec 0016 -->`
- `docs/lazuli_way/state-machines.md` — STUB `<!-- filled by spec 0017 -->`
- `docs/lazuli_way/comment-hygiene.md` — STUB `<!-- filled by spec 0007/0008 -->`
- `lazurite/templates/default/app/features/note/note.lzi` — seed feature using `conventions [crud]` + `defaults` block.
- `lazurite/templates/default/app/features/note/note.lzx` — seed surface.
- `crates/lazuli_cli/tests/docs_lazuli_way.rs` — test: index exists, every linked slug file exists, DoD doc exists.

**Modify:**
- `lazurite/templates/default/CLAUDE.md.tmpl` — add "## Authoring idioms" section (links lazuli_way + per-idiom one-liners); rewrite escape-hatch clause #1 (`@fn`) to close the SQL-in-Go loophole.
- `lazurite/templates/default/AGENTS.md.tmpl` — identical edits (the two files are mirrored verbatim).
- `docs/quickstart.md` — one line under a new "Authoring" note linking `docs/lazuli_way.md`.
- `docs/language-backlog.md` — move the stale `assignment`/`reacts to`/`crud` line out of "Missing/Pressure" into "Closed v0 Decisions" noting `conventions [crud]/[me]` shipped (cite `docs/proposals/ir-resource-conventions-crud.md`).
- `docs/README.md` — add `lazuli_way.md` to the doc-set table.

## Contracts
**The Definition-of-Done block (every feature spec 0002–0017 embeds this verbatim in its Gate):**
```
## Definition of Done (Lazuli feature gate)
1. BUILD: implemented; `cargo test -p <crate>` green for the new rule/grammar/codegen.
2. MIGRATE: every pilot that needed it is on it; `lazuli check && lazuli doctor && go build ./...` clean in hostpoint and/or pauta-web.
3. TEACH: docs/lazuli_way/<slug>.md filled (idiom → before/after pilot excerpt → enforcing doctor rule); scaffold CLAUDE.md.tmpl + AGENTS.md.tmpl bullet added.
4. ENFORCE: a doctor rule fires on the old hand-rolled shape OR the scaffold seed demonstrates the idiom. The rule code is named in the idiom doc.
A spec that skips gate 3 or 4 is NOT done. The RULE-team grader blocks it.
```

**Idiom-doc shape (fixed; downstream specs follow it):**
```
# <Idiom name>
## Reach for this
<one sentence>
## Before (hand-rolled)  /  After (idiomatic)
<real pilot excerpt, file:line>
## Enforced by
<DOCTOR-RULE-CODE> — <what it fires on>
```

**lazuli_way index link set (frozen — downstream specs do not add rows, only fill files):** crud-by-convention, escape-hatch-decision-tree, feature-defaults, field-policy, one-feature-one-capability, referential-guards, soft-delete, money, state-machines, comment-hygiene.

**Seed feature contract (`note`):** resource `Note { title: Text required, body: Text }` with `conventions [crud]`; a `defaults` block with `tenancy org` + (placeholder comment for `rate_limit`/`audit` until spec 0004 lands the keys); one `query.list`. Must be the minimal thing that shows the idiom and stays doctor-clean on today's grammar.

## Plan — for the executing agent
1. Create `docs/lazuli_way/` dir. Write the index `docs/lazuli_way.md` with the frozen link set (all 10 slugs), each row one line + status (filled/stub).
2. Write `docs/lazuli_way/definition-of-done.md` containing the DoD block above verbatim.
3. Write `docs/lazuli_way/crud-by-convention.md`: Reach-for; Before = pauta `customer_management.lzi` hand-rolled `create_customer`/`update_customer` (cite lines); After = `conventions [crud]`; Enforced by = `VOCAB-CRUD-SYNTH-AVAILABLE-001` (spec 0002, name it as "incoming"). Include the soft-delete caveat (delete may stay explicit until spec 0015).
4. Write `docs/lazuli_way/escape-hatch-decision-tree.md`: the ordered decision — (a) typed `command`/`query.list`/`query.lookup`? use it. (b) needs joins/aggregates/window? `query.sql` (declared, with `returns`+`policy`+`params`) or `query.compose`. (c) vendor call / genuine imperative? `@fn`. **Rule: raw SQL must live in a declared `query.sql`/`query.compose`, NEVER as a string literal inside a `@fn` Go handler** (cite hostpoint `trust/handlers/list_property_reviews.go` as the anti-example). Name `ESC-RAWSQL-IN-HANDLER-001` (spec 0010, incoming).
5. Write the 8 stub idiom files, each: `# <name>` + `<!-- filled by spec NNNN -->` + the fixed shape headers empty.
6. Create the seed `note` feature (`.lzi` + `.lzx`) per the contract. Run `lazuli check` + `lazuli doctor` on `lazurite/templates/default/app` until clean.
7. Edit `CLAUDE.md.tmpl`: add `## Authoring idioms` after "Workflow for adding a feature" — link `docs/lazuli_way.md`, list each idiom as "Reach for `<X>`, not hand-rolled `<Y>`." Rewrite escape-hatch #1 `@fn` clause: keep vendor-call/custom-validation; REMOVE language for anything expressible as a typed effect or query; add the explicit "raw SQL → `query.sql`/`query.compose`, never inside `@fn`" rule pointing at the decision tree.
8. Apply the identical edits to `AGENTS.md.tmpl` (verify the two files stay byte-identical in the edited regions).
9. Add the quickstart link + README row + backlog un-stale edit.
10. Write `crates/lazuli_cli/tests/docs_lazuli_way.rs`: parse `docs/lazuli_way.md`, assert each linked `docs/lazuli_way/*.md` exists and DoD doc exists; assert CLAUDE.md.tmpl and AGENTS.md.tmpl both contain "Authoring idioms".

## Tests first (TDD)
- [ ] `index_links_resolve` — every slug linked in `docs/lazuli_way.md` has a file on disk.
- [ ] `dod_doc_present` — `docs/lazuli_way/definition-of-done.md` exists and contains the 4 gate lines.
- [ ] `scaffold_teaches_idioms` — both templates contain `## Authoring idioms` and a `lazuli_way` link.
- [ ] `seed_feature_clean` — `lazuli check` + `lazuli doctor` exit 0 on the templated app (run in gate).
- [ ] `escape_hatch_clause_closed` — `CLAUDE.md.tmpl` no longer contains the loophole phrasing licensing arbitrary `@fn` logic; contains the "raw SQL → query.sql/compose" rule.

## Gate
`test_gate` green **and** a human glance confirms `lazuli_way.md` reads as a coherent table of contents and the seed `note` feature visibly uses `conventions [crud]`.

## Risks & rollback
- Seed feature fails doctor on current grammar (e.g. `defaults rate_limit` not yet a key) → mitigation: seed uses only TODAY's valid `defaults` keys (`tenancy`), with a comment placeholder for rate_limit/audit; spec 0004 upgrades the seed when those keys land.
- Editing CLAUDE.md/AGENTS.md drift apart → mitigation: the `scaffold_teaches_idioms` test + a manual diff of the two edited regions.

**Rollback:** `git revert` the commit — all changes are additive docs/templates/one example dir; nothing downstream depends on it at runtime.
