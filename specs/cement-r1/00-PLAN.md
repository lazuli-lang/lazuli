# Cement R1 — execution plan (raw)

```json
{
  "plan": {
    "dag": [
      {
        "spec": "SPEC-01",
        "depends_on": []
      },
      {
        "spec": "SPEC-02",
        "depends_on": []
      },
      {
        "spec": "SPEC-03",
        "depends_on": []
      },
      {
        "spec": "SPEC-04",
        "depends_on": [
          "SPEC-01"
        ]
      },
      {
        "spec": "SPEC-05",
        "depends_on": [
          "SPEC-04"
        ]
      },
      {
        "spec": "SPEC-06",
        "depends_on": []
      },
      {
        "spec": "SPEC-07",
        "depends_on": [
          "SPEC-04"
        ]
      },
      {
        "spec": "SPEC-08",
        "depends_on": []
      },
      {
        "spec": "SPEC-09",
        "depends_on": []
      },
      {
        "spec": "SPEC-10",
        "depends_on": [
          "SPEC-01",
          "SPEC-02",
          "SPEC-03",
          "SPEC-04",
          "SPEC-05",
          "SPEC-06",
          "SPEC-07",
          "SPEC-08",
          "SPEC-09",
          "SPEC-12",
          "SPEC-13",
          "SPEC-14",
          "SPEC-15",
          "SPEC-16",
          "SPEC-17",
          "SPEC-18"
        ]
      },
      {
        "spec": "SPEC-11",
        "depends_on": [
          "SPEC-01",
          "SPEC-02",
          "SPEC-03",
          "SPEC-04",
          "SPEC-05",
          "SPEC-06",
          "SPEC-07",
          "SPEC-08",
          "SPEC-09",
          "SPEC-12",
          "SPEC-13",
          "SPEC-14",
          "SPEC-15",
          "SPEC-16",
          "SPEC-17",
          "SPEC-18"
        ]
      },
      {
        "spec": "SPEC-12",
        "depends_on": []
      },
      {
        "spec": "SPEC-13",
        "depends_on": []
      },
      {
        "spec": "SPEC-14",
        "depends_on": [
          "SPEC-09"
        ]
      },
      {
        "spec": "SPEC-15",
        "depends_on": []
      },
      {
        "spec": "SPEC-16",
        "depends_on": []
      },
      {
        "spec": "SPEC-17",
        "depends_on": [
          "SPEC-13"
        ]
      },
      {
        "spec": "SPEC-18",
        "depends_on": []
      }
    ],
    "waves": [
      {
        "wave": 1,
        "specs": [
          "SPEC-01"
        ],
        "mode": "sequential",
        "rationale": "SPEC-01 is the root: it single-sources the closed catalogs (reference-namespaces / scalars / semantic-scalars / field-markers) in lazuli_keywords and adds the CatalogKind classification + derive fns. SPEC-04 and SPEC-11 declare a hard dependency on it, and its SemanticScalar rows are the destination SPEC-04 retires the @-forms into ('forward-compatible by design'). It mutates registry.rs (3601 LOC, over ceiling — must land the catalog.rs/registry-split housekeeping first), vocab.rs and refs.rs (the shared namespace allowlists SPEC-07 also re-grounds), and types.rs (the shared scalar lifter SPEC-03/SPEC-04 also touch). Everything downstream derives from its registry. Runs alone in its own worktree so the registry-split + derive-fn ABI is stable before any consumer forks."
      },
      {
        "wave": 2,
        "specs": [
          "SPEC-02",
          "SPEC-03",
          "SPEC-06",
          "SPEC-08",
          "SPEC-09",
          "SPEC-12",
          "SPEC-13",
          "SPEC-15",
          "SPEC-16",
          "SPEC-18"
        ],
        "mode": "parallel",
        "rationale": "DISJOINT-FILE ISLANDS that do not depend on each other and touch non-overlapping parser/registry surfaces — each runs in its OWN git worktree (the project git discipline forbids parallel agents sharing one index). SPEC-02 is .lzx-surface-only (ux.rs split + board.rs/repeatable.rs, experience/audience.rs) with no .lzi overlap. SPEC-03 is the analyzer scalar-alias rejection (types.rs alias arms + new doctor rule) — note it shares types.rs with SPEC-04, but SPEC-04 is sequenced later (wave 3) so SPEC-03 lands the alias-rejection mechanism first; SPEC-04 then consumes it. SPEC-06 (compound-join renames public_contract/scope_override) touches feature_prelude.rs/auth/query parsers + adds 2 registry rows — disjoint registry scope from SPEC-08's Context::Tests. SPEC-08 (test-dialect fold) is confined to Context::Tests/Policy registry rows + .lzx experience/agent-evals parsers. SPEC-09 (retire `many` field) removes one registry type-ctor row + resource dispatcher guard. SPEC-12/15/16/18 are app-manifest GRAMMAR-DOC specs (docs/grammar.app.md + IR cross-check) touching disjoint app_manifest surfaces. SPEC-13 (enum storage-value grammar) is enum-doc + enums.rs-doc only. CAVEAT: all specs that regenerate tmLanguage/keyword-reference (SPEC-06, SPEC-08, SPEC-09) must SEQUENCE the final xtask regen step at integration — regenerate after all land, not concurrently, per the shared-generated-artifact rule; each authors its registry edit in its worktree and the regen is reconciled at merge."
      },
      {
        "wave": 3,
        "specs": [
          "SPEC-04"
        ],
        "mode": "sequential",
        "rationale": "@ off types (@semantic.X/@cap.X → bare PascalCase) DEPENDS ON SPEC-01 (its SemanticScalar registry rows are the retirement destination) and shares types.rs with SPEC-03 (consumes the alias-rejection path SPEC-03 built). It rewrites the 17 @semantic./@cap. sites in the SHARED full-capsule.lzi/app.lzi/registry.lzi/contracts/ai.lzi — the central fixture-churn hotspot. Must land BEFORE SPEC-05/SPEC-07 (both depend on it) and is serialized against SPEC-08/SPEC-09 which also edit full-capsule. Requires the registry.rs split (registry/decorators.rs + types_catalog.rs) as a structural prereq. Runs alone because it is the gate for the SPEC-04→05→07 chain and owns the largest shared-fixture rewrite."
      },
      {
        "wave": 4,
        "specs": [
          "SPEC-05",
          "SPEC-07",
          "SPEC-13"
        ],
        "mode": "sequential",
        "rationale": "SEQUENTIAL because all three converge on the SHARED full-capsule.lzi (and the shared registry/generated artifacts) and the project git discipline forbids contaminating a shared index. SPEC-05 (== for equality) depends on SPEC-04 and rewrites the 12 predicate `self.x = ...` sites in full-capsule.lzi — must run after SPEC-04's sigil rewrite settles to avoid a fixture merge race. SPEC-07 (policy-syntax coherence) depends on SPEC-04 (sigil doctrine) and renames CRUD policy categories across examples incl. full-capsule + crm. SPEC-17 (enum metadata colon-form grammar) depends on SPEC-13 (enum storage grammar must land first). Order within the wave: SPEC-05 → SPEC-07 → SPEC-17, each committing its full-capsule edits before the next forks, so the canonical example is never in a broken intermediate state. (If strict isolation per-worktree is enforced and full-capsule edits are partitioned by line-region, SPEC-05/SPEC-07 could parallelize — but the conservative call given the explicit fixture-collision risk is sequential.)"
      },
      {
        "wave": 5,
        "specs": [
          "SPEC-14"
        ],
        "mode": "sequential",
        "rationale": "SPEC-14 (retire `many_decl` from grammar.lzi.md line 337) is the GRAMMAR-DOC half of SPEC-09's parser/registry retirement of the `many` field form — it must land after SPEC-09 so the dead grammar rule is removed only once the parser actually hard-errors the form. Trivial docs-only follow-up; sequenced after its parser counterpart to keep grammar and parser in lockstep per the many-faces rule. Could fold into SPEC-09 as a single PR; kept separate here because discovery surfaced it independently."
      },
      {
        "wave": 6,
        "specs": [
          "SPEC-10",
          "SPEC-11"
        ],
        "mode": "sequential",
        "rationale": "BOOKEND-LAST specs that must observe the FINAL post-cut surface. SPEC-10 Phase B (author the curated doctor-clean example set + re-verify full-capsule) depends on EVERY syntax cut + every upgrade recipe being green — authoring curated examples before the cuts land would bake in soon-to-be-retired spellings (it explicitly lists 'ALL syntax-cut SPECs' as deps). SPEC-11 (docs 3-layer reorg + generated catalog-reference.md + CLAUDE.md/AGENTS.md reconciliation) depends on SPEC-01 (the generation mechanism) and must describe the final canonical forms of all other SPECs in CLAUDE.md/AGENTS.md as the LAST reconciliation. SEQUENTIAL because both touch CLAUDE.md/AGENTS.md and docs/* and the examples/ canon — SPEC-10 finalizes examples canon, then SPEC-11 points the docs/catalog layer at the settled examples. NOTE: SPEC-10 Phase A (deletion-safety audit + severing test couplings + deleting the 9 dirty flat .lzi + re-pointing DEFAULT_TEMPLATE off examples/crm.lzi) is independent of the syntax cuts and can be PULLED FORWARD to run in parallel during wave 2 in its own worktree (it only touches test-wiring + file removal, no grammar); only Phase B is gated last. Likewise SPEC-11's structural-reorg + de-dictatorialize half (tone pass + layer split) is orthogonal to keyword changes and can begin mid-campaign, with only the CLAUDE.md reconciliation + generated-catalog include held to the end."
      }
    ],
    "critical_path": [
      "SPEC-01",
      "SPEC-04",
      "SPEC-05",
      "SPEC-11"
    ],
    "risks": [
      "FIXTURE-CHURN COLLISION on examples/full-capsule/full-capsule.lzi is the dominant risk: SPEC-04 (17 @semantic./@cap. sites), SPEC-05 (12 predicate `self.x =` sites), SPEC-07 (policy category renames), SPEC-08 (eval `forbids`→`denies` + view `accepted by`→`allows extension` in the .lzx siblings), and SPEC-09 (`labels: many Label`→`has_many`) all rewrite the SAME canonical capsule. Per the project's git discipline ('parallel agents share a .git/index; broad-stage commands sweep siblings' staged work'), these MUST be serialized or each given an isolated worktree with line-region-partitioned edits. The DAG sequences SPEC-04→05→07 on the critical path and isolates SPEC-08/SPEC-09 in wave 2 worktrees, but the final merge of all full-capsule edits is the highest-contention integration point — reconcile full-capsule.lzi last, re-run `lazuli inspect`/`generate --check` after each merge, and never `git add -A`.",
      "SHARED REGISTRY + GENERATED-ARTIFACT contention: registry.rs (3601 LOC, already over the 500 ceiling), editors/vscode/syntaxes/lazuli.tmLanguage.json, and docs/keyword-reference.md are edited/regenerated by SPEC-01/02/04/06/07/08/09. tmLanguage + keyword-reference are GENERATED and must NEVER be hand-merged — regenerate via `xtask gen-tmlanguage`/`gen-keyword-reference` AFTER all registry-mutating specs land, not concurrently. The registry.rs split (SPEC-01 lands catalog.rs/decorators.rs/types_catalog.rs siblings) is a structural prerequisite SPEC-04/06/08 depend on; if SPEC-01's split is incomplete, every downstream registry add risks crossing the 500-LOC ceiling and forcing an unplanned re-split mid-wave.",
      "types.rs THREE-WAY OVERLAP: SPEC-01 (derive membership from registry), SPEC-03 (remove silent alias arms + ScalarAlias diagnostic), and SPEC-04 (delete @semantic./@cap. arms) all edit crates/lazuli_analyzer/src/types.rs (452 LOC). The DAG orders them SPEC-01 → SPEC-03 → SPEC-04 so each consumes the prior's mechanism (SPEC-04 reuses SPEC-03's alias-rejection + SPEC-01's derive fns), but if SPEC-03 and SPEC-04 are accidentally parallelized they will produce conflicting match-arm edits. Keep SPEC-03 in wave 2 and SPEC-04 in wave 3 strictly.",
      "BREAKING-CHANGE BLAST RADIUS: every spec is `breaking:true` (pre-users, approved). The campaign is only coherent if each spec ships its `lazuli upgrade`/`migrate dsl` recipe WITH the breaking parser change in the same wave, and the scaffold (lazurite/templates/default) + full-capsule are migrated in lockstep — otherwise `scaffold_from_template_smoke_tree_matches_expected` (which runs `lazuli doctor` on the scaffold) goes red. SPEC-08's eval `requires`/`forbids` recipe is SCOPE-GUARDED (must not rewrite feature-header `requires <dep>` or actor-matrix `forbids @role`) — a blind regex rewrite here corrupts non-eval sites; this recipe needs the dry-run-diff review gate the spec mandates.",
      "SPEC-10 / SPEC-11 LATE-BINDING FRAGILITY: both are gated behind ALL cuts. If any wave-2/3/4 spec slips, the entire bookend stalls (long critical-path tail). Mitigate by pulling SPEC-10 Phase A (examples deletion + DEFAULT_TEMPLATE re-point off examples/crm.lzi — a HARD include_str! blocker at crates/lazuli_cli/src/main.rs:101) and SPEC-11's tone/layer-split half FORWARD into wave 2 in isolated worktrees, leaving only Phase B authoring + CLAUDE.md/AGENTS.md byte-identical reconciliation truly last. SPEC-10's group_02 LSP include_str! block (references all 9 dirty files) must be re-pointed before any flat .lzi is deleted or the workspace won't compile.",
      "DISCOVERY-SPEC OVERLAP: SPEC-14 (retire `many_decl` grammar line) is the doc-half of SPEC-09 — risk of double-editing grammar.lzi.md if run uncoordinated; DAG makes SPEC-14 depend on SPEC-09. SPEC-13/SPEC-17 both touch enum grammar/parser docs (enums.rs, grammar.lzi.md enum section) — SPEC-17 depends on SPEC-13 to avoid concurrent enum-section edits. SPEC-12/15/16/18 are app-manifest grammar docs and are genuinely disjoint, but SPEC-15 (cookie/proxy/limits/headers) and SPEC-12 (locale/cors/logging/tracing/encryption) both extend docs/grammar.app.md §10+ — sequence their grammar.app.md edits or partition by section to avoid a docs merge race even though their code surfaces are disjoint."
    ],
    "notes": "SPEC count: 11 original + 7 discovery = 18 nodes. Critical path length = 4 (SPEC-01 → SPEC-04 → SPEC-05 → SPEC-11), with SPEC-11 standing in for the wave-6 bookend (SPEC-10 is parallel-eligible against SPEC-11 on Phase B but both serialize on CLAUDE.md/docs canon). The DAG's longest dependency chain by edges is SPEC-01→SPEC-04→SPEC-07 (3) and SPEC-01→SPEC-04→SPEC-05 (3), then the wave-6 bookend depends on the full closure. Discovery specs SPEC-12/13/15/16/18 are low-risk docs-grammar additions parallelizable in wave 2; SPEC-14 folds behind SPEC-09 and SPEC-17 behind SPEC-13. Two pull-forward optimizations are called out (SPEC-10 Phase A and SPEC-11 tone/layer-split half) that the orchestrator can dispatch in wave 2 worktrees to shorten the critical-path tail — they are listed in wave 6 for dependency-correctness but their non-gated halves are independent. Genuinely-parallel islands (own-worktree, disjoint files): wave 2's 10 specs. Everything touching full-capsule.lzi or registry.rs generated artifacts is serialized per the project's explicit shared-index git discipline.\""
  },
  "discovery": {
    "additional_specs": [
      {
        "proposed_id": "SPEC-12",
        "title": "App manifest grammar alignment — document undocumented blocks (locale, cors, logging, tracing, encryption)",
        "problem": "docs/grammar.app.md does not document five app-level blocks that are:\n(1) parsed and present in the IR (crates/lazuli_ir/src/nodes/app_manifest/mod.rs lines 122-124, 136-140, 150-155, 158-164, 167)\n(2) exemplified in production fixtures (examples/full-capsule/app.lzi:9, 53, 96-99, 121-141; examples/production-grade/app.lzi:17-20, 44-48, 69-76, 81-85)\nThe blocks are: `locale` (BCP-47 tags + fallback graph), `cors` (per-environment origin rules), `logging` (level/format/redact), `tracing` (propagate/sample_rate), `encryption` (key bindings). Each has typed sub-children per the IR. Grammar currently ends at §9 (Deploy), missing §10+ sections for these five. Severity: HIGH — blocks are loaded into IR but authors have no canonical surface grammar to reference.",
        "severity": "high",
        "evidence": "crates/lazuli_ir/src/nodes/app_manifest/mod.rs §'app_body' contains: `pub cors: Option<AppCors>` (line 124), `pub logging: Option<AppLogging>` (line 137), `pub tracing: Option<AppTracing>` (line 139), `pub encryption_bindings: Vec<EncryptionBinding>` (line 155), `pub locale: Option<AppLocale>` (line 149). All exported via `pub use` (lines 50, 54). examples/full-capsule/app.lzi lines 9-20 (locale), 53-58 (cors), 96-99 (locale_negotiate under runtime unit), 121-125 (logging), 126-129 (tracing), 136-141 (encryption). examples/production-grade/app.lzi §identical pattern at lines 17-20, 44-48, 69-76, 81-85."
      },
      {
        "proposed_id": "SPEC-13",
        "title": "Enum variant storage value grammar — document both integer and string forms, retire 'value' keyword",
        "problem": "grammar.lzi.md line 389 states `enum_variant = IDENT_LOWER ( \"value\" STRING )? NEWLINE ;` implying only `variant = \"string\"` is valid. The parser (crates/lazuli_syntax/src/parser/lzi/enums.rs lines 60-76) accepts THREE forms: (1) bare `draft`, (2) integer `archived = 9`, (3) string `active = \"live\"`. Test at lines 310-338 (`enum_metadata_preserves_bare_variants_and_storage_values`) explicitly validates `= <number>` alongside `= \"<text>\"`. The `value` keyword is a grammar fiction; the parser never requires it. Severity: MEDIUM — authors are misguided about valid syntax; codegen may generate incorrect schema; LSP completion suggests dead syntax.",
        "severity": "medium",
        "evidence": "crates/lazuli_syntax/src/parser/lzi/enums.rs lines 60-76: split on `=`, parse integer via `.parse::<i64>()` or quoted string (no `value` keyword parsing). Test source code line 317: `active = \"live\"` (string), line 318: `archived = 9` (integer). Both asserted to parse successfully lines 327-337. Zero references to `\"value\"` keyword in the parser source (grep -r '\"value\"' crates/lazuli_syntax/src/parser/lzi/enums.rs returns only unrelated docstring at line 122). AST struct EnumStorageValueDecl (crates/lazuli_syntax/src/ast/feature/enums.rs lines 49-54) is an `enum` with Integer/String variants — no `value` marker."
      },
      {
        "proposed_id": "SPEC-14",
        "title": "Retire 'many_decl' field form from grammar (line 337) — never implemented in parser",
        "problem": "grammar.lzi.md line 337 specifies `many_decl = IDENT_LOWER \":\" \"many\" IDENT_UPPER NEWLINE ;` as a resource-level field form analogous to `has_many`. The parser (crates/lazuli_syntax/src/parser/lzi/resource/field.rs) does not recognize or parse this form. ResourceDecl AST (crates/lazuli_syntax/src/ast/resource.rs lines 36-116) has no variant for it; only `has_many: Vec<ResourceHasMany>` (line 47) exists. The grammar lists `many_decl` as an alternative in `resource_body` (line 246) but it is unreachable. Severity: MEDIUM — harmless dead grammar rule that confuses authors and fails parity audits.",
        "severity": "medium",
        "evidence": "grammar.lzi.md line 246: `| many_decl` listed as branch of `resource_body` alternatives. Line 337 defines the rule. Parser: grep -r 'many.*Type\\|: many' crates/lazuli_syntax/src/parser/lzi/ returns only `many_through`, never `many_decl`. crates/lazuli_syntax/src/ast/resource.rs has_many slot (line 47) is `Vec<ResourceHasMany>` (1:N inline), not a field-type. No corresponding AST node for field-level many. The 'many' form was designed as an alias for has_many but never wired."
      },
      {
        "proposed_id": "SPEC-15",
        "title": "App manifest security & locality blocks — grammar missing cookie, proxy, limits, headers sub-grammars",
        "problem": "grammar.app.md does NOT document four typed blocks at the app level: `cookie` (RFC 6265 attributes), `proxy` (trust chain), `limits` (request/connection caps), `headers` (security headers). All four are present in the IR (crates/lazuli_ir/src/nodes/app_manifest/security.rs: AppCookie lines 144-152, AppProxy lines 175-188, AppLimits lines 190-206, AppHeaders lines 34-53) with typed sub-children. All four are serializable in AppManifest (crates/lazuli_ir/src/nodes/app_manifest/mod.rs lines 156-167). Severity: HIGH — security-critical blocks with no public grammar makes policy audits and schema validation impossible.",
        "severity": "high",
        "evidence": "crates/lazuli_ir/src/nodes/app_manifest/mod.rs: `pub cookie: Option<AppCookie>` (line 157), `pub proxy: Option<AppProxy>` (line 160), `pub limits: Option<AppLimits>` (line 163), `pub headers: Option<AppHeaders>` (line 166). Each struct is a closed-vocabulary shape (e.g., AppCookie.name, .same_site, .secure, .http_only, .domain, .path per docstring lines 878-918 of grammar.lzi.md which describes the SIBLING location in auth.sessions). No corresponding parser in crates/lazuli_syntax/src/parser/lzx/app/mod.rs (line 219-221 only parses `.title`, `.version`, `.targets`, `.default_locale`, etc.; no .cookie/.proxy/.limits/.headers)."
      },
      {
        "proposed_id": "SPEC-16",
        "title": "Declare runtime locale_negotiate parsing contract — feature-level block lives in runtime units",
        "problem": "grammar.app.md runtime units section (§6 lines 189-208) does NOT document the `locale_negotiate` block that appears in examples at app.lzi:96-99 and is parsed by crates/lazuli_syntax/src/parser/lzi/api.rs (lines 130-139, which imports parse_locale_negotiate_decl from locale.rs). The block is a runtime-unit child that declares request-locale selection strategy (source/strategy/fallback). Severity: MEDIUM — block is parseable but not in any grammar doc; LSP and codegen developers must reverse-engineer from example fixtures.",
        "severity": "medium",
        "evidence": "examples/full-capsule/app.lzi lines 96-99 under `runtime` > `unit api` block: `locale_negotiate`, `source accept_language`, `strategy best_match`, `fallback \"pt-BR\"`. crates/lazuli_syntax/src/parser/lzi/locale.rs: public function `parse_locale_negotiate_decl` (line 25) with docstring (lines 1-18) describing the block. grammar.app.md §6 (runtime_block production, lines 189-208) does not mention locale_negotiate; runtime_unit_body lists only `serves`, `runs`, `healthcheck`, `readiness`, `rate_limit` (lines 195-200)."
      },
      {
        "proposed_id": "SPEC-17",
        "title": "Unify enum metadata surface — document `: label @translation.*, hint @translation.*, icon \"...\"` colon form",
        "problem": "Parser accepts enum variant metadata via colon suffix (crates/lazuli_syntax/src/parser/lzi/enums.rs lines 52, 56-59, 106-123: parse_enum_variant_metadata) but grammar.lzi.md does NOT document it. The rule is line 389 `enum_variant = IDENT_LOWER ( \"value\" STRING )? NEWLINE ;` — no colon clause. Tests (lines 311-338) show actual authored form: `active = \"live\": label @translation.status_active, icon \"check\"`. The colon separates storage value from label/hint/icon metadata, allowing multi-field decoration inline. Severity: MEDIUM — feature works but is undiscoverable from grammar alone.",
        "severity": "medium",
        "evidence": "crates/lazuli_syntax/src/parser/lzi/enums.rs: line 52 calls `find_enum_metadata_separator(body)` to detect `:`, line 56-59 destructures into `(main_body, metadata_body)`. Function `parse_enum_variant_metadata` (lines 125-179) validates `label <key>`, `hint <key>`, `icon \"<name>\"` comma-separated items after the colon. Test assertions (lines 332-333): `.label_key.as_deref() Some(\"status_active\")`, `.icon_key.as_deref() Some(\"check\")` from source line 317 `active = \"live\": label @translation.status_active, icon \"check\"`."
      },
      {
        "proposed_id": "SPEC-18",
        "title": "Gateway schema for app.lzi — codegen observability and deployment metadata blocks present in IR but unspecified",
        "problem": "The IR AppManifest contains several fields with no corresponding grammar rules or parser support: (1) `subscription resource <feature>.<Resource>` (line 15 of production-grade/app.lzi, plan-and-gate anchor), (2) `lazuli_version` (line 10 of same), both loaded during lowering but not in grammar.app.md. The analyzer silently ignores them during parsing (no error, no warning) because the lzx parser only recognizes the documented subset. Severity: MEDIUM — fields are loadable via JSON IR but source authors cannot author them in canonical indent form.",
        "severity": "medium",
        "evidence": "examples/production-grade/app.lzi lines 10-15: `lazuli_version \"0.13\"` and `subscription resource billing.subscription`. crates/lazuli_ir/src/nodes/app_manifest/mod.rs: `pub lazuli_version: Option<String>` (line 92) with docstring lines 88-90 explaining the field. No corresponding parser in crates/lazuli_syntax/src/parser/lzx/app/mod.rs — parse_lzx_app (lines 121-244) does not recognize either `lazuli_version` or `subscription` keywords."
      }
    ]
  },
  "reviews": [
    {
      "verdict": "All 11 specs are unusually thorough on parity surfaces — most enumerate parser, registry, analyzer-lowering, LSP (completion/hover/typo), tmLanguage (generated), keyword-reference (generated), curated grammar docs, scaffold/examples, freshness gates, a doctor rule, tests, and a migration recipe. The \"many faces\" definition-of-done is largely satisfied. However I found concrete completeness gaps: (1) two BLOCKER-class cross-spec collisions — SPEC-01 and SPEC-03 both rewrite the SAME types.rs:160-178 alias arms and both ship a scalar-alias doctor rule under DIFFERENT codes (VOCAB-SCALAR-ALIAS-001 vs SCALAR-ALIAS-001) while declaring themselves parallelizable; and SPEC-02 is built on a FALSE premise (it claims LZX-REPEATABLE-SUM-001/LZX-BOARD-LANES-001 are unimplemented, but they ARE implemented in doctor_run/lzx/ux_rules.rs + aggregators/lzx_ux.rs, which SPEC-02 then omits from its parity surfaces). (2) Several missing-surface and missing-dependency gaps detailed below. Verdict: specs are well-researched and parity-aware, but the SPEC-01/03 doctor-rule-code collision and SPEC-02's wrong-premise/missing-surface must be resolved before execution, and SPEC-05's spurious dependency on SPEC-04 should be re-examined.",
      "gaps": [
        {
          "spec": "SPEC-01 + SPEC-03",
          "gap_type": "doctor_rule_collision",
          "detail": "BLOCKER cross-spec collision on the scalar-alias enforcement. Both specs rewrite the IDENTICAL alias match arms at crates/lazuli_analyzer/src/types.rs:160-178 (verified: \"ID\"|\"Id\", \"Text\"|\"String\", \"Boolean\"|\"Bool\", \"Integer\"|\"Int\", \"Decimal\"|\"Float\", \"JSON\"|\"Json\", bare Email) and both create a NEW scalar-alias doctor rule — SPEC-01 names it VOCAB-SCALAR-ALIAS-001 (file crates/lazuli_doctor/src/vocab/vocab_scalar_alias_001.rs) while SPEC-03 names it SCALAR-ALIAS-001 (also crates/lazuli_doctor/src/vocab/vocab_scalar_alias_001.rs). Same file path, two different rule codes, same target arms. SPEC-01 declares parallelizable_with:[SPEC-02,SPEC-03] and SPEC-03 declares parallelizable_with:[SPEC-02] — i.e. they assert mutual parallelizability while editing the same load-bearing match block and the same doctor file. They also disagree on the migration vehicle: SPEC-01 routes autocorrect through `lazuli upgrade`/canonical_scalar(); SPEC-03 routes it through `lazuli migrate dsl` + `lazuli fix`. These must be merged into ONE spec or one made strictly dependent on the other with a single canonical rule code; otherwise the second to land will conflict-edit types.rs and double-register the doctor rule.",
          "severity": "blocker"
        },
        {
          "spec": "SPEC-02",
          "gap_type": "false_premise_and_missing_surface",
          "detail": "SPEC-02 evidence claims 'Documented LZX-REPEATABLE-SUM-001 and LZX-BOARD-LANES-001 doctor rules are unimplemented (grep returns no files; only rule_category.rs mentions LZX-)'. This is FALSE. They are implemented: crates/lazuli_doctor_run/src/doctor/lzx/ux_rules.rs declares `pub const REPEATABLE_SUM_CODE = \"LZX-REPEATABLE-SUM-001\"` (line 67) and `pub const BOARD_LANES_CODE = \"LZX-BOARD-LANES-001\"` (line 63), with the aggregator at crates/lazuli_doctor_run/src/doctor/aggregators/lzx_ux.rs and dispatch wiring at crates/lazuli_doctor_run/src/doctor/dispatch.rs:169-170, plus the kitchen-sink guard at crates/lazuli_cli/tests/kitchen_sink_e2e_guard.rs:109-111. Consequently SPEC-02's plan to 'replace the documented-but-unimplemented LZX-REPEATABLE-SUM-001 with a real type-catalog check' is wrong, and its parity_surfaces list OMITS the real surfaces it must touch when the repeatable form changes from brace to indentation: lzx/ux_rules.rs (REPEATABLE_SUM check walks repeatable_groups), aggregators/lzx_ux.rs, and dispatch.rs. Since SPEC-02 adds LZX-REPEATABLE-FIELD-SCALAR-001 it must reconcile with the EXISTING LZX-REPEATABLE-SUM-001 rather than 'replace' a phantom.",
          "severity": "blocker"
        },
        {
          "spec": "SPEC-01",
          "gap_type": "classification_error",
          "detail": "SPEC-01 end_state classifies the CatalogKind variants but puts Money in the wrong bucket: it lists 'Scalar (Text/Integer/Decimal/Date/DateTime/JSON/ID/Boolean/Money)'. The analyzer (verified crates/lazuli_analyzer/src/types.rs:172-174) lowers bare `Money` to ir::BuiltinType::SemanticMoney{currency:BRL}, NOT to a plain scalar — it is a semantic scalar (same family as @semantic.Money at types.rs:152). SPEC-01's own SemanticScalar list (Email/Phone/Url/Uuid/Currency/GeoPoint/HexColor/Percentage) OMITS Money. This mis-classification would make the catalog_classification.rs snapshot test and the analyzer membership lookups disagree with reality. Money must be in SemanticScalar, not Scalar.",
          "severity": "major"
        },
        {
          "spec": "SPEC-05",
          "gap_type": "spurious_dependency",
          "detail": "SPEC-05 declares dependencies:[SPEC-04]. SPEC-05 changes the equality operator from `=` to `==` in the closed predicate language (agent.rs:342, query.rs:219, workflow.rs, view_guard.rs). SPEC-04 retires @semantic./@cap. type sigils. There is no surface overlap: SPEC-05 touches operator tables and predicate parsers; SPEC-04 touches type lifting and the registry decorator rows. The only shared file is examples/full-capsule which both migrate, but that is a coordination concern (sequence the example edits), not a hard build dependency. The stated dependency will needlessly serialize two independent changes. If the intent is purely 'author the migrated example after both land', that is an examples-ordering note (the SPEC-10 concern), not a SPEC-05 build dependency. Re-examine and downgrade to parallelizable-with-coordination unless a concrete shared symbol is identified.",
          "severity": "major"
        },
        {
          "spec": "SPEC-02",
          "gap_type": "missing_test",
          "detail": "SPEC-02 retires the @command sigil on inline_table on_change and reviews the @command decorator at registry.rs:3218, but the @command namespace is ALSO a reference-namespace that the doctor allowlist (refs.rs) and LSP allowlist (vocab.rs:264) accept for `@feature`/`@command`/`@file`/`@audience`. SPEC-02 does not state whether the @command DECORATOR row is removed from the registry or retained (it is still a valid reference namespace per vocab.rs). If the row is removed, proven_complete.rs and is_allowed_reference_namespace lose @command and SPEC-01's reference_namespaces() derive shrinks. SPEC-02 needs an explicit test asserting whether @command survives as a reference-namespace decorator (for any other use) or is fully retired, to avoid silently dropping it from the SPEC-01 catalog.",
          "severity": "major"
        },
        {
          "spec": "SPEC-06",
          "gap_type": "missing_migration_recipe_mechanism",
          "detail": "SPEC-06 (rename public contract->public_contract, scope override->scope_override) ships a recipe.toml with kind=rename, but apply_recipe in crates/lazuli_cli/src/upgrade.rs:137-139 ONLY implements 'additive' ('other => Err(... not yet implemented)'). SPEC-06's migration_recipe field acknowledges this with an either/or ('implement the rename recipe kind ... OR ship as a lazuli doctor fix autofix'), and its dependencies field says 'Sequence after a migration-engine SPEC if one implements the rename recipe kind'. But NO spec in this set is designated as the migration-engine SPEC that implements the rewrite/rename kind. SPEC-02, SPEC-05, SPEC-08, SPEC-09 each independently say they must 'extend apply_recipe'. There is no single owner for extending the upgrade engine, so each breaking spec re-implements or stubs the same upgrade.rs:137 match — a coordination gap that will cause repeated conflicting edits to apply_recipe. A foundational 'extend upgrade engine with rewrite/rename kinds' spec or explicit owner is missing.",
          "severity": "major"
        },
        {
          "spec": "SPEC-08",
          "gap_type": "missing_surface",
          "detail": "SPEC-08 renames AgentEvalKind::Requires/Forbids -> Allows/Denies and folds accepted-by/rejected-by. It lists the analyzer eval lowering (agent.rs + report.rs) but the registry has `requires` at THREE scopes (verified: registry.rs:1725 FeatureHeader, :2732, :3408 Tests/predicate-connector) and `forbids` at TWO (:2566 Tests, :2630 Policy). SPEC-08's parity surface only says to DROP `accepted`/`rejected` and ADD `extension` — it does NOT specify what happens to the eval-scoped `requires`/`forbids` rows in the registry. Since eval verbs move to allows/denies, any Tests-scope row that existed ONLY for the eval meaning must be removed, but the feature-dependency `requires` (1725) and policy `forbids` (2630) must survive. SPEC-08 must explicitly enumerate which of the registry.rs:1725/2566/2630/2732/3408 rows are kept vs dropped, or proven_complete.rs parity will break ambiguously. The end_state prose is correct in intent but the parity_surfaces registry bullet is under-specified.",
          "severity": "major"
        },
        {
          "spec": "SPEC-10",
          "gap_type": "coverage_gap",
          "detail": "SPEC-10 problem says 'Nine of twelve' top-level flat .lzi carry the disclaimer and lists nine + customer-capsule for deletion. But the examples/ directory (verified) also contains notification.lzi and crm.lzi as flat top-level .lzi files. crm.lzi is handled (the include_str! template rehome). notification.lzi is NOT mentioned anywhere in SPEC-10 — not in the deletion list, not in the evidence, not in the keep-list. Either it is undisclaimed-and-clean (then it still violates the directory-shape canon SPEC-10 establishes and the examples_are_doctor_clean walker must cover it) or it is another dirty fixture missed by the audit. SPEC-10's Phase-A deletion-safety matrix is incomplete without a verdict on notification.lzi.",
          "severity": "minor"
        },
        {
          "spec": "SPEC-04",
          "gap_type": "missing_migration_recipe_mechanism",
          "detail": "SPEC-04 ships migrations/recipes/.../at-off-types-rewrite/recipe.toml with kind=\"rewrite\" and relies on the smoke harness (upgrade.rs:143-191) for IR-equality. Like SPEC-06, the 'rewrite' kind is not implemented in apply_recipe (upgrade.rs:137 only handles 'additive'). SPEC-04 does not flag this engine gap in its migration_recipe field the way SPEC-06 does, nor does it depend on a migration-engine spec. The recipe will not actually apply via `lazuli upgrade --to` until the rewrite kind is implemented. SPEC-04 should either declare the engine dependency or include the apply_recipe extension in its own scope.",
          "severity": "major"
        },
        {
          "spec": "SPEC-03",
          "gap_type": "missing_surface",
          "detail": "SPEC-03 claims scalars 'are NOT in the keyword registry' and that proven_complete.rs 'scans only crates/lazuli_syntax/src/**' so the silent surface escaped every parity gate — used to justify NOT touching the registry. But SPEC-01 (which SPEC-03 declares itself parallelizable with) ADDS scalars + semantic-scalars AS registry rows. If SPEC-01 lands, SPEC-03's premise ('scalars aren't registry keywords, so tmLanguage type-name block is hand-curated not xtask-generated') becomes false and its tmLanguage edit instructions (hand-edit tmLanguage.json:2829) conflict with SPEC-01's 'regenerate via xtask' instruction for the same scalar highlight group. SPEC-03 and SPEC-01 give CONTRADICTORY instructions for the scalar tmLanguage surface (hand-edit vs regenerate). This is a direct parity-surface ownership conflict that the 'parallelizable' claim hides.",
          "severity": "blocker"
        },
        {
          "spec": "SPEC-11",
          "gap_type": "missing_dependency",
          "detail": "SPEC-11 declares dependencies:[SPEC-01] and is the docs/CLAUDE.md reconciliation 'keystone-last' spec that must 'describe the final state' of every other SPEC's canonical forms (it says so: 'reflect every other SPEC's final canonical forms'). But it only depends on SPEC-01. To document the FINAL canonical surface (== for equality from SPEC-05, public_contract from SPEC-06, allows/denies from SPEC-08, has_many from SPEC-09, bare semantic/cap types from SPEC-04), SPEC-11 must depend on ALL of SPEC-02..SPEC-10, not just SPEC-01. Its own justification says 'runs last for the CLAUDE.md/AGENTS.md reconciliation' — that intent requires the full dependency set. As written, SPEC-11 could land after only SPEC-01 and bake in pre-migration spellings for the other cuts, exactly the drift it aims to kill.",
          "severity": "major"
        },
        {
          "spec": "SPEC-07",
          "gap_type": "missing_surface",
          "detail": "SPEC-07 reclassifies @role/@scope/@actor from decorator() to a catalog-atom kind in the registry (registry.rs:3204-3207, verified those rows exist as plain decorator()). This is the SAME registry rows SPEC-04 touches when removing @cap and re-scoping @semantic, and the same CatalogKind/classification machinery SPEC-01 introduces. SPEC-07 depends on SPEC-04 (correct) but NOT on SPEC-01, even though the 'catalog-atom kind' it needs is precisely SPEC-01's CatalogKind enum (ReferenceNamespace covers role/scope/actor/policy). Without SPEC-01's CatalogKind, SPEC-07 must invent its own kind facet, duplicating SPEC-01's work and risking a second classification axis on CapabilitySpec. SPEC-07 should depend on SPEC-01 (the classification owner), or SPEC-01/04/07 must agree on a single kind facet.",
          "severity": "major"
        },
        {
          "spec": "SPEC-09",
          "gap_type": "missing_migration_recipe_mechanism",
          "detail": "SPEC-09 migration_recipe sets recipe.toml kind=\"rewrite\" and explicitly hedges 'if only additive/rename exist per upgrade.rs:137-141, extend apply_recipe minimally to handle this line-rewrite or ship it as a documented manual recipe'. Verified: only 'additive' is implemented. So SPEC-09, like SPEC-04/05/06/08, needs the upgrade engine extended. The recurring pattern across five breaking specs (02,04,05,06,08,09 all need a non-additive recipe kind) confirms the systemic gap: there is no spec owning the upgrade-engine extension, and each spec's migration recipe will silently fail to apply via `lazuli upgrade` until that engine work lands. Recommend a precursor spec (or fold the engine extension into the first breaking spec) so the rest can depend on it.",
          "severity": "major"
        },
        {
          "spec": "SPEC-08",
          "gap_type": "missing_test",
          "detail": "SPEC-08 renames the IR ViewTestAssertion::AcceptedBy/RejectedBy -> AllowsExtension/DeniesExtension (serde tag change allows_extension/denies_extension) and AgentEvalKind serde rename. These are IR serde-tag changes that affect any frozen expected_ir.json snapshot (SPEC-10's curated examples carry expected_ir.json, and examples_bundle round-trips IR). SPEC-08's tests_required covers the IR round-trip unit tests but does NOT mention re-freezing any committed expected_ir.json / inspect-snapshot fixtures that contain view-test or eval assertions. If SPEC-10's full-capsule or any golden IR snapshot includes these assertions, the serde-tag rename breaks the snapshot. SPEC-08 needs an explicit 'regenerate any committed IR snapshots containing view-test/eval assertions' test/step (and coordination with SPEC-10).",
          "severity": "minor"
        },
        {
          "spec": "SPEC-06",
          "gap_type": "missing_surface",
          "detail": "SPEC-06 problem field admits it was truncated ('Full detail ... carried in the end_state and evidence fields because the harness truncates whatever field directly follows a long problem field'). The parity_surfaces list is consequently terse. It lists feature_prelude.rs:25 and query/list.rs:80 (verified correct) but its LSP surface bullet only names keywords.rs/hover/diagnostics generically without confirming the typo-catalog old->new mapping (public contract->public_contract) is added to the LSP typo catalog so authors who type the retired space form get a completion fix. The 'many faces' DoD requires the typo catalog face; SPEC-06 mentions 'add typo catalog old to new' in one parity bullet but does not list it in tests_required as an explicit assertion. Minor: add a test that the LSP typo catalog maps the retired space spelling to the underscore form.",
          "severity": "minor"
        }
      ]
    },
    {
      "verdict": "LOC-discipline review across 11 specs, grounded against the actual tree. One BLOCKER and three MAJOR gaps.\\n\\nBLOCKER (cross-cutting): The 500-LOC ceiling that every loc_plan is judged against is already violated by ~40 files in the current tree, most egregiously registry.rs at 3601 LOC. CLAUDE.md:250 claims 'No exceptions, 0 above 500' but that is stale. The consequence: specs that edit registry.rs in-place (SPEC-01, SPEC-04, SPEC-06, SPEC-11) cannot truthfully satisfy a 'touched .rs <=500' acceptance criterion unless the registry is actually split first. Only SPEC-04 commits to that split as a prerequisite, and no spec sequences SPEC-01/06/11 after it. This ordering must be made explicit or the acceptance criteria are unachievable.\\n\\nMAJOR 1 (SPEC-06): Fabricated LOC exception. SPEC-06 alone touches registry.rs with NO split plan, justifying it as 'the documented pure-data-const exception to the 500 ceiling.' No such exception exists anywhere in CLAUDE.md or AGENTS.md (verified by grep). This both evades the discipline and contradicts SPEC-04/SPEC-11, which treat the same file as over-budget needing a split. Must be reconciled — either document a real registry exception (and then SPEC-04/11's split prereq is wrong) or remove the false claim and give SPEC-06 a real split plan.\\n\\nMAJOR 2 & 3 (SPEC-04, SPEC-11): The registry split mechanism is under-specified. The ALL registry is a single 'pub const ALL: &[CapabilitySpec] = &[ ... ]' array literal spanning ~3000 lines (verified). The plans say 'include! the rows' or 'const fn concat,' but you cannot include! a fragment of a const array literal and stay ABI-additive without restructuring ALL into named const sub-slices and concatenating them in const context — which Rust does not do trivially (no stable const slice concat; needs a fixed-size const array builder). The split is the right call; the hardest part is hand-waved.\\n\\nThe remaining specs (SPEC-01, 02, 03, 05, 07, 08, 09, 10) have credible split strategies: concrete mod.rs-re-exporter + sibling-file plans, named include!(tests/...) hatches for inline test blocks, and (SPEC-05/SPEC-10) splits that NET-REDUCE LOC by deduping. SPEC-02 (ux.rs 747→split into repeatable.rs/board.rs) and SPEC-09 (resource/mod.rs already 597) correctly reckon with files already over the ceiling, though SPEC-09's plan only diverts NEW tests and leaves the existing 184-LOC inline block in place, so mod.rs stays ~609 — it should also extract the existing test block to fully comply.\\n\\nNet: the per-spec split craftsmanship is mostly good; the systemic problems are (a) the baseline invariant is already broken, making 'touched .rs <=500' criteria unachievable for registry-editing specs without the SPEC-04 split landing first, and (b) SPEC-06's fabricated exception plus SPEC-04/11's under-specified const-array split mechanism.",
      "gaps": [
        {
          "spec": "SPEC-06",
          "gap_type": "false-loc-claim",
          "detail": "loc_plan asserts: 'registry.rs is 3601 LOC and is the documented pure-data-const exception to the 500 ceiling, so add rows and do not split it here.' This is FABRICATED. CLAUDE.md:250 states the ceiling is '...every .rs file under crates/ <= 500 LOC. Production AND test code. No exceptions.' A grep of CLAUDE.md/AGENTS.md for any registry/data-const/exception/exempt language returns ZERO hits. There is no documented exception. SPEC-06 is the ONLY spec that touches registry.rs without a split plan, justifying that omission with a non-existent carve-out. Because the file is already at 3601 LOC (7x over), adding ~14 LOC of rows in-place keeps it badly over budget with no remediation, and the spec explicitly refuses to split it. Either the team has an undocumented real policy (in which case SPEC-04/06/11 contradict each other on the same file and must be reconciled), or this is a credibility gap that lets one spec evade the discipline while a sibling spec (SPEC-04) treats the same file as a hard split prerequisite.",
          "severity": "major"
        },
        {
          "spec": "SPEC-04",
          "gap_type": "underspecified-split-mechanism",
          "detail": "SPEC-04 correctly identifies registry.rs as 'already OVER the 500 ceiling' and makes a registry split a 'pre-req housekeeping' commit. But the proposed mechanism is not mechanically credible as stated: it says extract the decorator-rows block into 'a sibling registry/decorators.rs + registry/types_catalog.rs included via include! or a const fn concat.' The actual structure (verified) is a SINGLE 'pub const ALL: &[CapabilitySpec] = &[ ... ]' array literal spanning lines 577-~3500. You cannot include!() a fragment of a const array literal and preserve it as ABI-additive without restructuring ALL into named const sub-slices (e.g. const DECORATORS: &[CapabilitySpec] = &[...]) and concatenating them — and Rust const slice concatenation is non-trivial (no stable slice concat in const context; requires a const fn array builder over fixed sizes). The 'include! a chunk of the literal' framing under-specifies this and risks a non-compiling or non-additive split. The split is still the right call; the plan just hand-waves the hardest part.",
          "severity": "major"
        },
        {
          "spec": "SPEC-11",
          "gap_type": "underspecified-split-mechanism",
          "detail": "Same registry-literal hazard as SPEC-04. loc_plan says 'put the catalog rows + builder in a sibling crates/lazuli_keywords/src/catalogs.rs (~150 LOC) re-exported from registry.rs (pub use catalogs::*;) to stay additive and under budget.' Re-exporting helper fns via pub use is fine, but the spec also needs new catalog/member rows to participate in the proven_complete parity (it says proven_complete must assert registry namespace+member set equality), which implies the rows enter the ALL slice — and the same single-const-array constraint applies. The 150-LOC sibling for catalog DATA only stays additive if those rows are a separately-named const that ALL references, which again hits const-slice-concat. The xtask catalog_reference.rs split (~220 LOC with a named sub-split to mod.rs+namespaces.rs+semantics.rs+scalars.rs) is credible and well-scoped. The doctor-rule file splits are credible. Only the registry-data half is under-specified.",
          "severity": "minor"
        },
        {
          "spec": "SPEC-09",
          "gap_type": "test-host-over-ceiling",
          "detail": "resource/mod.rs is verified at 597 LOC TODAY (already over the 500 ceiling), with the production half ending at line 412 and '#[cfg(test)] mod resource_block_parser_tests' starting at 413 (so production ~318, test ~184 inline, total 597). SPEC-09 correctly diagnoses this and proposes adding the +12 production guard (keeping production ~330, fine) and routing NEW parser tests into a co-located sibling via include! to keep mod.rs's inline test block from crossing 500. The plan is credible and is the only spec that correctly reckons with this already-over file. Minor gap: the file is ALREADY 597 (>500) before SPEC-09 touches it, and SPEC-09's plan does not move the existing inline 184-LOC test block out — it only diverts NEW tests. So after SPEC-09, mod.rs is still ~609 LOC (597 + 12 guard), STILL over the ceiling. To actually satisfy the invariant the spec must also extract the existing resource_block_parser_tests block to a sibling, which the plan does not commit to. The split strategy exists but is incomplete: it addresses incremental growth, not the pre-existing overage it is editing into.",
          "severity": "minor"
        },
        {
          "spec": "SPEC-02",
          "gap_type": "credible-split-residual-risk",
          "detail": "surface/ux.rs is verified at 747 LOC (over ceiling) and SPEC-02 has the most concrete split plan of any spec: extract repeatable.rs (~140) and board.rs (~100) as siblings, leaving ux.rs at ~480, with pub(in crate::parser::lzx) re-exports so view.rs/audience.rs call sites are unchanged (ABI-additive). This is the canonical mod-sibling pattern and is mechanically sound. Residual risk: the ~480 estimate for the ux.rs remainder is tight against 500 and depends on the inline test block — surface_tests/ux.rs is a SEPARATE file (296 LOC, verified, fine), but if any inline #[cfg(test)] lives in ux.rs the 480 figure could slip over. The plan names the include-sibling escape hatch for surface_tests/ux.rs growth, so the residual is small. Net: strong split plan, low residual risk.",
          "severity": "minor"
        },
        {
          "spec": "SPEC-08",
          "gap_type": "thin-margin-no-firm-split-trigger",
          "detail": "experience/mod.rs is 409 LOC; loc_plan adds ~+10 (→~419) and evals.rs is 424→~433. Both stay under 500, so no split is strictly required, and the plan correctly names the extract-to-sibling escape hatch ('if it crosses 500 it will not, extract the tests inner loop into view_tests.rs'). The three new doctor rule files (~120-180 each) are credibly under budget. The risk is only that the +10/+8 estimates are optimistic for a rename+fold that also adds two retired-spelling hard-error branches each plus negative tests; if test bodies are added inline rather than in siblings, evals.rs (already 424, with an existing evals_tests module in-file) could cross 500. The plan should commit to placing the new negative tests in a sibling rather than leaving it conditional. Low severity because the fold is genuinely small and the escape hatch is named.",
          "severity": "minor"
        },
        {
          "spec": "SPEC-01",
          "gap_type": "credible-split-plan",
          "detail": "No blocking LOC risk. lib.rs is verified at 542 (already 42 over ceiling); the plan extracts the catalog concern into a NEW sibling catalog.rs (~180) re-exported via pub use catalog::*, adding only a ~3-LOC field to CapabilitySpec in lib.rs — that does NOT bring lib.rs under 500 (it stays ~545), but the catalog extraction is net-neutral-to-additive and the file was already over before this spec. The registry additions hit the same single-const-array constraint as SPEC-04/11 (the plan's fallback 'land additions in a NEW registry/catalog_rows.rs included additively' is the same under-specified include!-a-const-fragment move). keyword_reference.rs is 380→~440 with a named extract-to-catalog_reference.rs hatch if it crowds 500 (credible). The two new doctor files (~120/~150) are credibly under budget. Overall the split strategy is concrete except for the shared registry-literal hazard.",
          "severity": "minor"
        },
        {
          "spec": "SPEC-03",
          "gap_type": "credible-split-plan",
          "detail": "No blocking LOC risk and arguably the cleanest plan. types.rs is verified at 452 (under ceiling); replacing six alias arms with a ~25 LOC helper plus the named escape hatch (extract the scalar block to types/scalars.rs ~120, re-exported pub(crate) use) keeps it safe. The new doctor rule vocab_scalar_alias_001.rs (~160-200, modeled on the verified 408-LOC vocab_grammar_form_001.rs) with the named include!(tests/...) split if fixtures push >500 is the established pattern. LSP/tmLanguage edits are literal trims (LOC-neutral). Migrate-dsl/lazuli_fix additions are ~30-60 LOC into existing files — the spec does not state those host files' current sizes, a small omission, but the additions are small enough to be low risk.",
          "severity": "minor"
        },
        {
          "spec": "SPEC-05",
          "gap_type": "credible-split-plan",
          "detail": "No blocking LOC risk. agent.rs (387), query.rs (440), workflow.rs (338), helpers.rs (327) all verified under ceiling; edits are in-place token swaps (+5 to +15 LOC each). The plan's optional extract of a shared predicate_ops.rs (~80) actually DEDUPES the two copy-pasted operator tables (agent.rs:336-343 and query.rs:213-220) — a net simplification that lowers, not raises, LOC pressure. The new doctor file predicate_eq_operator_001.rs (~180-220, modeled on the verified 327-LOC webhook_emit_predicate_field_001.rs) is credibly under budget. upgrade.rs is 296; the named extract to upgrade/rewrite_predicate_eq.rs (~90) if apply_recipe grows is a sound hatch. Strong plan.",
          "severity": "minor"
        },
        {
          "spec": "SPEC-07",
          "gap_type": "credible-split-plan",
          "detail": "No blocking LOC risk. policy/namespace.rs verified at 208 (well under); the three new doctor rules go in separate one-rule-per-file modules (~120/180/140) registered additively from lzi_hygiene/mod.rs (verified 64 LOC, ample room). The inline-test escape hatch via include!(tests/<concern>_tests.rs) is named. The registry change is described as a 'few-line kind reclassification' with the new CapabilitySpec kind variant landing additively in lib.rs — this is genuinely small and does NOT add rows to the ALL literal (it reclassifies existing decorator() rows), so it sidesteps the const-array-split hazard that SPEC-01/04/11 hit. Lowest-risk registry interaction of any spec in the set.",
          "severity": "minor"
        },
        {
          "spec": "SPEC-10",
          "gap_type": "credible-split-plan",
          "detail": "No blocking LOC risk and the plan is unusually careful. examples_bundle.rs is verified at 401; the plan PRE-EMPTIVELY extracts the validation helper into examples_bundle/validate.rs (mod.rs re-exports) before adding ~25 LOC, with explicit ABI-additive pub use restoration named — correct anticipation. group_02 test file is verified at 423; the +list-swap is net-neutral with a named sibling-split hatch (group_02_examples_tests.rs) if it crosses 500. The two new doctor rule files (~120/150) and the new integration test examples_doctor_clean.rs (~180, with the tests/<name>/main.rs+mod helpers conversion named if >500) are all credibly bounded. Note: most of SPEC-10's surface is .lzi/.toml/.json content (no LOC ceiling) — the .rs footprint is small and well-planned.",
          "severity": "minor"
        },
        {
          "spec": "GLOBAL",
          "gap_type": "baseline-invariant-already-violated",
          "detail": "The premise every loc_plan is judged against — CLAUDE.md:250 'every .rs file under crates/ <= 500 LOC ... No exceptions ... 0 above 500' — is ALREADY FALSE in the current tree. A workspace scan finds ~40 .rs files over 500 LOC, including registry.rs (3601), handler_signature_mismatch_001.rs (1362), handler_sql_column_drift_001.rs (1162), rule_bridges.rs (1072), doctor_config/lib.rs (1012), and many 500-768 LOC files (resource/mod.rs 597, resource/field.rs 745, view_guard.rs 768, lzx.rs 607, lib_tests files, etc.). This means: (1) the specs that touch ALREADY-over files (SPEC-02 ux.rs 747, SPEC-04/06/11 registry.rs 3601, SPEC-09 resource/mod.rs 597, SPEC-08 none-over, SPEC-05 view_guard.rs 768 is touched in-place) inherit a pre-existing overage their loc_plan must not make worse — SPEC-02 and SPEC-09 explicitly remediate; SPEC-06 explicitly refuses (see SPEC-06 gap). (2) Any acceptance criterion of the form 'every touched .rs is <=500 LOC' is UNACHIEVABLE for specs editing registry.rs in-place (SPEC-01/04/06/11) unless they actually land the registry split, which only SPEC-04 commits to as a prerequisite. The cross-spec dependency is implicit and unsequenced: SPEC-04's registry split must land before SPEC-01/06/11 can honestly claim their <=500 acceptance criterion on registry.rs, but no spec declares this ordering.",
          "severity": "blocker"
        }
      ]
    },
    {
      "verdict": "The DAG's dependency reasoning is largely sound and the high-risk fixture/registry collisions are correctly identified in the risks[] narrative — but the machine-readable structure has THREE blocker-class defects that an orchestrator consuming the waves[] array would act on incorrectly, plus one missing serialization edge for a verified shared-line collision.\\n\\nBLOCKERS (must fix before dispatch):\\n1. SPEC-13 is double-placed (appears in both wave 2 AND wave 4); SPEC-17 is omitted from every wave. Root cause: the wave-4 specs array reads [SPEC-05, SPEC-07, SPEC-13] but the wave-4 rationale describes SPEC-17. Correct wave 4 = [SPEC-05, SPEC-07, SPEC-17]. This is a copy/paste error that simultaneously double-runs SPEC-13 and drops SPEC-17.\\n2. SPEC-04 / SPEC-08 share a single line (full-capsule.lzi:562 `forbids output contains @semantic.Email`) yet have no dependency edge and sit in different waves (08 in parallel wave 2, 04 in wave 3). Verified by grep. Add SPEC-08 depends_on SPEC-04 (or fold SPEC-08 into the wave-4 full-capsule serialized chain).\\n\\nMAJOR: SPEC-06/08/09 concurrently regenerate the shared generated artifacts (tmLanguage.json, keyword-reference.md) in wave 2 with no edge enforcing the single-regen reconciliation the rationale mandates — encode it, do not leave it as prose.\\n\\nCORRECTLY PARTITIONED (verified): SPEC-01 is the right root and right to run solo (registry.rs confirmed 3601 LOC, over ceiling — split is a real prereq). The SPEC-01->SPEC-03->SPEC-04 types.rs three-way overlap is correctly serialized across waves 1/2/3. The SPEC-04->SPEC-05->SPEC-07 full-capsule chain (17 sigil + 12 predicate + N policy sites all confirmed on full-capsule.lzi) is correctly sequential. SPEC-10b/SPEC-11 are correctly bookended last with their real blockers (DEFAULT_TEMPLATE include_str! of examples/crm.lzi confirmed at main.rs:101) gated. The minor grammar.app.md docs race (SPEC-12/SPEC-15) is hand-mergeable and acceptable with section partitioning.\\n\\nNet: the WAVE-LEVEL serialization philosophy is correct and conservative; the failures are (a) two array-encoding bugs that break the partition, (b) one un-encoded shared-line edge between SPEC-04 and SPEC-08, and (c) generated-artifact regen left implicit. Fix the three blockers and add the SPEC-08->SPEC-04 edge and the plan is dispatchable.",
      "gaps": [
        {
          "spec": "SPEC-04 / SPEC-08",
          "gap_type": "missing-dependency-edge / fixture-collision",
          "detail": "SHARED-FILE COLLISION the DAG fails to serialize. SPEC-08 is in wave 2 (parallel, depends_on:[]); SPEC-04 is in wave 3 (depends_on:[SPEC-01]). There is NO edge between them, yet they rewrite the SAME LINE of examples/full-capsule/full-capsule.lzi. Verified: line 562 reads `forbids output contains @semantic.Email` — SPEC-08 rewrites `forbids`->`denies` and SPEC-04 rewrites `@semantic.Email`->`Email` on that one line. Per the project's git discipline ('parallel agents share a .git/index ... broad-stage commands sweep siblings' staged work'), SPEC-08 authoring its full-capsule.lzx + full-capsule.lzi:562 edit in a wave-2 worktree while SPEC-04 has not yet run (wave 3) guarantees a merge/rebase conflict on that line and risks one agent's edit clobbering the other. The risks[] narrative acknowledges SPEC-08 'rewrites the SAME canonical capsule' but the DAG does NOT encode an edge, so an orchestrator reading the machine-readable dag will dispatch them concurrently. FIX: add SPEC-08 depends_on SPEC-04 (or move SPEC-08 into the wave-4 serialized full-capsule chain after SPEC-04), so the shared-line edits are ordered.",
          "severity": "blocker"
        },
        {
          "spec": "SPEC-09 / SPEC-04 / SPEC-05",
          "gap_type": "missing-dependency-edge / fixture-collision",
          "detail": "SPEC-09 (depends_on:[], wave 2 parallel) migrates the `: many <Type>` collection form and its in-repo migration of examples is scoped to examples/linear-issue.lzi:42 — VERIFIED there is no `: many ` site in full-capsule.lzi, so SPEC-09's full-capsule contention is LOWER than the risks[] text claims (the risk note says SPEC-09 rewrites `labels: many Label`->`has_many` in full-capsule; grep shows that site is in linear-issue.lzi, not full-capsule). HOWEVER SPEC-09 still touches the shared registry.rs (removes the `many` type-ctor row) and regenerates tmLanguage + keyword-reference.md — the same generated artifacts SPEC-04/06/08 mutate. The DAG puts SPEC-09 in wave 2 concurrent with SPEC-06 and SPEC-08, all three regenerating the same two generated files. The wave-2 rationale's CAVEAT correctly says the xtask regen must be sequenced at integration, but the DAG provides no edge forcing it; concurrent `gen-tmlanguage`/`gen-keyword-reference` on a shared worktree will produce a generated-file merge race. FIX: either give SPEC-06/08/09 a shared-artifact serialization edge, or mandate (in the wave spec, not just prose) a single post-wave-2 regen reconciliation step owned by one agent.",
          "severity": "major"
        },
        {
          "spec": "SPEC-13",
          "gap_type": "wave-partition-violation",
          "detail": "SPEC-13 appears in TWO waves. waves[1] (wave 2) specs array includes \"SPEC-13\"; waves[3] (wave 4) specs array is [\"SPEC-05\",\"SPEC-07\",\"SPEC-13\"]. A spec must appear in exactly one wave. This is a hard partition error: an orchestrator iterating the waves array would dispatch SPEC-13 twice. The wave-4 RATIONALE text actually describes SPEC-17 (\"SPEC-17 (enum metadata colon-form grammar) depends on SPEC-13 ... Order within the wave: SPEC-05 -> SPEC-07 -> SPEC-17\") — so the wave-4 specs array is a copy/paste error: it should read SPEC-17, not SPEC-13. FIX: wave 4 specs = [SPEC-05, SPEC-07, SPEC-17]; remove SPEC-13 from wave 4 (it correctly lives in wave 2).",
          "severity": "blocker"
        },
        {
          "spec": "SPEC-17",
          "gap_type": "spec-omitted-from-waves",
          "detail": "SPEC-17 (depends_on:[SPEC-13]) is present in the dag[] but appears in NO waves[].specs array. The wave-4 rationale discusses SPEC-17 at length and orders it last in the wave ('SPEC-05 -> SPEC-07 -> SPEC-17'), but the wave-4 specs membership array lists SPEC-13 in its place. Consequence: SPEC-17 would never be dispatched by an orchestrator that consumes waves[].specs. This is the mirror of the SPEC-13 double-placement bug — fixing the wave-4 array to [SPEC-05, SPEC-07, SPEC-17] resolves both. Confirmed SPEC-17's only dependency (SPEC-13) lands in wave 2, so a wave-4 placement is dependency-valid.",
          "severity": "blocker"
        },
        {
          "spec": "SPEC-01 (root)",
          "gap_type": "root-correctness",
          "detail": "SPEC-01 IS correctly the root and correctly runs alone in wave 1. Verified: it single-sources the closed catalogs in lazuli_keywords and is the only consumer-blocking prerequisite for SPEC-04 (its SemanticScalar rows are SPEC-04's retirement destination) and SPEC-11 (the generation mechanism). registry.rs is confirmed at 3601 LOC (over the 500 ceiling), types.rs at 452 LOC, lib.rs at 542 LOC — the registry split SPEC-01 must land before downstream registry-mutating specs (SPEC-04/06/08/09) fork is a genuine structural prerequisite, correctly sequenced. No gap with the root; recorded as a positive confirmation. One minor note: SPEC-02 and SPEC-03 declare parallelizable_with SPEC-01 in their bodies, but the DAG correctly does NOT make them depend on SPEC-01 and places them in wave 2 (after SPEC-01's wave-1 solo run), which is the safe conservative call given SPEC-01 mutates types.rs that SPEC-03 also edits.",
          "severity": "minor"
        },
        {
          "spec": "SPEC-01 / SPEC-03 / SPEC-04",
          "gap_type": "three-way-shared-file",
          "detail": "types.rs three-way overlap is CONFIRMED and CORRECTLY SERIALIZED by the DAG. Verified crates/lazuli_analyzer/src/types.rs (452 LOC) carries: the silent alias arms (lines 160-177: String/Bool/Int/Float/Json/Id) that SPEC-03 removes, AND the @semantic.*/@cap.* arms (lines 139-158, 191-204) that SPEC-04 deletes, AND the scalar membership SPEC-01 re-roots. The DAG orders SPEC-01 (wave 1) -> SPEC-03 (wave 2) -> SPEC-04 (wave 3) so each consumes the prior's mechanism — this is correct. The ONLY residual risk: SPEC-03 (wave 2) and SPEC-04 (wave 3) are in different waves so they will not run concurrently, which is the right call; the risks[] note correctly warns 'if SPEC-03 and SPEC-04 are accidentally parallelized they will produce conflicting match-arm edits.' No edge is missing here — recording as confirmation that the conservative wave separation (not just the dag edge) is load-bearing and must be preserved.",
          "severity": "minor"
        },
        {
          "spec": "SPEC-05 / SPEC-07",
          "gap_type": "sequential-correctness",
          "detail": "SPEC-04/05/07 sequential chain is CORRECT. Verified the shared-fixture churn: full-capsule.lzi has 17 @semantic./@cap. sites (SPEC-04), 12 `self.x =` predicate sites (SPEC-05), and @policy.* policy-category sites at lines 89/101/111/121/122/210/384/405/425/451+ (SPEC-07). All three converge on full-capsule.lzi. The DAG sequences SPEC-04 (wave 3) -> SPEC-05+SPEC-07 (wave 4 sequential) correctly per the git shared-index discipline. SPEC-05 depends_on SPEC-04 and SPEC-07 depends_on SPEC-04 are both present in the dag. Within wave 4 the rationale orders SPEC-05 -> SPEC-07 -> SPEC-17 sequentially, which is the safe call. This partition is sound. Minor: line 122 `requires @policy.delete` is an eval/policy `requires` site — confirm SPEC-07's policy rename and SPEC-08's eval-verb retirement do not both claim it (SPEC-08 forbids/requires retirement is scope-guarded to agent evals; line 122 is inside a command policy block, not an eval case, so they are disjoint — but the scope-guard must hold).",
          "severity": "minor"
        },
        {
          "spec": "SPEC-10 / SPEC-11",
          "gap_type": "bookend-correctness",
          "detail": "SPEC-10b/SPEC-11-final correctly placed LAST in wave 6. Verified SPEC-10's hard blocker: crates/lazuli_cli/src/main.rs:101 `const DEFAULT_TEMPLATE: &str = include_str!(\"../../../examples/crm.lzi\")` — confirmed the DEFAULT_TEMPLATE re-point off examples/crm.lzi is a real include_str! coupling that must be severed before crm.lzi is touched. SPEC-10 Phase B correctly depends on ALL syntax cuts (authoring curated examples in pre-cut syntax would bake in retired spellings) and SPEC-11 correctly depends on SPEC-01 (generation mechanism) + must reconcile CLAUDE.md/AGENTS.md last. Both bookends are dependency-correct. The pull-forward optimizations (SPEC-10 Phase A deletion/re-point into wave 2, SPEC-11 tone/layer-split half mid-campaign) are sound and correctly flagged as orthogonal to keyword changes. CAVEAT recorded: SPEC-10 and SPEC-11 BOTH edit CLAUDE.md/AGENTS.md and docs/* and examples/ canon, so they must be serialized against each other (the wave-6 'sequential' mode is correct) — do not parallelize them even though SPEC-10 Phase B and SPEC-11 are otherwise independent.",
          "severity": "minor"
        },
        {
          "spec": "SPEC-12 / SPEC-15",
          "gap_type": "docs-merge-race",
          "detail": "SPEC-12 and SPEC-15 are both in wave 2 parallel with disjoint CODE surfaces, but the risks[] note correctly identifies that both extend docs/grammar.app.md (SPEC-15: cookie/proxy/limits/headers; SPEC-12: locale/cors/logging/tracing/encryption sections 10+). The DAG treats them as fully disjoint islands (no edge) but they share one markdown file. This is a lower-severity version of the generated-artifact race: two agents appending to the same docs/grammar.app.md section region in separate worktrees will conflict at merge. FIX (or accept-with-mitigation): partition grammar.app.md edits by section, or sequence SPEC-12 and SPEC-15's grammar.app.md hunk at integration. Not a blocker because grammar.app.md is curated prose (hand-mergeable, unlike generated files which must never be hand-merged), but it should be called out so the orchestrator does not assume zero contention.",
          "severity": "minor"
        },
        {
          "spec": "SPEC-04",
          "gap_type": "parallelizable-with-inconsistency",
          "detail": "Minor metadata inconsistency: SPEC-04's body declares parallelizable_with:[SPEC-02], and SPEC-02 is in wave 2 while SPEC-04 is in wave 3 — so they never actually run in the same wave, making the parallelizable_with hint moot (harmless but misleading). Similarly SPEC-01 body lists parallelizable_with:[SPEC-02,SPEC-03] but the DAG runs SPEC-01 solo in wave 1. These are advisory fields that the wave partition overrides conservatively (correctly, given the shared types.rs/registry.rs churn). No action required beyond noting the spec-body parallelism hints are superseded by the safer wave assignment; the orchestrator should follow the waves, not the per-spec parallelizable_with.",
          "severity": "minor"
        }
      ]
    },
    {
      "verdict": "NOT pristine — the spec set has real cross-spec incoherence that would surface as build-breaks and doctrine drift if executed as written. On the specific questions asked: (1) SPEC-04 does NOT leave @policy/@role/@pii/@key with a fully coherent, documented sigil doctrine. It only resolves @semantic/@cap (type position). @pii/@key are carved out as classification-markers that keep the sigil, but SPEC-04 introduces a same-concept-two-faces split (bare PII(class:..) capability-type vs sigiled @pii field-marker) that no spec justifies, and it DEFERS the @key-inside-Encrypted decision to SPEC-03, which does not address @key at all (a dangling, unresolvable coordination reference — blocker). (2) SPEC-07 aligns with SPEC-04's '@=named-ref, bare=catalog/type' line for @policy (correct: named reference keeps @), but CONTRADICTS the same line for @role/@scope/@actor: SPEC-04 drops the sigil from closed-catalog semantic types precisely BECAUSE they are closed catalogs, while SPEC-07 keeps the sigil on closed-catalog authz atoms — opposite conclusions from the identical 'this is a closed catalog' premise, with no spec stating the reconciling axis principle. (3) SPEC-01's catalog classification anticipates SPEC-04's @semantic removal cleanly (SemanticScalar bare rows pre-built as the destination) but does NOT anticipate the @cap side (no CapabilityType CatalogKind variant / no bare File/Encrypted/Hashed/Token/Secret/PII rows), and SPEC-01's pinned 23-namespace snapshot test sits exactly on the boundary SPEC-04 mutates, where the two specs describe @semantic/@cap's catalog membership inconsistently (TypeDecorator per SPEC-01 vs reference-namespace-allowlist entries per SPEC-04's vocab.rs edits). (4) YES, a unifying 'sigil doctrine' spec is missing: the '@=named-reference, bare=closed-catalog/type' line is stated as inline prose in TWO specs (04 and 07) with two scopes and never codified as a normative single-owner artifact; the end-state @-sigil still carries FOUR distinct kinds (named-ref, plugin-scalar, classification-marker, catalog-atom), which is the very polysemy SPEC-04's problem statement set out to eliminate — net straddle is reduced by one, not resolved. Additionally there are two execution-blocking incoherences independent of the sigil question: SPEC-01 and SPEC-03 both own the scalar-alias problem with CONTRADICTORY lowering semantics (UserDefined hard-break vs lower-to-builtin-plus-warn) and different rule codes, editing the same types.rs lines in parallel; and SPEC-11 (the campaign keystone that reconciles CLAUDE.md and regenerates the authoritative catalog docs) under-declares its dependencies (only SPEC-01) when it must run after every catalog-mutating spec (04,05,07,08,09) to render the correct final taxonomy. These are not polish items — the SPEC-01/SPEC-03 contradiction and the SPEC-04 @key dangling-reference are blockers; the missing unifying doctrine, the SPEC-04/SPEC-07 closed-catalog contradiction, and the SPEC-11 dependency gap are majors.",
      "gaps": [
        {
          "spec": "SPEC-04 + SPEC-01 + SPEC-07",
          "gap_type": "missing-unifying-spec",
          "detail": "There is NO standalone 'sigil doctrine' spec, and the doctrine that SPEC-04 leans on ('@ = named-reference; bare = closed-catalog/type') is asserted only as inline prose inside SPEC-04's end_state, never codified as a normative artifact (doc + doctor rule) that the other specs reference. SPEC-04 states the rule for @semantic/@cap ('bare name => core type; @semantic.<Alias> => plugin type; @cap deleted'), and SPEC-07 independently states its own rule for @policy ('@ = named reference, retained per SPEC-04's doctrine'). But the line is articulated TWICE, in two different specs, with two different scopes, and no single spec owns 'what does @ mean across ALL of @policy/@role/@scope/@actor/@pii/@key/@semantic/@cap/@fn/@anchor'. design-decisions.md:131-160 (axis-per-namespace) is the closest thing to a unifying doctrine and it is being EDITED piecemeal by SPEC-04 (remove @cap, annotate @semantic plugin-only) and SPEC-07 (split @role/@scope/@actor into a catalog-atom kind) — but neither spec produces a single coherent restatement of the post-campaign @-sigil taxonomy. The reader is left to assemble the final doctrine from fragments in SPEC-01 (CatalogKind), SPEC-04 (type-sigil retirement), SPEC-07 (catalog-atom-vs-reference kind), and SPEC-11 (catalog-reference.md generation). This is a genuine coherence gap: the campaign's single most-cited closed surface (the @ namespace) ends with its meaning distributed across four specs and no authoritative consolidation until SPEC-11's docs reorg — which depends only on SPEC-01, NOT on SPEC-04 or SPEC-07, so SPEC-11 may regenerate catalog-reference.md WITHOUT the @cap removal / @semantic re-scope / catalog-atom split having been reflected in design-decisions.md if ordering slips.",
          "severity": "major"
        },
        {
          "spec": "SPEC-04",
          "gap_type": "incoherent-doctrine-for-@pii/@key",
          "detail": "The critic's framing asks whether SPEC-04 leaves @policy/@role/@pii/@key with a COHERENT documented sigil doctrine. It does NOT for @pii and @key. SPEC-04 only addresses @semantic and @cap (the two TYPE-position sigils). It explicitly carves out @pii and @key by treating them as classification/field-markers that keep the sigil — but note the SUBSTANTIVE INCONSISTENCY it introduces: SPEC-04's end_state lists 'PII(class:contact)' and 'Encrypted(key:Key.tenant)' as BARE PascalCase reserved CAPABILITY types, meaning the @cap.PII form (a data-classification CAPABILITY) becomes bare PII(...), while the @pii.contact field-marker DECORATOR (registry.rs:3190 decorator('@pii','PII-classification decorator.')) stays @pii. So after SPEC-04 the token 'PII' appears bare in type position AND '@pii' appears sigiled in field-marker position — the SAME concept (PII classification) wears two faces depending on whether it's a capability-type or a field-marker, with no spec stating that this split is intentional or how an author/LLM disambiguates. Worse, the @key ref inside Encrypted is left genuinely UNDECIDED: SPEC-04's end_state hedges 'the @key. ref inside Encrypted/E2ee becomes the bare Key.tenant (or stays @key.tenant ONLY if SPEC-03 keeps @key as a classification sigil — see coordination)' — but SPEC-03 is the SCALAR-ALIAS spec and says NOTHING about @key. There is no spec that decides @key's fate. This is a dangling, unresolvable coordination reference: SPEC-04 defers the @key decision to a spec (SPEC-03) that does not address it.",
          "severity": "blocker"
        },
        {
          "spec": "SPEC-01 vs SPEC-04",
          "gap_type": "classification-contradiction",
          "detail": "SPEC-01's CatalogKind enum classifies @semantic and @cap as 'TypeDecorator (@semantic, @cap)' and adds 'SemanticScalar (Email/Phone/Url/Uuid/Currency/GeoPoint/HexColor/Percentage)' rows as the bare destination. SPEC-01 explicitly claims forward-compatibility with SPEC-04: 'SemanticScalar rows already exist as the destination so @ off types is a row-retire of the @-forms, not a re-architecture.' This is PARTIALLY coherent but has a real seam. SPEC-01 also classifies the reference-namespace set as 23 entries INCLUDING the @-decorators it keeps, and its catalog_classification.rs test PINS 'ReferenceNamespace set == the 23 the LSP previously hardcoded (pinned snapshot so a silent shrink fails).' SPEC-04 then DELETES @cap from is_allowed_reference_namespace (vocab.rs) — i.e. it shrinks the namespace set by removing 'cap'. But SPEC-01's pinned snapshot test will GO RED on exactly that removal, because the snapshot was frozen at the historical 23 (which, per the actual code at vocab.rs:215-267, does NOT contain 'cap' as a reference-namespace — 'cap' is a TypeDecorator, while the 23-set the spec enumerates also doesn't list cap/semantic as ReferenceNamespace). The classification boundary between TypeDecorator and ReferenceNamespace is where SPEC-01 and SPEC-04 must agree byte-for-byte, and the specs describe it inconsistently: SPEC-01 says @semantic/@cap are TypeDecorator (not ReferenceNamespace), yet SPEC-04 says it must 'KEEP semantic (222) but its meaning narrows' in is_allowed_reference_namespace and 'REMOVE cap (line 223)' — treating BOTH as reference-namespaces in vocab.rs. So @semantic/@cap are simultaneously TypeDecorator (SPEC-01 registry classification) and entries in the reference-namespace allowlist (SPEC-04 vocab.rs). The two specs do not agree on which catalog @semantic/@cap belong to, and SPEC-01's pinned-snapshot test is the exact gate that will surface the contradiction at build time.",
          "severity": "major"
        },
        {
          "spec": "SPEC-01",
          "gap_type": "catalog-anticipates-spec-04-only-half",
          "detail": "The critic asks whether SPEC-01's catalog classification anticipates SPEC-04 removing @semantic/@cap decorators. It anticipates the SEMANTIC side cleanly (SemanticScalar bare rows are the pre-built destination, explicitly called out) but does NOT anticipate the @cap side. SPEC-01 enumerates CatalogKind variants and lists 'TypeDecorator (@semantic, @cap)' and 'SemanticScalar (Email...Percentage)' but provides NO 'CapabilityType' / bare-cap-scalar destination rows for File/PII/Hashed/Encrypted/E2ee/Token/Secret. SPEC-04 needs bare Encrypted/Hashed/E2ee/Token/Secret/File/PII as reserved type names (it adds them to the analyzer collision-guard and reserved-name policy), but SPEC-01's registry rows only cover semantic scalars, not capability types. So when SPEC-04 lands, the capability bare-names have NO registry row to derive from — SPEC-04 must add them itself (its parity_surfaces do say 'ADD bare semantic+cap scalars as Type-token closed-catalog entries'), but that means SPEC-01's CatalogKind enum is missing a 'CapabilityType'/'CapabilityScalar' variant that SPEC-04 will need. SPEC-04 declares dependency on SPEC-01, so it inherits CatalogKind, but CatalogKind as specified in SPEC-01 has no variant for the six capability types — SPEC-04 would have to EXTEND the enum (fine, it's #[non_exhaustive]) but neither spec names this extension, leaving the capability-type classification a gap between the two.",
          "severity": "major"
        },
        {
          "spec": "SPEC-07",
          "gap_type": "alignment-with-spec-04-partial",
          "detail": "Does SPEC-07 align with SPEC-04's doctrine? Mostly yes, but with one unaddressed inconsistency in the catalog-atom reclassification. SPEC-07 reclassifies @role/@scope/@actor from generic decorator() to 'a distinct catalog-atom kind' in the registry, and KEEPS @policy as a reference-namespace, explicitly citing SPEC-04's '@=named-ref, bare=catalog/type' line. This is coherent for @policy (a named reference to a user-declared category => @ is right) and coherent in SPIRIT for @role/@scope/@actor (closed catalog atoms). BUT SPEC-04's doctrine as stated is 'bare = closed-catalog/type' — and @role/@scope/@actor ARE closed catalogs (e.g. @scope.authenticated, @role.admin). By SPEC-04's own rule, a CLOSED-CATALOG atom should be BARE (like Text, like Email post-SPEC-04), not sigiled. SPEC-07 instead keeps @role/@scope/@actor SIGILED while merely renaming their registry KIND from decorator to catalog-atom. This is a DIRECT tension with SPEC-04's doctrine: SPEC-04 makes the closed-catalog semantic types bare BECAUSE they are closed catalogs ('collision-safe-by-reservation exactly like Text'), but SPEC-07 keeps the closed-catalog authz atoms sigiled. The two specs apply OPPOSITE conclusions to the same premise ('this is a closed catalog'): SPEC-04 says closed-catalog => drop the sigil; SPEC-07 says closed-catalog (authz) => keep the sigil but rename its kind. Neither spec reconciles why @scope.authenticated stays sigiled while @semantic.Email loses its sigil, when both are closed catalogs. The 'axis' rationale (these are authz, type position is unambiguous) is plausible but is NOT stated as the reconciling principle in either spec — it's exactly the unifying-doctrine spec that's missing.",
          "severity": "major"
        },
        {
          "spec": "SPEC-04 + SPEC-05",
          "gap_type": "dependency-ordering-coherence",
          "detail": "SPEC-05 (== for equality) depends on SPEC-04, and SPEC-04 depends on SPEC-01. SPEC-05's stated reason for depending on SPEC-04 is not given explicitly in its dependencies field (it just lists ['SPEC-04']). The substantive coupling: SPEC-05's agent-eval predicate migration and the full-capsule rewrite touch the SAME fixture lines SPEC-04 rewrites (full-capsule.lzi predicate sites vs type sites). SPEC-04's agent-eval predicate 'contains @semantic.Email' => 'contains Email' migration (evals.rs:275-283) and SPEC-05's '=' => '==' predicate migration both edit the eval/predicate surface and both rewrite full-capsule. This is correctly serialized by the dependency chain, but SPEC-08 ALSO migrates eval verbs (requires/forbids => allows/denies) in the SAME evals block, and SPEC-08 declares NO dependency on SPEC-04 or SPEC-05 and lists itself parallelizable_with specs touching 'unrelated keyword families.' SPEC-08 rewrites examples/full-capsule eval cases ('forbids output contains' => 'denies output contains') while SPEC-04 rewrites the same cases' 'contains @semantic.Email' => 'contains Email' and SPEC-05 rewrites any '=' in those predicates. Three specs edit the full-capsule eval block with no declared mutual ordering (SPEC-08 is floating). Per the repo's own git-discipline ('sequential > parallel on a shared worktree', shared-file contention), this is an unflagged coherence gap: SPEC-08 should declare a sequencing constraint with SPEC-04/SPEC-05 on the shared full-capsule eval fixture, and does not.",
          "severity": "minor"
        },
        {
          "spec": "SPEC-01 vs SPEC-03",
          "gap_type": "duplicate-overlapping-spec",
          "detail": "SPEC-01 and SPEC-03 BOTH define a doctor rule for scalar aliases with overlapping-but-not-identical scope, and BOTH name it inconsistently. SPEC-01 defines 'VOCAB-SCALAR-ALIAS-001' (flags String->Text, Bool->Boolean, Int->Integer, Float->Decimal, Json->JSON, Id->ID, bare Email->@semantic.Email). SPEC-03 defines 'SCALAR-ALIAS-001' (Id->ID, String->Text, Bool->Boolean, Int->Integer, Float->Decimal, json/Json->JSON) — note SPEC-03 does NOT include the bare-Email alias that SPEC-01 does, and SPEC-03 uses a DIFFERENT code (SCALAR-ALIAS-001 vs VOCAB-SCALAR-ALIAS-001) and a DIFFERENT remediation philosophy (SPEC-01: 'reject, alias arms REMOVED, fall through to UserDefined'; SPEC-03: 'reject + autocorrect, lowers to canonical builtin BUT records a diagnostic, does NOT become UserDefined'). These two specs are in DIRECT CONTRADICTION on the core lowering behavior: SPEC-01 says aliases become UserDefined (hard break, type unknown); SPEC-03 says aliases still lower to the canonical builtin AND warn (soft, autocorrected). Both are marked parallelizable_with each other (SPEC-01 lists SPEC-03 implicitly via SPEC-02/03; SPEC-03 lists SPEC-02). They edit the SAME file (crates/lazuli_analyzer/src/types.rs:160-178 alias arms) with INCOMPATIBLE end states. This is a blocker-class contradiction: the campaign contains two specs that both own the scalar-alias problem, resolve it differently, and are scheduled to run in parallel against the same lines.",
          "severity": "blocker"
        },
        {
          "spec": "SPEC-04 + SPEC-07",
          "gap_type": "@semantic-meaning-collision",
          "detail": "SPEC-04 re-scopes @semantic to 'ONLY open plugin-declared aliases' and adds doctor rule VOCAB-SEMANTIC-PLUGIN-SIGIL-001 to guard that surviving meaning. SPEC-07 keeps @policy as a reference-namespace and reclassifies @role/@scope/@actor as catalog-atoms, citing SPEC-04. The collision: after SPEC-04, the @-sigil has THREE surviving meanings (named-reference like @policy/@fn/@anchor; plugin-scalar like @semantic.BrazilianCPF; classification-marker like @pii/@key) PLUS SPEC-07's new fourth (catalog-atom like @role/@scope/@actor). That is FOUR distinct semantic kinds for @, after a campaign whose entire justification (SPEC-04 problem statement) was that 'the sigil makes @ straddle four unrelated semantic kinds.' SPEC-04 reduces the type-straddle by ONE (removes @cap, makes types bare) but SPEC-07 ADDS a formally-distinct catalog-atom kind, and @pii/@key/@semantic-plugin remain. So the campaign's net effect on the @-straddle the specs set out to fix is roughly neutral-to-slightly-reduced, NOT resolved — and no spec acknowledges that @ still carries 4 kinds at the end. The 'coherent documented sigil doctrine' the critic asks about is therefore NOT achieved: the end-state doctrine is '@ means one of {named-ref, plugin-scalar, classification-marker, catalog-atom} disambiguated by namespace token,' which is exactly the polysemy SPEC-04 opened by criticizing.",
          "severity": "major"
        },
        {
          "spec": "SPEC-04",
          "gap_type": "reserved-name-collision-scope",
          "detail": "SPEC-04 makes Money/Email/Encrypted/PII/Token/Secret/etc bare reserved type names and adds VOCAB-TYPE-RESERVED-COLLISION-001 to reject a user 'resource Money'. But it does not reconcile with SPEC-09 (has_many) or the existing TYPE_CTOR rows (many/list_of/ref). More importantly, 'Secret' and 'Token' as bare reserved names collide with extremely common user-domain nouns (a fintech app modeling Token, an auth app modeling Secret/Session). SPEC-04 treats this as a feature (collision-safety guarantee) but does not flag the DX cost or coordinate with SPEC-10's curated examples, several of which (auth, billing) are precisely the domains most likely to want 'Token'/'Secret' as resource names. SPEC-10 authors curated examples in the FINAL surface and depends on SPEC-04, so a curated auth example wanting a 'Token' resource would now hard-error — an interaction neither spec surfaces.",
          "severity": "minor"
        },
        {
          "spec": "SPEC-11",
          "gap_type": "dependency-graph-incoherence",
          "detail": "SPEC-11 (docs reorg + catalog-reference.md generation + CLAUDE.md reconciliation) declares dependency ONLY on SPEC-01. But SPEC-11's explicit job is to regenerate the closed-catalog docs and reconcile CLAUDE.md/AGENTS.md to 'reflect every other SPEC's final canonical forms' and name the post-campaign namespace taxonomy. It CANNOT correctly do that with only SPEC-01 as a dependency: the @cap removal (SPEC-04), the @semantic re-scope (SPEC-04), the catalog-atom kind split (SPEC-07), the predicate == operator (SPEC-05), the test-verb folds (SPEC-08), and the many-retirement (SPEC-09) all change the catalogs/keywords that catalog-reference.md and the design-decisions namespace table must render. SPEC-11's own end_state says design-decisions.md:144-149 namespace table 'gains the 5 missing namespaces or is replaced by a generated include' — but if SPEC-04 has removed @cap by the time SPEC-11 runs, the generated include must NOT list @cap, which only works if SPEC-11 runs AFTER SPEC-04. With dependency declared solely on SPEC-01, SPEC-11 could legally run before SPEC-04/07, baking the pre-cut taxonomy into the 'authoritative' generated docs. This is the campaign-keystone spec under-declaring its dependencies — it should depend on the full set of keyword/catalog-mutating specs (01,04,05,07,08,09), not just 01.",
          "severity": "major"
        }
      ]
    }
  ]
}
```
