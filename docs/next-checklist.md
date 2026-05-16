# Next Checklist — Tracked Cuts from Graded Proposals

Items here are **graded out** of a proposal that landed PASS but carry follow-up cost. Each entry cites its origin proposal + dimension. Pull from this list during the next planning cycle.

---

## From external cruel review thread (2026-05-16)

These items were surfaced by the long external-review conversation that produced the catalog/host cruel re-review + idiomatic refactor pass. Items missed in earlier note-taking — anchoring here so the next planning cycle picks them up.

- [x] **`VOCAB-TESTS-MISSING-001` doctor lint — capsule without `test` blocks.** SHIPPED in Wave 1 (`cf82472`). v0 fires on resources/commands + zero `test` blocks; opt-out parsing + git-touched filter deferred per `vocab_tests_missing_001.rs:10-17`. User question 2026-05-16: "não ter testes escritos nos lzi pode ser considerado warning? como fazer isso sem ser falso positivo?" Even with great `test` syntax shipped, the LLM that authored the user's pilot didn't reach for it — because nothing prompted. Spec: `lazuli doctor` walks every feature and warns when (a) the feature has resources/commands AND (b) zero `test` blocks. Tighten with: warn only on feature-touched-in-last-N-commits (avoid false positives on legacy untouched buckets), opt-out via `# doctor:allow VOCAB-TESTS-MISSING-001 — reason "..."` per feature. Coverage-style mode (`lazuli doctor --coverage`) summarizes "X/Y commands have at least one test" across the capsule.

- [x] **`docs/style-guide.md` — idiomatic Lazuli conventions.** SHIPPED in Wave 2 (`321c01e`) PASS 9.09/10 via `lazuli-language-architect`. Six idioms covered with Rule / Why / Idiomatic / Anti-idiom / Doctor-enforcement template. External reviewer surfaced as the 5th language-gap item: convention not written down. Author once with: enum identifiers in English (labels via i18n catalog), shared value-types in dedicated features (Address in `account` or shared), `updates X` declarative form over `@fn` handler when the command is "just save input", lifecycle blocks for multi-step flows, semantic types over plain Text for PII (CPF/CNPJ/Email/Phone), `Type[]` over `JSON` for known-shape arrays. Without the guide, the audit-skill has no canon to enforce.

- [ ] **Caminho C — `examples/hostpoint-canonical/` idiomatic mirror.** Reviewer's question 2026-05-16: Hostpoint ships fast OR Lazuli demos strong? Caminho C bifurcates: Hostpoint stays at production velocity; `examples/hostpoint-canonical/` is the same product modeled with full lifecycle + rules + events + semantic types, used for Lazuli launch material. ~2-week investment; preserves both pilot-shipping speed AND tech-demo strength.

- [x] **`VOCAB-HANDLER-HEAVY-001` doctor lint — feature with high handler ratio.** SHIPPED in Wave 1 (`e0c190b`). Fires on ≥3 commands + ≥70% `@fn`-handler ratio. IR-only detection (raw-text heuristic acknowledged false-negative class for skill-bundle consumers per `LIMITATIONS.md`). External reviewer observation: "feature está em modo CRUD with handlers, não em modo specification with invariants. Cada feature em modo `updates X` declarativo ou `@fn` handler? Razão sugere debt." Spec: warn when ≥70% of commands in a feature use `@fn` handlers (vs `creates`/`updates`/`deletes`/`returns` declarative effects), with diagnostic suggesting "convert to `updates X` if logic is just field assignments; keep @fn only for cross-resource transactions, OAuth, OTP, or other irreducibly imperative work." Hostpoint's pre-refactor `host.lzi` (5/5 handler-heavy) was the textbook trigger.

