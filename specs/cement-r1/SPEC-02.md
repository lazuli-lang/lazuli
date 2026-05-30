# SPEC-02 — Retire the section 7a surface dialect (braces/semicolons/Int/@command) into canonical indentation

**Breaking:** True | **Est. commits:** 9 | **Depends on:** none | **Parallel with:** SPEC-01 if it touches .lzi resource/field grammar (no overlap with .lzx surface ux.rs), Any SPEC touching .lzi-only surfaces (commands, queries, resources) since this SPEC is confined to .lzx surface UX primitives + their registry/doctor/scaffold faces, NOT parallelizable with any SPEC that also edits crates/lazuli_keywords/src/registry.rs SurfaceView entries, surface/ux.rs, experience/audience.rs, or regenerates tmLanguage/keyword-reference (shared generated artifacts + shared parser files cause index/merge contention)

## Problem
The section 7a view-level UX primitives are the ONLY place in the language using structural braces and semicolons, the non-canonical scalar alias Int, and the @command sigil where commands are referenced bare everywhere else. Four grounded violations: (1) BRACE+SEMICOLON: `repeatable input <name> group { <f>: <T>; ... } validates sum(<f>) = <n>` — EBNF docs/grammar.lzx.md:414-418; parser crates/lazuli_syntax/src/parser/lzx/surface/ux.rs:393-536 does starts_with open-brace (ux.rs:426), find close-brace (ux.rs:432), body.split semicolon (ux.rs:442); shipped+tested at docs/grammar.lzx.md:438, lzx_parser_tests.rs:719, surface_tests/ux.rs:239; violates docs/grammar.lzi.md:1166-1168 (admits the legacy brace-MVP shapes nowhere). (2) NON-CANONICAL Int: fixtures use days: Int but the closed catalog is ID|Text|Boolean|Integer|Decimal|Date|DateTime|JSON (docs/grammar.lzi.md:37-38,274); brace body type is opaque text (ux.rs:451), no catalog check, Int flows to codegen verbatim (crates/lazuli_codegen_ts/src/lzx_ux.rs:135-141). (3) @command SIGIL: view.inline_table on_change @command.<name> requires the prefix (ux.rs:90-127, rejects bare at ux.rs:109-114; decorator registry.rs:3218), yet commands are bare everywhere else and the analyzer already strips @command at crates/lazuli_analyzer/src/surface/list_decls.rs:184-191. (4) DOTTED KEYWORDS: view.board/view.inline_table (registry.rs:2947,2953) are dotted, unlike every other bare view child; dispatch surface/view.rs:248,263,276 and experience/audience.rs:230-238. Dual-parsed: both dialects call the SAME shared parse functions in surface/ux.rs, so the brace/sigil/Int forms leak into the blessed experience dialect. Violates one-canonical-way (docs/architecture.md:260-261). docs/design-decisions.md read end-to-end (lines 32-497) — NO entry blesses this shape; it is unmigrated MVP residue.

## Evidence
- Brace+semicolon shape is unique; parser splits on braces and semicolons — `crates/lazuli_syntax/src/parser/lzx/surface/ux.rs:426,432,442`
- EBNF documents the brace/semicolon repeatable form — `docs/grammar.lzx.md:414-418`
- Shipped brace form with Int alias is documented and tested — `docs/grammar.lzx.md:438; crates/lazuli_syntax/src/parser/lzx/lzx_parser_tests.rs:719; crates/lazuli_syntax/src/parser/lzx/surface_tests/ux.rs:239`
- Closed scalar catalog excludes Int (canonical is Integer) — `docs/grammar.lzi.md:37-38,274`
- @command sigil required by inline_table parser but stripped downstream — `crates/lazuli_syntax/src/parser/lzx/surface/ux.rs:109-115; crates/lazuli_analyzer/src/surface/list_decls.rs:184-191`
- @command decorator and dotted view keywords live in the registry — `crates/lazuli_keywords/src/registry.rs:3218,2947,2953`
- Both dialects share the parse functions, leaking the brace form into the blessed dialect — `crates/lazuli_syntax/src/parser/lzx/surface/view.rs:248,263,276; crates/lazuli_syntax/src/parser/lzx/experience/audience.rs:230-238`
- Grammar mandate: admit brace shapes nowhere — `docs/grammar.lzi.md:1166-1168`
- One-canonical-way principle — `docs/architecture.md:260-261`
- design-decisions.md has no entry blessing the section 7a brace/Int/@command shape — `docs/design-decisions.md:32-497`
- Documented LZX-REPEATABLE-SUM-001 and LZX-BOARD-LANES-001 doctor rules are unimplemented — `crates/lazuli_doctor/src (grep returns no files; only rule_category.rs mentions LZX-)`
- surface/ux.rs already exceeds the 500-LOC ceiling and must be split — `crates/lazuli_syntax/src/parser/lzx/surface/ux.rs (747 LOC)`
- Canonical correctness-rule shape to model the new doctor rule on — `crates/lazuli_doctor/src/correctness/full_text_type_001.rs:1-150`
- Upgrade recipe infra (recipe.toml front-matter + input/output IR smoke) — `crates/lazuli_cli/src/upgrade.rs:39-192`
- Codegen consumes board/inline_table/repeatable unchanged in IR shape — `crates/lazuli_codegen_ts/src/lzx_ux.rs:48-101,135-141`

