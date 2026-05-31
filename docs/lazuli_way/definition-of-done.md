# Definition of Done — the Lazuli feature gate

Every feature spec (0002–0017) embeds the block below **verbatim** in its
techspec Gate. Teaching and enforcement are release gates, not follow-ups: a
shipped feature that agents don't know how to use is wasted effort (proof: the
`conventions [crud]/[me]` feature shipped fully, yet Pauta hand-rolled 84 CRUD
commands because nothing taught it).

```
## Definition of Done (Lazuli feature gate)
1. BUILD: implemented; `cargo test -p <crate>` green for the new rule/grammar/codegen.
2. MIGRATE: every pilot that needed it is on it; `lazuli check && lazuli doctor && go build ./...` clean in hostpoint and/or pauta-web.
3. TEACH: docs/lazuli_way/<slug>.md filled (idiom → before/after pilot excerpt → enforcing doctor rule); scaffold CLAUDE.md.tmpl + AGENTS.md.tmpl bullet added.
4. ENFORCE: a doctor rule fires on the old hand-rolled shape OR the scaffold seed demonstrates the idiom. The rule code is named in the idiom doc.
A spec that skips gate 3 or 4 is NOT done. The RULE-team grader blocks it.
```
