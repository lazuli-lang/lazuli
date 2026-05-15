# Lazuli — Theme Compatibility Audit

- **Date:** 2026-05-15
- **Grammar version:** post-Wave 2 (commit `3039ec9`)
- **Scope catalog audited:** 99 representative scope families covering all 133 main grammar scopes + 8 Lazurite.toml scopes (a single representative is used for the `entity.name.function.statement.*.lazuli` family because every leaf in that family has identical theming guidance)
- **Method:** static analysis. For each theme, the JSON `tokenColors` table was loaded (resolving `include` chains for the VS Code defaults bundle) and each Lazuli scope was matched against the most-specific theme selector that prefix-covers it. A rule is considered "explicit" if the matched selector has at least 3 dotted segments (e.g. `keyword.control.X`), "generic" if it matches a root family with 1–2 segments (e.g. `keyword.control`), and "no rule" if no rule matches at all. Both "explicit" and "generic" produce a colored token; only **no rule** falls through to the editor foreground (renders plain).

## Themes inspected

| Theme | Source path | Rules loaded |
|---|---|---|
| Default Dark Modern | `c:/Users/lucas/AppData/Local/Programs/Microsoft VS Code/0958016b2a/resources/app/extensions/theme-defaults/themes/dark_modern.json` (includes `dark_plus.json` → `dark_vs.json`) | 169 |
| Default Dark+ | `theme-defaults/themes/dark_plus.json` (includes `dark_vs.json`) | 169 |
| Default Light Modern | `theme-defaults/themes/light_modern.json` (includes `light_plus.json` → `light_vs.json`) | 188 |
| Default Light+ | `theme-defaults/themes/light_plus.json` (includes `light_vs.json`) | 188 |
| Monokai (built-in) | `theme-monokai/themes/monokai-color-theme.json` | 66 |
| Solarized Dark (built-in) | `theme-solarized-dark/themes/solarized-dark-color-theme.json` | 56 |
| One Dark Pro | `~/.vscode/extensions/zhuangtongfa.material-theme-3.19.0/themes/OneDark-Pro.json` | 460 |
| Atom One Dark (akamud) | `~/.vscode/extensions/akamud.vscode-theme-onedark-2.3.0/themes/OneDark.json` | 356 |

Themes from the brief that are **not installed locally** and were therefore **skipped**:

- GitHub Dark Default (`github.github-vscode-theme`) — not installed
- Dracula Official (`dracula-theme.theme-dracula`) — not installed
- Material Theme / Material Theme by PKief — not installed

The `akamud.vscode-theme-onedark` package was used as a stand-in for the popular community One-Dark family alongside the more aggressively-rebranded `zhuangtongfa.material-theme` (which is the package conventionally distributed under the marketplace name "One Dark Pro"). Solarized Dark is the only Solarized variant inspected; Solarized Light is omitted (analogous results expected but not inspected).

## Legend

