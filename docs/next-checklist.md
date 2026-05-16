# Next Checklist — Tracked Cuts from Graded Proposals

Items here are **graded out** of a proposal that landed PASS but carry follow-up cost. Each entry cites its origin proposal + dimension. Pull from this list during the next planning cycle.

---

## From `docs/proposals/semantic-types-money-brazilian.md` v0.3 (PASS 8.89/10, 2026-05-16)

- [ ] **A1 — Multi-Money-field codegen ambiguity.** When a resource has multiple Money fields (e.g. `Charge { amount, platform_fee, net_to_host }`), spec the codegen choice: one shared currency column per resource (most common: one transaction, one currency) vs one column per Money field. Decision before Hostpoint Charge resource migration. Recommended: one shared column named `currency`, with diagnostic on multi-currency resources that opt out via explicit `: Currency` field declaration.
- [ ] **A2 — Two-form default literal creates polysemy.** `"BRL:0.00"` (ISO-prefixed) and `"0.00"` (gated on remote `app.lzi locale / default_currency`) are both accepted. An LLM reading the field site in isolation cannot determine validity without reading `app.lzi`. Reconsider dropping the no-prefix form post-Hostpoint pilot. Verbosity cost is minor; cognitive cost is large.
- [ ] **A3 — Release-N window polysemy.** During release N (`VOCAB-MONEY-002` warn), `field: Money` still resolves to `Decimal`. After release N+1, resolves to `SemanticMoney`. LLMs trained on either snapshot will be wrong for the other. Tag releases explicitly + document in capsule changelog.
- [ ] **F1 — Multi-target codegen example missing for mobile (Expo).** Proposal claims multi-target but only Go + web shown. Either add a mobile codegen example to the proposal or state explicitly that mobile shares the same TS interface as web.
- [ ] **F2 — `@fn.*` per-row currency override is hand-wavy.** Add a concrete 5-line example: `command UpdateChargeCurrency { input: { id: ID, currency: Currency } via @fn.set_charge_currency }`.
- [ ] **F3 — `lazuli plan diff` should land before second pilot.** Hostpoint's first pilot of Money will produce ~6 hand-rolled ALTER TABLE migrations. The second pilot should not have to repeat that — graduate the diff-based migration runner.

---