- [x] **audit-skill MVP scope is narrower than original L0 plan.** SHIPPED in Wave 3 — proposal `docs/proposals/audit-skill-mvp.md` v0.3 PASS 8.93 + bundle in `skills/audit/` (SKILL.md + INVOCATION.md + LIMITATIONS.md + RULES.md + 13 EXAMPLES/*.lzi + `examples_snapshot.rs` snapshot test). 13-rule catalog mirroring the doctor vocab. Original L0: audit-skill depends on docs-as-IR-projection (cookbook + 3 pilots stabilized). Revised: ship MVP with subset of hardcoded rules (the doctor lints proposed above are the seed) validated against Hostpoint capsules BEFORE the full bundle lands. Record in proposal once authored so the difference between MVP-audit-skill and final-audit-skill is documented.

## From collections review (2026-05-16)

- [ ] **Exclusive-sentinel annotation for enum arrays.** `Traveler.pets` has a `none` variant that's mutually exclusive with all others — UI implements `togglePet` toggle logic manually. Spec: `field: PetType[] required exclusive_sentinel(none)` — analyzer rejects payloads that include `none` AND another variant.

## From cross-feature symbol resolution review (2026-05-16)

- [x] **LSP/CLI surfaces symbol origin for cross-feature references.** SHIPPED in Wave 3 — proposal `docs/proposals/lsp-symbol-origin.md` v0.2 PASS 8.92 + `lazuli inspect <qualified-symbol>` CLI surface (`93144a1`) + analyzer walker `build_symbol_origin_index` + IR `SymbolOriginIndex` sidecar. LSP hover wiring (B.1+B.2) tracked separately below as a deferred item. When `host.lzi` references `Gender` and Lazuli resolves it via `uses account` to `account.Gender`, the IDE/CLI should expose that path without authors needing a `# Gender imported from account` comment. Spec: `lazuli inspect host.Gender` returns `defined in: account.lzi:line N, imported via: uses account at host.lzi:line 4`. Tree-sitter hover shows the same. Until this lands, authors are tempted to add procedence comments that violate `feedback_normative_not_narrative_2026-05-15` (specs are prescriptive; changelog/why goes to commit message or proposal). External cruel review 2026-05-16 surfaced this as a `Lazuli capitulating` smell.

## From `docs/proposals/semantic-types-money-brazilian.md` v0.3 (PASS 8.89/10, 2026-05-16)

- [ ] **A1 — Multi-Money-field codegen ambiguity.** When a resource has multiple Money fields (e.g. `Charge { amount, platform_fee, net_to_host }`), spec the codegen choice: one shared currency column per resource (most common: one transaction, one currency) vs one column per Money field. Decision before Hostpoint Charge resource migration. Recommended: one shared column named `currency`, with diagnostic on multi-currency resources that opt out via explicit `: Currency` field declaration.
- [ ] **A2 — Two-form default literal creates polysemy.** `"BRL:0.00"` (ISO-prefixed) and `"0.00"` (gated on remote `app.lzi locale / default_currency`) are both accepted. An LLM reading the field site in isolation cannot determine validity without reading `app.lzi`. Reconsider dropping the no-prefix form post-Hostpoint pilot. Verbosity cost is minor; cognitive cost is large.
- [ ] **A3 — Release-N window polysemy.** During release N (`VOCAB-MONEY-002` warn), `field: Money` still resolves to `Decimal`. After release N+1, resolves to `SemanticMoney`. LLMs trained on either snapshot will be wrong for the other. Tag releases explicitly + document in capsule changelog.
- [ ] **F1 — Multi-target codegen example missing for mobile (Expo).** Proposal claims multi-target but only Go + web shown. Either add a mobile codegen example to the proposal or state explicitly that mobile shares the same TS interface as web.
- [ ] **F2 — `@fn.*` per-row currency override is hand-wavy.** Add a concrete 5-line example: `command UpdateChargeCurrency { input: { id: ID, currency: Currency } via @fn.set_charge_currency }`.
- [ ] **F3 — `lazuli plan diff` should land before second pilot.** Hostpoint's first pilot of Money will produce ~6 hand-rolled ALTER TABLE migrations. The second pilot should not have to repeat that — graduate the diff-based migration runner.

## Pre-existing test failure (surfaced 2026-05-16 during Wave 3a)

- [ ] **`lazuli_cli::doctor::tests::doctor_pipeline_invokes_folder_and_design_rules` FAILED on commit 321c01e (pre-Wave-3 baseline).** Test expects `feature-orphan-component` folder rule to fire; got `["LAZULI-VERSION-001", "app_urls_missing", "design-token-hex-leak"]` instead. Likely tied to recent handler-arch refactor (commits 32fd8be / 4f09b9c) that changed where handlers land. Not introduced by this wave — verified by `git checkout 321c01e -- . && cargo test ...` reproducing the failure. Track as a follow-up.

- [ ] **`lazuli_codegen_go::emitter::migration_ddl::tests::maps_builtin_types_and_requiredness` FAILED on commit 9037fa3 (pre-Wave-3c baseline).** Test asserts `sql.contains("cents BIGINT NOT NULL")` but the Money tier 1 commit (`13b98b3`) changed Money fields to `NUMERIC(20,4)`. Test fixture needs to be updated to expect the new SQL (or test renamed to assert the new Money mapping). Not introduced by Wave 3.

## From `docs/proposals/audit-skill-mvp.md` v0.3 (PASS 8.93/10, 2026-05-16)

- [ ] **Status-line procedence cleanup.** v0.3 Status field carries inline v0.1→v0.2 trail; per `feedback_normative_not_narrative_2026-05-15`, procedence belongs in a `## §11. Revision history` section. Cosmetic; affects Criterion 1 by ~0.2.
- [ ] **§5 rule table — Rust-internal vocabulary leak.** Row for `VOCAB-HANDLER-HEAVY-001` carries `Command.effect` / `external_calls` / `Expr::Path` Rust typenames inline. Move the disambiguation to the §5 Note paragraph (already addresses it) so the user-facing table reads in product vocabulary only.
- [ ] **§7 difference table — `EXAMPLES/*.lzi` migration spec.** When v2 projector lands, RULES.md is regenerated from IR; but the 13 `EXAMPLES/*.lzi` snippets are user-authored test fixtures, not IR-projectable. 2-line addendum to §7 clarifying "EXAMPLES/ stays as snapshot-test corpus when v2 ships; the projector regenerates RULES.md only" closes the orphan-at-migration risk.
- [ ] **SKILL.md comment-strip guard.** §4.4 acknowledges "command inside a comment" false-positive class but SKILL.md prose doesn't tell the LLM consumer to strip `#`-prefixed lines before applying the catalog walk. 1-line addition behavioralizes the mitigation.

## From `docs/proposals/cross-feature-contracts.md` v0.2 (PASS 8.94/10, 2026-05-16)

- [ ] **Suppression annotation for deliberate internal coupling.** §11 Q14 doesn't cover the case where authors intentionally couple two features under `microservices` (e.g., a service boundary that exists for deploy-isolation but not for code-isolation). A future cell adds a closed-form annotation (e.g., `internal coupling between <feature_a>, <feature_b>` declared once at app level) that suppresses `CROSS-FEATURE-CONTRACT-MISSING-001` for that pair. Track until Hostpoint or another pilot surfaces the need; defer the syntax design until evidence demands it.

- [x] **Cross-feature-contracts implementation wave.** SHIPPED in Wave 4 (commits d995bb6 + 9e8fce1 + b93cf8e + 63f360b + fed59fd + 692a461 + wire-up). A.1 (parser) + A.2 (IR types) + A.3 (analyzer walker + SymbolOriginIndex.contract_version extension) + B.1 (CONTRACT-MISSING-001) + B.2 (VERSION-DRIFT-001 scaffolded) + B.3 (WORKFLOW-SPAN-001 warning) + B.4 (mod.rs wire-up) + D.1 (full-capsule public-contract annotations on CustomerStatus/CustomerTier/Customer) + F.1+F.2+F.3 (grammar.lzi.md / invariants.md / error-contract.md). 14 cross_feature doctor tests + 11 analyzer walker tests + 8 IR snapshot tests + 6 parser tests all green; emit_v1 fixture parses cleanly.

- [ ] **D.2 — Cross-feature-contracts integration tests in lazuli_cli.** Per proposal §9 D.2: end-to-end tests covering (a) `monolith`/`modular_monolith` no-op behavior; (b) `microservices` mode catching N missing contracts; (c) version-drift detection (gated on consumer-pin syntax landing first); (d) workflow-span warning. Deferred — the unit tests inside each B.* rule cover the logic; D.2 adds the CLI harness layer.

- [ ] **Consumer-side version pin syntax.** Required for `CROSS-FEATURE-CONTRACT-VERSION-DRIFT-001` to actually fire (currently scaffolded v0 with no trigger). Candidate forms: `uses account version v<N>` or per-symbol `uses account.Gender@v1`. Design judgement; not a single-file Codex cell.

- [ ] **Event public_contract wiring.** Proposal §5.3 row 6 includes `public contract event.<name> as v<N>` but Wave 4 only wired enum/resource/record/command/query (5 of 7 targets). Add `public_contract` field to `ir::Event` + lower from AST + extend `symbol_origin` walker to populate `contract_version` for events.

- [ ] **Auth identity `public contract identity as v<N>` wiring.** Proposal §3.5 + §5.3 row 7 — policy `actor.*` field references currently bypass the contract gate. Wire `public_contract: Option<PublicContract>` on `ir::Auth` (or wherever identity surface lives), lower from AST, fire CROSS-FEATURE-CONTRACT-MISSING-001 on cross-feature identity references.

- [ ] **`@runtime/cross-feature-transport` runtime-side proposal.** §6.3 + §10 explicitly defer vendoring strategy, wire codec (Protobuf/JSON/Avro), transport (RPC/gRPC/REST), and schema registry to a future runtime proposal. When the first `microservices` pilot ships, draft this proposal — keep it strictly in the runtime layer per `docs/capability-layering.md` §Decision Test.

## Deferred from Wave 3 (lsp-symbol-origin impl, 2026-05-16)

- [ ] **LSP hover handler (B.1 + B.2) — defer.** Proposal §6.7 + §9 Cell B.1 specs hover enrichment in `crates/lazuli_lsp/src/lib.rs` (15K+ LOC file). Wave 3c shipped the CLI consumer surface (`lazuli inspect <qualified-symbol>`) and the analyzer walker + IR types; the LSP hook is the next consumer. Defer until a real LSP consumer (Pleiades web in an editor, Hostpoint authoring flow) actively needs hover-on-symbol — currently no live consumer requests it.
- [ ] **D.3 Hostpoint capsule validation — defer.** Proposal §8 D.3 + §6 Hostpoint validation procedure. Private capsule (per `project_public_vs_private_repo`); no committable artifact. Run locally before tagging each skill bundle version. Tracked as a manual gate, not a wave cell.
- [ ] **CLI `--format lazuli` for symbol-mode.** Wave 3c emits JSON regardless of `--format` in symbol-mode. A future cell adds a human-readable `Lazuli` rendering (one-line `Symbol — defined in features/<feat>/<feat>.lzi:N`).
- [ ] **Cross-feature `imported_via` resolution.** Current symbol-mode lookup returns `imported_via: null` even for qualified queries that traverse a `uses` clause. A follow-up walker reads `feature.uses` + `SymbolOriginIndex.imports` and populates the `imported_via` block per proposal §5.2.
- [ ] **`referenced_by` field for `.lzx → .lzi` lookup** per proposal §8.3.1. Walker scans `.lzx` surfaces (lazuli_ir `Surface.audiences[*].views[*].source/submit`) and emits an inverted reference list. Needs a separate analyzer pass; not bundled into the v1 walker.

## From `docs/proposals/lsp-symbol-origin.md` v0.2 (PASS 8.92/10, 2026-05-16)

- [ ] **`SymbolKind` naming collision pre-empt.** `ir::SymbolKind` will coexist with `tower_lsp::lsp_types::SymbolKind` (`crates/lazuli_lsp/src/lib.rs:13`). Cell A.3 should explicitly `use lazuli_ir::SymbolKind as IrSymbolKind` at LSP boundary; not fatal but worth pre-empting at the rename site. Add to A.3 cell description.
- [ ] **Bare-name disambiguation tradeoff (`lazuli inspect Gender`).** Path-wins rule is deterministic but `lazuli inspect host` (where `host` is both a feature name and a project subdirectory) silently switches to path-mode. The user must qualify (`host.Host`) to get symbol-mode. Document the tradeoff in §10.7 with an example. Cosmetic; affects Criterion 5 by ~0.1.
- [ ] **`SymbolKind::Scalar` populated post-L0 #4.** §6.2 lists `Scalar` as reserved. When L0 #4 ships scalar aliases (per `project_validation_strategy_2026-05-14`), populate the variant + add a JSON example shape to §5.2.1.

## From `docs/style-guide.md` (PASS 9.09/10, 2026-05-16)

- [ ] **Idiom 2 (`shared` feature) tie-breaker example.** Doc states "create `shared` only when ≥ 2 consumers exist AND no feature semantically owns the type without forcing a contortion." Add a concrete worked example showing both the right and wrong place to put `Address`.
- [ ] **`.lzx` idiom catalog.** Style guide title and out-of-scope kick `.lzx` to a follow-up doc. Author `docs/style-guide-lzx.md` (or extend with §7-§N) once `.lzx` authoring patterns stabilize across Pleiades / Hostpoint. Deferred until pattern frequency ≥ 3 across pilots.

---