- `OK` — explicit rule matches (3+ dotted segments). Token gets a leaf-specific accent.
- `gen` — only a generic root family rule matches (1–2 segments). Token still gets a color (the parent family's color), but no per-Lazuli accent. **This is the normal/expected case for the vast majority of TextMate-style scope names** — themes deliberately style at the family level (e.g. one color for *all* `keyword.control.*`, regardless of leaf).
- `NONE` — no rule of any depth matches. Token renders plain (editor foreground). **This is the only column that should worry us.**

## Coverage matrix

| Family | Default Dark Modern | Dark+ | Light Modern | Light+ | Monokai | Solarized Dark | One Dark Pro | Atom One Dark |
|---|---|---|---|---|---|---|---|---|
| `comment.line.number-sign.lazuli` | gen | gen | gen | gen | gen | gen | gen | gen |
| `string.quoted.double.lazuli` | gen | gen | gen | gen | gen | gen | gen | gen |
| `string.quoted.double.escape-route.lazuli` | gen | gen | gen | gen | gen | gen | gen | gen |
| `string.quoted.double.rule-name.lazuli` | gen | gen | gen | gen | gen | gen | gen | gen |
| `constant.character.escape.lazuli` | OK | OK | OK | OK | gen | gen | OK | OK |
| `keyword.control.declaration.structural.lazuli` | gen | gen | gen | gen | gen | gen | gen | gen |
| `keyword.control.section.lazuli` | gen | gen | gen | gen | gen | gen | gen | gen |
| `keyword.control.statement.lazuli` | gen | gen | gen | gen | gen | gen | gen | gen |
| `keyword.operator.assignment.lazuli` | gen | gen | gen | gen | gen | gen | OK | gen |
| `keyword.operator.comparison.lazuli` | gen | gen | gen | gen | gen | gen | OK | gen |
| `keyword.operator.containment.lazuli` | gen | gen | gen | gen | gen | gen | gen | gen |
| `keyword.operator.logical.lazuli` | gen | gen | gen | gen | gen | gen | OK | gen |
| `keyword.operator.plan-and-gate.lazuli` | gen | gen | gen | gen | gen | gen | gen | gen |
| `keyword.operator.predicate.lazuli` | gen | gen | gen | gen | gen | gen | gen | gen |
| `keyword.operator.transition.lazuli` | gen | gen | gen | gen | gen | gen | gen | gen |
| `keyword.operator.union.lazuli` | gen | gen | gen | gen | gen | gen | gen | gen |
| `keyword.other.plan.lazuli` | gen | gen | gen | gen | gen | gen | gen | gen |
| `entity.name.type.aggregate.lazuli` | OK | OK | OK | OK | OK | OK | OK | OK |
| `entity.name.type.app.lazuli` | OK | OK | OK | OK | OK | OK | OK | OK |
| `entity.name.type.feature.lazuli` | OK | OK | OK | OK | OK | OK | OK | OK |
| `entity.name.type.record.lazuli` | OK | OK | OK | OK | OK | OK | OK | OK |
| `entity.name.type.enum.lazuli` | OK | OK | OK | OK | OK | OK | OK | OK |
| `entity.name.function.named-block.lazuli` | OK | OK | OK | OK | OK | OK | OK | OK |
| `entity.name.function.action.lazuli` | OK | OK | OK | OK | OK | OK | OK | OK |
| `entity.name.function.cache-profile.lazuli` | OK | OK | OK | OK | OK | OK | OK | OK |
| `entity.name.function.eval-case.lazuli` | OK | OK | OK | OK | OK | OK | OK | OK |
| `entity.name.function.event-group.lazuli` | OK | OK | OK | OK | OK | OK | OK | OK |
| `entity.name.function.event.lazuli` | OK | OK | OK | OK | OK | OK | OK | OK |
| `entity.name.function.extension-point.lazuli` | OK | OK | OK | OK | OK | OK | OK | OK |
| `entity.name.function.lifecycle.lazuli` | OK | OK | OK | OK | OK | OK | OK | OK |
| `entity.name.function.route.lazuli` | OK | OK | OK | OK | OK | OK | OK | OK |
| `entity.name.function.secret-rotation.lazuli` | OK | OK | OK | OK | OK | OK | OK | OK |
| `entity.name.function.view.lazuli` | OK | OK | OK | OK | OK | OK | OK | OK |
| `entity.name.function.statement.*.lazuli` (all 23 leaves) | OK | OK | OK | OK | OK | OK | OK | OK |
| `entity.name.label.anchor-target.lazuli` | OK | OK | OK | OK | **NONE** | **NONE** | OK | gen |
| `entity.name.label.audience.lazuli` | OK | OK | OK | OK | **NONE** | **NONE** | OK | gen |
| `entity.name.label.integration.lazuli` | OK | OK | OK | OK | **NONE** | **NONE** | OK | gen |
| `entity.name.label.rbac.lazuli` | OK | OK | OK | OK | **NONE** | **NONE** | OK | gen |
| `entity.name.label.slot.lazuli` | OK | OK | OK | OK | **NONE** | **NONE** | OK | gen |
| `entity.name.label.surface-area.lazuli` | OK | OK | OK | OK | **NONE** | **NONE** | OK | gen |
| `entity.name.label.surface-target.lazuli` | OK | OK | OK | OK | **NONE** | **NONE** | OK | gen |
| `entity.name.reference.anchor.lazuli` | **NONE** | **NONE** | **NONE** | **NONE** | **NONE** | **NONE** | **NONE** | gen |
| `entity.name.reference.decorator.lazuli` | **NONE** | **NONE** | **NONE** | **NONE** | **NONE** | **NONE** | **NONE** | gen |
| `entity.name.reference.model-path.lazuli` | **NONE** | **NONE** | **NONE** | **NONE** | **NONE** | **NONE** | **NONE** | gen |
| `entity.name.reference.package.lazuli` | **NONE** | **NONE** | **NONE** | **NONE** | **NONE** | **NONE** | **NONE** | gen |
| `entity.name.reference.semantic.lazuli` | **NONE** | **NONE** | **NONE** | **NONE** | **NONE** | **NONE** | **NONE** | gen |
| `entity.name.tag.decorator.lazuli` | OK | OK | OK | OK | OK | OK | OK | OK |
| `entity.name.audit.fields.lazuli` | **NONE** | **NONE** | **NONE** | **NONE** | **NONE** | **NONE** | **NONE** | gen |
| `support.type.primitive.lazuli` | gen | gen | gen | gen | gen | gen | OK | gen |
| `support.type.ui.lazuli` | gen | gen | gen | gen | gen | gen | **NONE** | gen |
| `support.type.extension.lazuli` | gen | gen | gen | gen | gen | gen | **NONE** | gen |
| `support.type.domain.lazuli` | gen | gen | gen | gen | gen | gen | **NONE** | gen |
| `support.function.type-constructor.lazuli` | gen | gen | gen | gen | gen | gen | gen | gen |
| `support.variable.context.lazuli` | gen | gen | gen | gen | **NONE** | gen | **NONE** | gen |
| `variable.other.binding.lazuli` | gen | gen | gen | gen | gen | gen | gen | gen |
| `variable.other.dictionary-key.lazuli` | gen | gen | gen | gen | gen | gen | gen | gen |
| `variable.other.field.lazuli` | gen | gen | gen | gen | gen | gen | gen | gen |
| `variable.other.member.lazuli` | OK | OK | OK | OK | gen | gen | gen | gen |
| `variable.other.query-key.lazuli` | gen | gen | gen | gen | gen | gen | gen | gen |
| `variable.other.route-slot.lazuli` | gen | gen | gen | gen | gen | gen | gen | gen |
| `variable.other.search-target.lazuli` | gen | gen | gen | gen | gen | gen | gen | gen |
| `variable.other.url-key.lazuli` | gen | gen | gen | gen | gen | gen | gen | gen |
| `constant.language.boolean.lazuli` | gen | gen | gen | gen | gen | gen | gen | gen |
| `constant.language.channel.lazuli` | gen | gen | gen | gen | gen | gen | gen | gen |
| `constant.language.cookie.lazuli` | gen | gen | gen | gen | gen | gen | gen | gen |
| `constant.language.deploy.lazuli` | gen | gen | gen | gen | gen | gen | gen | gen |
| `constant.language.dlq.lazuli` | gen | gen | gen | gen | gen | gen | gen | gen |
| `constant.language.http-method.lazuli` | gen | gen | gen | gen | gen | gen | gen | gen |
| `constant.language.lock.lazuli` | gen | gen | gen | gen | gen | gen | gen | gen |
| `constant.language.log-level.lazuli` | gen | gen | gen | gen | gen | gen | gen | gen |
| `constant.language.report-format.lazuli` | gen | gen | gen | gen | gen | gen | gen | gen |
| `constant.language.rotation.lazuli` | gen | gen | gen | gen | gen | gen | gen | gen |
| `constant.language.template-strategy.lazuli` | gen | gen | gen | gen | gen | gen | gen | gen |
| `constant.language.transport.lazuli` | gen | gen | gen | gen | gen | gen | gen | gen |
| `constant.language.verify.lazuli` | gen | gen | gen | gen | gen | gen | gen | gen |
| `constant.language.visibility.lazuli` | gen | gen | gen | gen | gen | gen | gen | gen |
| `constant.language.selection-mode.lazuli` | gen | gen | gen | gen | gen | gen | gen | gen |
| `constant.language.drawer-trigger.lazuli` | gen | gen | gen | gen | gen | gen | gen | gen |
| `constant.language.search-mode.lazuli` | gen | gen | gen | gen | gen | gen | gen | gen |
| `constant.language.binding-source.lazuli` | gen | gen | gen | gen | gen | gen | gen | gen |
| `constant.language.persistence.lazuli` | gen | gen | gen | gen | gen | gen | gen | gen |
| `constant.numeric.lazuli` | gen | gen | gen | gen | gen | gen | gen | gen |
| `constant.numeric.http-status.lazuli` | gen | gen | gen | gen | gen | gen | gen | gen |
| `constant.other.direction.lazuli` | **NONE** | **NONE** | **NONE** | **NONE** | gen | gen | gen | gen |
| `constant.other.enum-member.lazuli` | **NONE** | **NONE** | **NONE** | **NONE** | gen | gen | gen | gen |
| `constant.other.locale-tag.lazuli` | **NONE** | **NONE** | **NONE** | **NONE** | gen | gen | gen | gen |
| `constant.other.verify-alg.lazuli` | **NONE** | **NONE** | **NONE** | **NONE** | gen | gen | gen | gen |
| `constant.other.wildcard.lazuli` | **NONE** | **NONE** | **NONE** | **NONE** | gen | gen | gen | gen |
| `storage.modifier.lazuli` | gen | gen | gen | gen | gen | gen | gen | gen |
| `punctuation.accessor.lazuli` | **NONE** | **NONE** | **NONE** | **NONE** | **NONE** | **NONE** | gen | gen |
| `punctuation.definition.generic.begin.lazuli` | **NONE** | **NONE** | **NONE** | **NONE** | **NONE** | **NONE** | gen | gen |
| `punctuation.section.parens.begin.lazuli` | **NONE** | **NONE** | **NONE** | **NONE** | **NONE** | **NONE** | gen | gen |
| `punctuation.separator.comma.lazuli` | **NONE** | **NONE** | **NONE** | **NONE** | **NONE** | **NONE** | gen | OK |
| `punctuation.separator.key-value.lazuli` | **NONE** | **NONE** | **NONE** | **NONE** | **NONE** | **NONE** | OK | gen |
| `punctuation.separator.type.lazuli` | **NONE** | **NONE** | **NONE** | **NONE** | **NONE** | **NONE** | gen | gen |
| `entity.name.namespace.Lazurite.toml` | OK | OK | OK | OK | OK | OK | OK | gen |
| `entity.name.tag.lazurite-target.toml` | OK | OK | OK | OK | OK | OK | OK | OK |
| `support.function.lazurite-key.toml` | gen | gen | gen | gen | gen | gen | gen | gen |
| `entity.name.reference.plugin.lazuli` | **NONE** | **NONE** | **NONE** | **NONE** | **NONE** | **NONE** | **NONE** | gen |

## Per-theme totals

| Theme | OK | gen | NONE | Total |
|---|---|---|---|---|
| Default Dark Modern | 29 | 52 | 18 | 99 |
| Default Dark+ | 29 | 52 | 18 | 99 |
| Default Light Modern | 29 | 52 | 18 | 99 |
| Default Light+ | 29 | 52 | 18 | 99 |
| Monokai | 20 | 58 | 21 | 99 |
| Solarized Dark | 20 | 59 | 20 | 99 |
| One Dark Pro | 33 | 55 | 11 | 99 |
| Atom One Dark | 21 | 78 | **0** | 99 |

Atom One Dark (akamud) ships a top-level `entity` rule (and similar wildcards) that catches every `entity.name.*` token, so nothing in the grammar renders plain under it. It is the most-forgiving theme in the audit.

## Cross-theme weak spots — render PLAIN in 2+ themes

Sorted by severity (number of themes with no matching rule).

| Family | Themes with NONE | Severity |
|---|---|---|
| `entity.name.reference.anchor.lazuli` | 7/8 | **CRITICAL** |
| `entity.name.reference.decorator.lazuli` | 7/8 | **CRITICAL** |
| `entity.name.reference.model-path.lazuli` | 7/8 | **CRITICAL** |
| `entity.name.reference.package.lazuli` | 7/8 | **CRITICAL** |
| `entity.name.reference.semantic.lazuli` | 7/8 | **CRITICAL** |
| `entity.name.audit.fields.lazuli` | 7/8 | **CRITICAL** |
| `entity.name.reference.plugin.lazuli` (Lazurite.toml) | 7/8 | **CRITICAL** |
| `punctuation.accessor.lazuli` | 6/8 | high |
| `punctuation.definition.generic.begin.lazuli` (`[`/`]`) | 6/8 | high |
| `punctuation.section.parens.begin.lazuli` (`(`/`)`) | 6/8 | high |
| `punctuation.separator.comma.lazuli` | 6/8 | high |
| `punctuation.separator.key-value.lazuli` | 6/8 | high |
| `punctuation.separator.type.lazuli` | 6/8 | high |
| `constant.other.direction.lazuli` | 4/8 | medium |
| `constant.other.enum-member.lazuli` | 4/8 | medium |
| `constant.other.locale-tag.lazuli` | 4/8 | medium |
| `constant.other.verify-alg.lazuli` | 4/8 | medium |
| `constant.other.wildcard.lazuli` | 4/8 | medium |
| `entity.name.label.anchor-target.lazuli` | 2/8 | low |
| `entity.name.label.audience.lazuli` | 2/8 | low |
| `entity.name.label.integration.lazuli` | 2/8 | low |
| `entity.name.label.rbac.lazuli` | 2/8 | low |
| `entity.name.label.slot.lazuli` | 2/8 | low |
| `entity.name.label.surface-area.lazuli` | 2/8 | low |
| `entity.name.label.surface-target.lazuli` | 2/8 | low |
| `support.variable.context.lazuli` | 2/8 | low |

## Per-theme detail

### Default Dark Modern

- **Coverage:** 29 explicit / 52 generic / 18 plain.
- **Visible deficits in Lazuli context:**
  - All 5 `entity.name.reference.*.lazuli` scopes render plain. This means cross-references like `@runtime/postgres`, `@anchor.foo`, `Item.tags`, `params.id`, and `@plugin/...` (Lazurite.toml) all read with no color — a substantial visual hit since cross-references are dense in `.lzi`.
  - The `entity.name.audit.fields.lazuli` outlier (called out in `SCOPES.md` as non-conventional) renders plain.
  - All 6 punctuation scopes plus `punctuation.accessor` render plain. (This is normal behavior for VS Code defaults — they intentionally don't color most punctuation. Acceptable but worth noting.)
  - All 5 `constant.other.*.lazuli` scopes render plain (sort directions `asc`/`desc`, enum members, locale tags `en-US`, verify algs, wildcards `*`). Notable because enum members are visible in every `enum` body.

- **Recommendation:** see global recommendations below. Defaults Dark Modern / Dark+ / Light Modern / Light+ have **identical** coverage (Modern variants only override workbench colors, not tokenColors), so any change applies to all four.

### Default Dark+

Identical to Default Dark Modern (Modern includes Dark+ which includes Dark VS).

### Default Light Modern

Identical to Default Dark Modern in token coverage.

### Default Light+

Identical to Default Dark Modern in token coverage.

### Monokai (built-in)

- **Coverage:** 20 explicit / 58 generic / 21 plain.
- **Additional deficits beyond the defaults:**
  - All 7 `entity.name.label.*.lazuli` scopes render plain (rbac names, audience names, slot names, surface labels, etc.).
  - `support.variable.context.lazuli` renders plain (so `ctx`, `input`, `output`, `payload`, `route`, `row`, etc. read without color).
- **Saving graces:** the `constant.other.*.lazuli` family **does** color via Monokai's `constant.other` rule, so enum members and the `*` wildcard render correctly.

### Solarized Dark (built-in)

- **Coverage:** 20 explicit / 59 generic / 20 plain.
- Almost identical deficit profile to Monokai (same 7 label scopes plain, same 7 reference/audit/plugin scopes plain, same 6 punctuation scopes plain). One marginal improvement: `support.variable.context.lazuli` matches via `support.variable`.

### One Dark Pro

- **Coverage:** 33 explicit / 55 generic / 11 plain. Highest explicit-rule count of any theme inspected.
- **Specific deficits:**
  - All 5 `entity.name.reference.*.lazuli` scopes render plain (same family-wide gap as the defaults).
  - `support.type.ui.lazuli`, `support.type.extension.lazuli`, `support.type.domain.lazuli` render plain. **One Dark Pro is the only theme inspected where these are plain** — its `support.type` rule is more narrowly scoped (it only matches the leaf `support.type` literally, not `support.type.X`). This means user-defined types and UI component types (Form, Table, etc.) read uncolored under One Dark Pro despite being load-bearing identifiers.
  - `support.variable.context.lazuli` renders plain.
  - `entity.name.audit.fields.lazuli` and `entity.name.reference.plugin.lazuli` render plain.

### Atom One Dark (akamud)

- **Coverage:** 21 explicit / 78 generic / **0 plain**.
- The akamud One Dark theme catches everything via broad selectors (`entity`, `keyword`, `punctuation`, `constant`, etc.) so no Lazuli scope falls through to the foreground. No deficits to report.

## Recommendations

These are listed in priority order. None of them are required for the grammar to "work" — the relevant tokens still receive whatever color VS Code's default editor foreground is. They affect *aesthetics* in popular themes that don't ship rules for the affected families.

### High priority — these are the cases where polished, popular themes leave Lazuli tokens plain

#### R1. Add a fallback alias for `entity.name.reference.*.lazuli`

Five scopes (`anchor`, `decorator`, `model-path`, `package`, `semantic`) plus the Lazurite manifest `entity.name.reference.plugin.lazuli` all render plain in **7 of 8 themes** including all four VS Code defaults and One Dark Pro.

**Root cause:** themes don't conventionally style `entity.name.reference` — VS Code itself only styles `entity.name.type`, `entity.name.function`, `entity.name.tag`, `entity.name.label`. The `entity.name.reference` namespace is non-standard.

**Suggested grammar change** (a follow-up cell, not part of this audit):

- For `entity.name.reference.package.lazuli` (e.g. `@runtime/postgres`) — add the parallel scope `support.class.import.lazuli` or `entity.name.namespace.package.lazuli`. The `entity.name.namespace` form is universally supported (it already gets `OK` for `entity.name.namespace.Lazurite.toml` in 7 of 8 themes).
- For `entity.name.reference.decorator.lazuli` and `entity.name.reference.anchor.lazuli` — these are decorator-shaped (`@foo.bar` / `@anchor.x`); add a parallel `entity.name.tag.X.lazuli` scope so they pick up the same color as the existing curated decorator catalog (`entity.name.tag.decorator.lazuli` is `OK` in all 8 themes).
- For `entity.name.reference.model-path.lazuli` (e.g. `Item.tags`) — add a parallel `support.class.model-path.lazuli` or `entity.name.type.reference.lazuli` (the `entity.name.type` parent is `OK` in all 8 themes).
- For `entity.name.reference.semantic.lazuli` (e.g. `params.id`) — add a parallel `variable.other.property.lazuli`. The `variable.other` parent is universally generic-matched so the token always gets a color.

This single cell would resolve 6 of the 7 critical-severity rows.

#### R2. Re-scope `entity.name.audit.fields.lazuli` to a conventional name

`SCOPES.md` itself flags this as non-conventional and suggests a refactor to `variable.other.audit-fields.lazuli` or `entity.name.label.audit-fields.lazuli`. The audit confirms this matters: 7 of 8 themes have no rule for it. Recommended target: `variable.other.audit-fields.lazuli` (the `variable.other` parent rule is universal — token always renders colored).

#### R3. Add a parallel scope for `support.type.ui.lazuli`, `support.type.extension.lazuli`, `support.type.domain.lazuli` to fix One Dark Pro

One Dark Pro is the popular-theme outlier where these render plain (the other 7 themes are fine via their `support.type` family rules). The cheapest fix is to also emit `support.class.X.lazuli` or to keep the current scope and accept that One Dark Pro users will see UI type names in the editor foreground color. Given that this only affects one theme and the workaround would require dual-emitting scopes, **document and accept** is reasonable.

### Medium priority — affects defaults only

#### R4. `constant.other.*.lazuli` family renders plain in all four VS Code defaults

Affects sort directions (`asc`/`desc`), enum members, locale tags (`en-US`, `pt-BR`), verify algs, and the `*` wildcard. The defaults have rules for `constant.numeric`, `constant.character`, and `constant.language` but not `constant.other`. Suggested fix: emit `constant.language.X.lazuli` instead of `constant.other.X.lazuli` for these (they're closed catalogs anyway), which would move all 5 from plain to colored in the defaults.

The strongest case is `constant.other.enum-member.lazuli` — every `enum` body has these and they currently render plain in stock VS Code. Switch to `constant.language.enum-member.lazuli` (or `variable.other.enum-member.lazuli` if you want them to look like properties).

### Low priority — narrow impact

#### R5. `entity.name.label.*.lazuli` plain in Monokai/Solarized only

The seven label scopes render plain in the two oldest built-in themes (Monokai, Solarized Dark). The defaults all color them via their explicit `entity.name.label` rule (`#C8C8C8`); One Dark Pro and the akamud One Dark also color them. Workaround: dual-emit as `entity.name.tag.label.X.lazuli` if Monokai/Solarized parity matters, otherwise leave as-is — those two themes are aesthetic choices and many users on them already accept lower-coverage syntax highlighting.

#### R6. `support.variable.context.lazuli` plain in Monokai and One Dark Pro

Affects `ctx`, `input`, `output`, etc. Two-theme deficit, both partially: Monokai = plain, One Dark Pro = plain, others = generic. Workaround: switch the grammar to emit `variable.language.context.lazuli` or `variable.other.context.lazuli` (the `variable.other` parent is universal). The original choice of `support.variable` was reasonable but pays a cost — `variable.other.context.lazuli` would render colored in 8/8 themes.

### Theme-author-facing notes

- The `SCOPES.md` "Quick reference" table is already a good resource for theme authors who want to add explicit Lazuli support. No additions needed there.
- Worth advertising in the Lazuli VS Code extension README that adding explicit `entity.name.reference.*.lazuli` rules is the single biggest win a theme author can make.

## Caveats

- This audit is **text-only**. Actual rendering requires running VS Code with each theme active and inspecting tokens via `Developer: Inspect Editor Tokens and Scopes`. The matrix here represents what *should* render based on `tokenColors` rules at the audited theme versions; it does not account for semantic-token overrides (which Lazuli does not emit) or for the editor's own per-language overrides in user settings.
- Themes change between versions. This snapshot reflects the versions installed at audit time (Default themes from VS Code 0958016b2a; One Dark Pro 3.19.0; Atom One Dark 2.3.0; Monokai/Solarized Dark from VS Code defaults bundle).
- Three themes from the brief (GitHub Dark Default, Dracula Official, Material Theme) were skipped because they were not installed locally. To extend the audit, install them and re-run `c:/tmp/scope_audit.py` (the script paths and family list are reproducible).
- The classifier treats a theme rule's match depth as a proxy for "is this a leaf-specific accent or a family fallback". A 2-segment rule like `keyword.control` is classified as `gen` even though it produces a coloured token — this is a deliberate over-conservative bias to surface "no leaf accent" cases. The "NONE" column is the genuine "renders plain" indicator.
- Themes that ship a top-level `entity` / `keyword` / `punctuation` wildcard rule (akamud One Dark is the prominent example) will register as `gen` for everything and `NONE` for nothing, but the resulting color may be visually flat — the audit doesn't measure colour distinctiveness, only existence.
