# Next Checklist — Tracked Cuts from Graded Proposals

Items here are **graded out** of a proposal that landed PASS but carry follow-up cost. Each entry cites its origin proposal + dimension. Pull from this list during the next planning cycle.

---

## From external cruel review thread (2026-05-16)

These items were surfaced by the long external-review conversation that produced the catalog/host cruel re-review + idiomatic refactor pass. Items missed in earlier note-taking — anchoring here so the next planning cycle picks them up.

- [ ] **`VOCAB-TESTS-MISSING-001` doctor lint — capsule without `test` blocks.** User question 2026-05-16: "não ter testes escritos nos lzi pode ser considerado warning? como fazer isso sem ser falso positivo?" Even with great `test` syntax shipped, the LLM that authored the user's pilot didn't reach for it — because nothing prompted. Spec: `lazuli doctor` walks every feature and warns when (a) the feature has resources/commands AND (b) zero `test` blocks. Tighten with: warn only on feature-touched-in-last-N-commits (avoid false positives on legacy untouched buckets), opt-out via `# doctor:allow VOCAB-TESTS-MISSING-001 — reason "..."` per feature. Coverage-style mode (`lazuli doctor --coverage`) summarizes "X/Y commands have at least one test" across the capsule.

- [ ] **`docs/style-guide.md` — idiomatic Lazuli conventions.** External reviewer surfaced as the 5th language-gap item: convention not written down. Author once with: enum identifiers in English (labels via i18n catalog), shared value-types in dedicated features (Address in `account` or shared), `updates X` declarative form over `@fn` handler when the command is "just save input", lifecycle blocks for multi-step flows, semantic types over plain Text for PII (CPF/CNPJ/Email/Phone), `Type[]` over `JSON` for known-shape arrays. Without the guide, the audit-skill has no canon to enforce.

- [ ] **Caminho C — `examples/hostpoint-canonical/` idiomatic mirror.** Reviewer's question 2026-05-16: Hostpoint ships fast OR Lazuli demos strong? Caminho C bifurcates: Hostpoint stays at production velocity; `examples/hostpoint-canonical/` is the same product modeled with full lifecycle + rules + events + semantic types, used for Lazuli launch material. ~2-week investment; preserves both pilot-shipping speed AND tech-demo strength.

- [ ] **`VOCAB-HANDLER-HEAVY-001` doctor lint — feature with high handler ratio.** External reviewer observation: "feature está em modo CRUD with handlers, não em modo specification with invariants. Cada feature em modo `updates X` declarativo ou `@fn` handler? Razão sugere debt." Spec: warn when ≥70% of commands in a feature use `@fn` handlers (vs `creates`/`updates`/`deletes`/`returns` declarative effects), with diagnostic suggesting "convert to `updates X` if logic is just field assignments; keep @fn only for cross-resource transactions, OAuth, OTP, or other irreducibly imperative work." Hostpoint's pre-refactor `host.lzi` (5/5 handler-heavy) was the textbook trigger.

- [ ] **audit-skill MVP scope is narrower than original L0 plan.** Original L0: audit-skill depends on docs-as-IR-projection (cookbook + 3 pilots stabilized). Revised: ship MVP with subset of hardcoded rules (the doctor lints proposed above are the seed) validated against Hostpoint capsules BEFORE the full bundle lands. Record in proposal once authored so the difference between MVP-audit-skill and final-audit-skill is documented.

## From collections review (2026-05-16)

- [ ] **Exclusive-sentinel annotation for enum arrays.** `Traveler.pets` has a `none` variant that's mutually exclusive with all others — UI implements `togglePet` toggle logic manually. Spec: `field: PetType[] required exclusive_sentinel(none)` — analyzer rejects payloads that include `none` AND another variant.

## From cross-feature symbol resolution review (2026-05-16)

- [ ] **LSP/CLI surfaces symbol origin for cross-feature references.** When `host.lzi` references `Gender` and Lazuli resolves it via `uses account` to `account.Gender`, the IDE/CLI should expose that path without authors needing a `# Gender imported from account` comment. Spec: `lazuli inspect host.Gender` returns `defined in: account.lzi:line N, imported via: uses account at host.lzi:line 4`. Tree-sitter hover shows the same. Until this lands, authors are tempted to add procedence comments that violate `feedback_normative_not_narrative_2026-05-15` (specs are prescriptive; changelog/why goes to commit message or proposal). External cruel review 2026-05-16 surfaced this as a `Lazuli capitulating` smell.

## From `docs/proposals/semantic-types-money-brazilian.md` v0.3 (PASS 8.89/10, 2026-05-16)

- [ ] **A1 — Multi-Money-field codegen ambiguity.** When a resource has multiple Money fields (e.g. `Charge { amount, platform_fee, net_to_host }`), spec the codegen choice: one shared currency column per resource (most common: one transaction, one currency) vs one column per Money field. Decision before Hostpoint Charge resource migration. Recommended: one shared column named `currency`, with diagnostic on multi-currency resources that opt out via explicit `: Currency` field declaration.
- [ ] **A2 — Two-form default literal creates polysemy.** `"BRL:0.00"` (ISO-prefixed) and `"0.00"` (gated on remote `app.lzi locale / default_currency`) are both accepted. An LLM reading the field site in isolation cannot determine validity without reading `app.lzi`. Reconsider dropping the no-prefix form post-Hostpoint pilot. Verbosity cost is minor; cognitive cost is large.
- [ ] **A3 — Release-N window polysemy.** During release N (`VOCAB-MONEY-002` warn), `field: Money` still resolves to `Decimal`. After release N+1, resolves to `SemanticMoney`. LLMs trained on either snapshot will be wrong for the other. Tag releases explicitly + document in capsule changelog.
- [ ] **F1 — Multi-target codegen example missing for mobile (Expo).** Proposal claims multi-target but only Go + web shown. Either add a mobile codegen example to the proposal or state explicitly that mobile shares the same TS interface as web.
- [ ] **F2 — `@fn.*` per-row currency override is hand-wavy.** Add a concrete 5-line example: `command UpdateChargeCurrency { input: { id: ID, currency: Currency } via @fn.set_charge_currency }`.
- [ ] **F3 — `lazuli plan diff` should land before second pilot.** Hostpoint's first pilot of Money will produce ~6 hand-rolled ALTER TABLE migrations. The second pilot should not have to repeat that — graduate the diff-based migration runner.

---