## End state
One canonical indentation-only spelling per intent; no braces, semicolons, Int, @command, or dotted-view anywhere. repeatable becomes the bare keyword `repeatable <name>` (registry entry exists at registry.rs:2875) with child lines `field <name>: <Type>` where Type is a closed-catalog scalar (so Integer, not Int) and `validates sum(<field>) = <n>`; the group keyword and braces/semicolons are dropped because the indented block is the group. inline_table becomes bare `inline_table on_change <name>` (was view.inline_table) with a bare command reference and no @command sigil, matching every other command reference. board becomes bare `board <name>` (was view.board) with the unchanged `lanes derived_from <field>` child. The IR (ir::ViewUx board/inline_table/repeatable_groups) and codegen (lzx_ux.rs) are unchanged in shape because lowering already normalizes @command (list_decls.rs:184-191) and the IR CommandRef/RepeatableField/Board carry no sigil or brace; the only new IR-affecting constraint is that RepeatableField.type_name must be a catalog scalar, so Int becomes Integer in every fixture. Both dialects inherit the new spelling through the shared surface/ux.rs parse functions, so a single fix corrects both. The retired forms hard-error at parse with a code that names the canonical replacement, and lazuli upgrade auto-rewrites existing pilots.

## Parity surfaces
- Parser recognition feature-surface dialect: crates/lazuli_syntax/src/parser/lzx/surface/view.rs:248,263,276 dispatch view.inline_table/view.board/repeatable input to bare inline_table/board/repeatable
- Parser recognition experience dialect: crates/lazuli_syntax/src/parser/lzx/experience/audience.rs:230-242 same dispatch + updated child-list error message
- Shared parse functions: crates/lazuli_syntax/src/parser/lzx/surface/ux.rs:90-127 (inline_table drop @command requirement), :301-391 (board keyword now board), :393-536 (repeatable rewrite brace to block + scalar-catalog check)
- AST: ViewUxAst/InlineTableAst/BoardAst/RepeatableGroupAst (InlineTableAst.on_change bare; RepeatableFieldAst.type_name catalog scalar)
- Analyzer lowering: crates/lazuli_analyzer/src/surface/list_decls.rs:182-240 lower_view_ux (@command strip becomes assert-absent; repeatable field type validated)
- Keyword registry: crates/lazuli_keywords/src/registry.rs:2947 view.inline_table to inline_table, :2953 view.board to board, :2875 repeatable kept, :3218 @command decorator scope review
- LSP completion/hover/typo: crates/lazuli_lsp/src/keywords.rs + canonical_kinds/sections replace dotted forms; add typo catalog old to new
- tmLanguage generated: editors/vscode/syntaxes/lazuli.tmLanguage.json via cargo run -p xtask -- gen-tmlanguage
- Keyword reference generated: docs/keyword-reference.md via cargo run -p xtask -- gen-keyword-reference
- Curated grammar docs: docs/grammar.lzx.md:392-445 (section 7a.5/7a.6 EBNF + examples to indentation)
- Doctor: crates/lazuli_doctor/src/correctness/lzx_surface_brace_retired_001.rs (new) + LSP source-text mirror
- Lazurite scaffold: lazurite/templates/default/** views using these primitives
- examples/: examples/full-capsule/** + any .lzx using view.board/view.inline_table/repeatable input
- Fixtures: crates/lazuli_syntax/src/parser/lzx/lzx_parser_tests.rs:704-741, surface_tests/ux.rs:87-296, codegen_ts/src/lzx_ux.rs tests:194-277
- Migration recipe: migrations/recipes/<from>-to-<to>/section-7a-surface-canonical/ (+ lazuli upgrade)

## Doctor rules
- `LZX-SURFACE-BRACE-RETIRED-001` (error) — Rejects the retired section 7a brace/semicolon repeatable form, the dotted view.board/view.inline_table spellings, and the @command sigil inside on_change. Source-text + parse-level rule firing BEFORE the bare parse error so the author gets a migration message naming the canonical indentation form and the lazuli upgrade recipe. Module header with severity + trigger cue (fires when). Modeled on crates/lazuli_doctor/src/correctness/full_text_type_001.rs.
- `LZX-REPEATABLE-FIELD-SCALAR-001` (error) — Each field name:Type inside a repeatable block must use a closed-catalog scalar (ID|Text|Boolean|Integer|Decimal|Date|DateTime|JSON); rejects the alias Int (suggest Integer) and any out-of-catalog type. IR-level walk of Feature.surfaces[].audiences[].views[].ux.repeatable_groups[].fields[].type_name. Replaces the documented-but-unimplemented LZX-REPEATABLE-SUM-001 with a real type-catalog check.

## Tests required
- lazuli_syntax: positive parse of repeatable block in BOTH dialects yielding fields=[days:Integer, percentage:Decimal], sum_field=percentage, sum_target=100
- lazuli_syntax: positive parse of bare inline_table on_change update_row yielding on_change==update_row without @command
- lazuli_syntax: positive parse of bare board activity + lanes derived_from status
- lazuli_syntax: NEGATIVE retired brace repeatable input form errors mentioning canonical repeatable block + lazuli upgrade
- lazuli_syntax: NEGATIVE dotted view.board/view.inline_table rejected with migration message
- lazuli_syntax: NEGATIVE inline_table on_change @command.update_row rejected (sigil retired)
- lazuli_syntax: NEGATIVE field days: Int rejected with use Integer
- lazuli_analyzer: lower_view_ux over new form yields RepeatableField.type_name==Integer and InlineTable.on_change CommandRef.name==update_row; assert no @command reaches IR
- lazuli_doctor: LZX-SURFACE-BRACE-RETIRED-001 positive+negative; LZX-REPEATABLE-FIELD-SCALAR-001 positive (Int fires) + negative (Integer clean)
- lazuli_keywords: proven_complete.rs green with registry holding bare inline_table/board and no dotted forms
- lazuli_lsp: keyword_surface_parity.rs green with inline_table/board in LSP catalog+tmLanguage+keyword-reference and dotted forms absent
- xtask: keyword_reference_fresh.rs + tmlanguage_fresh.rs green after regeneration
- lazuli_codegen_ts: existing lzx_ux.rs tests pass UNCHANGED with Integer fixtures (proves IR/codegen shape stable)
- lazuli_cli: upgrade-recipe smoke where input.lzx post-upgrade IR == output.lzx IR via lazuli inspect (upgrade.rs:143-192)
- end-to-end: lazuli inspect examples/full-capsule round-trips and generate --check succeeds (NOT lazuli parse, which is lossy)
- editors/vscode: vscode-tmgrammar-snap regenerated and snapshot tests green

## Docs required
- GENERATED: docs/keyword-reference.md via cargo run -p xtask -- gen-keyword-reference (inline_table/board rows replace view rows)
- GENERATED: editors/vscode/syntaxes/lazuli.tmLanguage.json via cargo run -p xtask -- gen-tmlanguage
- CURATED: docs/grammar.lzx.md section 7a.5 (388-404) inline_table EBNF + example to bare keyword + bare command ref
- CURATED: docs/grammar.lzx.md section 7a.6 (406-445) board_decl + repeatable_decl EBNF brace to indentation; Int to Integer in TYPE_NAME example; worked examples 428-439
- CURATED: docs/grammar.lzi.md:1166-1168 note the section 7a brace residue is now fully retired
- CURATED: docs/design-decisions.md short entry recording the section 7a retirement so audit pipelines do not re-propose the brace form; cross-ref the recipe
- CURATED: migration recipe markdown migrations/recipes/<from>-to-<to>/section-7a-surface-canonical/ with from/to/kind/summary front-matter (upgrade.rs:6-16) + input/output fixtures

## LOC plan (<=500/file)
surface/ux.rs is ALREADY 747 LOC (over the 500 ceiling) so this SPEC must split it. (1) Extract repeatable parser into new sibling crates/lazuli_syntax/src/parser/lzx/surface/repeatable.rs (~140 LOC after the brace-to-block rewrite, simpler than the string-surgery it replaces) holding parse_repeatable_group_line + RepeatableFieldAst helpers + scalar-catalog check. (2) Extract board parser into crates/lazuli_syntax/src/parser/lzx/surface/board.rs (~100 LOC). (3) ux.rs retains inline_table+view_mode+tab_group+wizard_steps+tabs/wizard (~480 LOC, under ceiling); re-export moved fns via pub(in crate::parser::lzx) use so view.rs/audience.rs call sites are unchanged (ABI-additive). (4) New doctor files: correctness/lzx_surface_brace_retired_001.rs (~180 LOC incl tests) + lzx_repeatable_field_scalar_001.rs (~160 LOC incl tests), each with module header (severity + trigger cue), registered in correctness/mod.rs. (5) surface_tests/ux.rs (296 LOC): rewritten repeatable tests stay under 500; if it grows, split into surface_tests/repeatable.rs + board.rs via the include sibling pattern. Every touched .rs is <=500 LOC post-change. No pub-symbol deletions; renamed registry entries are catalog data, not ABI.

## Acceptance criteria
- ripgrep for view.board or view.inline_table across crates/ docs/ examples/ lazurite/ returns ZERO non-negative-test hits
- ripgrep for the brace repeatable input form across crates/ docs/ examples/ lazurite/ returns ZERO hits
- No required @command in on_change across parser/examples/scaffold (bare command refs only)
- No Int alias in any .lzx fixture/example/scaffold; repeatable fields use catalog scalars
- Parser accepts the repeatable block + bare inline_table/board in BOTH dialects and rejects all four retired forms with a message naming the canonical form
- cargo build --workspace green and per-crate tests green for lazuli_syntax/analyzer/keywords/lsp/doctor/codegen_ts/xtask
- keyword_surface_parity.rs + proven_complete.rs + keyword_reference_fresh.rs + tmlanguage_fresh.rs all green with no hand-edit of generated files
- LZX-SURFACE-BRACE-RETIRED-001 and LZX-REPEATABLE-FIELD-SCALAR-001 fire on retired forms and are clean on canonical forms; both have module headers with trigger cues
- lazuli inspect examples/full-capsule and generate --check succeed on the migrated example (verified via inspect/generate, not lazuli parse)
- lazuli upgrade recipe rewrites a brace/Int/@command/dotted .lzx so post-upgrade IR equals hand-written canonical IR (upgrade smoke passes)
- Every .rs file touched or created is <=500 LOC (surface/ux.rs split below ceiling)
- codegen_ts lzx_ux.rs tests pass unchanged in shape (proves pure surface migration)

## Migration recipe
BREAKING (pre-users, approved). Ship migrations/recipes/<from>-to-<to>/section-7a-surface-canonical/ as a rewrite-kind recipe (extend upgrade.rs apply_recipe beyond the current additive-only match at upgrade.rs:137-141, or implement as a lazuli fmt --canonical rewrite pass referenced from the recipe). Mechanical line rewrites: (a) view.inline_table on_change @command.<n> becomes inline_table on_change <n> (drop view. prefix + @command sigil); (b) view.board <n> becomes board <n> (drop view. prefix; lanes derived_from body unchanged); (c) repeatable input <n> group { f1: T1; f2: T2 } validates sum(f) = num becomes a multi-line block repeatable <n> then field f1: T1prime then field f2: T2prime then validates sum(f) = num, where each Tiprime maps Int to Integer (and any other alias to its catalog scalar). The recipe ships input.lzx + output.lzx fixtures; the upgrade smoke harness (upgrade.rs:143-192) asserts the lazuli inspect IR of post-upgrade input equals output. The hard parse error LZX-SURFACE-BRACE-RETIRED-001 names the recipe so an author hitting the old form on a fresh build is told exactly which lazuli upgrade to run.

## Justification (token / entropy)
LLM-first expected-tokens-to-correct-output = tokens-per-attempt times attempts; this SPEC cuts BOTH. PEAKEDNESS (fewer attempts): today an LLM authoring a repeatable group faces a high-entropy choice surface: brace-vs-indent (the brace shape is unique in the entire language, so the model has no consistent prior and must recall this one exception), Int-vs-Integer (an alias the closed catalog at grammar.lzi.md:37-38 forbids, yet the parser accepts Int as opaque text at ux.rs:451, producing silent wrong output that only surfaces downstream), and @command-vs-bare (the model has seen bare customer.command.create everywhere, so the section 7a sigil requirement is a retry trap, exactly the ux.rs:109-114 rejection path). Collapsing to one indentation form per intent makes these primitives isomorphic to every other view child (columns/view_mode/tab_group are all bare-keyword indentation blocks), so the model's prior over the rest of the language transfers, output entropy drops, and first-attempt validity rises. TOKENS-PER-ATTEMPT: bare PascalCase Integer and bare keywords inline_table/board/repeatable tokenize cheaper under cl100k/o200k than the @command sigil (the @ and . split into extra tokens) and the dotted view.board (the . boundary forces a subword split versus a single board token); removing braces and semicolons removes punctuation tokens entirely. The indentation form is marginally more lines but each line is structurally identical to its neighbours, which is exactly what an autoregressive model predicts at lowest surprisal. The doctor rule + hard parse error close the loop: a model that does emit the retired form gets a deterministic, replacement-naming diagnostic on attempt N, guaranteeing attempt N+1 converges rather than re-guessing. Net: lower entropy times lower per-attempt cost times bounded retries.
