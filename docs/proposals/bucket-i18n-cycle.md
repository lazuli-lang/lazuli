# Bucket Cycle: i18n / Internationalization (L0→L2)

**Status**: design proposal. Stages 3–9 of the `bucket=i18n` pipeline.
Implementation deferred to a separate run with `mode=implement`.

**Audience**: language team (Lazuli core), Lazuli Go runtime team.

**Date**: 2026-05-11.

**Pilot bucket**: i18n is a §1.22 candidate, not part of the four L0→L2
pilot buckets (auth / storage / jobs / observability) declared in
`docs/roadmap.md:23-45`. It is proposed as a **post-pilot expansion
bucket**: language declares the contract, the runtime/adapters fill the
ICU/CLDR mechanics. The cycle is feasible because (a) `default_locale`
already exists as L0, (b) the `logging`/`tracing` precedent from row 36
(commit `71a889a`) gives a clean app-block template, (c) every
authoring axis that needs localization (rule `message`, notification
`template`, surface labels) already appears in the canonical fixture.

## Contexto

The canonical fixture is **mono-locale**. The only language-level i18n
construct today is the bare scalar `default_locale "pt-BR"` at
`examples/full-capsule/app.lzi:7`, lowered to
`AppManifest.default_locale: Option<String>` at
`crates/lazuli_ir/src/lib.rs:1318`. It is parsed by `parse_app_manifest`
at `crates/lazuli_cli/src/app_manifest.rs:386-387` and recognised as an
`is_app_scalar_child` keyword at `crates/lazuli_lsp/src/lib.rs:8154`.

That is the entire L0 surface. There is:

- **no supported-locales list** — the runtime cannot know what locales
  to negotiate against without one.
- **no fallback chain** — when a translation is missing in `es-AR`,
  there is no declared rule that says "fall back to `es` then `en`".
- **no translation table** — strings live inline in `.lzi` (rule
  messages: `full-capsule.lzi:147, 157, 165`) and in external template
  files (notification templates: `full-capsule.lzi:826, 837`) with
  zero contract for variants.
- **no locale-negotiation declaration** — the runtime has no signal to
  wire `Accept-Language` header parsing into a `Ctx.Locale` axis. The
  word `locale` appears in `app.communication propagate` and
  `workspace.communication propagate` allowlists
  (`crates/lazuli_lsp/src/lib.rs:6704, 8252`), so the locator slot is
  reserved, but nothing populates it.
- **no missing-translation reporting** — doctor cannot tell an author
  that `view list` uses a `title` key that has no `translation` entry,
  because there are no `title`s and no `translation` entries.
- **no extraction CLI** — there is no `lazuli translate extract`.

The audit `docs/audit/framework-coverage-1400.md:314-320` (§24
Internacionalização) summarises the layering exactly:

| Tier | Items | Owner |
|---|---|---|
| **L0** | `default_locale "pt-BR"` in `app.lzi:7` | language (shipped) |
| **DL** | `locale` kind, `translation` kind, locale negotiation, locale middleware, missing-translation doctor rule, translation fallback, translation extraction CLI | language (this proposal) |
| **DF** | ICU message format, pluralization, gender rules, date/time/number/currency localization, timezone support | Lazuli Go runtime |
| **DA** | Lokalise, Crowdin, Phrase | adapters |
| **F** | Cut i18n full chain — deferred until a real multi-locale pilot | pilot-gated |

This proposal designs **DL only**. ICU runtime is a parallel Lazuli
Go runtime deliverable; adapters are out of scope.

**Boundary discipline reminder**: Lazuli core never names ICU, CLDR,
gettext, fluent, polyglot.js, react-intl, i18next, FormatJS,
moment-timezone, Intl.NumberFormat. Those are runtime/adapter concerns.
The language declares **which locales exist, what is translatable, how
fallback flows, and where the catalog lives**; the Lazuli Go runtime
picks the message format library; adapters export to TMS platforms.

## Baseline (Stages 1-2 inventory)

| Construct | Surface | Grammar | IR | Doctor/LSP | Codegen | Runtime | L-level |
|---|---|---|---|---|---|---|---|
| `default_locale "<tag>"` | `app.lzi:7` | yes (`is_app_scalar_child` at lsp:8154) | `AppManifest.default_locale: Option<String>` (`lazuli_ir/src/lib.rs:1318`) | none (scalar pass-through) | none | none — `Ctx` has no `Locale` field | **L0** |
| `default_timezone "<zone>"` | `app.lzi:8` | yes (`is_app_scalar_child` at lsp:8155) | `AppManifest.default_timezone: Option<String>` (`lazuli_ir/src/lib.rs:1320`) | none | none | none | **L0** |
| `propagate locale` in app.communication | `app.lzi:72` allowlist | yes (LSP closed catalog `lsp:6704, 8252`) | typed under `AppCommunication.propagate[]` | LSP closed catalog | none | `Ctx.Locale` field absent (`runtime/go/lazuli/ctx.go`) | **L0 (reserved slot)** |
| Rule `message "<text>"` | `full-capsule.lzi:147, 157, 165` | yes | text on `Rule` | none | none | none | **language gap** — strings inline, not localizable |
| Notification `template "./path"` | `full-capsule.lzi:826, 837` | yes | `Notification.template_path: Option<String>` | none | none | none | **language gap** — single path, no locale variants |
| Surface labels (view `title`, action labels, empty states) | not authored | n/a | n/a | n/a | n/a | n/a | **language gap** — pre-pilot; defer |
| `locale` kind | not in fixture; roadmap §1.22 | n/a | n/a | n/a | n/a | n/a | **proposed** |
| `translation` kind | not in fixture; roadmap §1.22 | n/a | n/a | n/a | n/a | n/a | **proposed** |
| `locale_negotiate` middleware | not in fixture; roadmap §1.22 | n/a | n/a | n/a | n/a | n/a | **proposed** |
| Missing-translation doctor rule | not in fixture; roadmap §1.22 | n/a | n/a | n/a | n/a | n/a | **proposed** |
| `lazuli translate extract` CLI | not implemented; roadmap §1.22 | n/a | n/a | n/a | n/a | n/a | **proposed** |
| ICU / pluralization / gender | not authored | n/a | n/a | n/a | n/a | n/a | **DF (Lazuli Go)** |
| Date/number/currency localization | not authored | n/a | n/a | n/a | n/a | n/a | **DF (Lazuli Go)** |
| Lokalise / Crowdin / Phrase | not authored | n/a | n/a | n/a | n/a | n/a | **DA (adapters)** |

**Cross-cutting fact**: every authoring construct that carries a
translatable string today (rule messages, notification templates,
surface labels) is either (a) anonymous inline (rule messages) or
(b) file-path-referenced (templates). Promoting both to typed
**translation keys** is the unifying axis this proposal needs.

## Linguagem proposta (Stage 3)

The surface adds **two new top-level kinds** (`locale` in `app.lzi`,
`translation` in features), one **decorator** on `api` for negotiation,
and three **referencing rules** that connect the existing translatable
strings (rule messages, notification templates) to translation keys.
All other ICU / pluralization / gender / date / number / currency
machinery stays in the runtime layer.

### 3.1 `locale` block in `app.lzi` — supported locales + fallback chain

The bare scalar `default_locale "pt-BR"` is preserved as a one-line
authoring shortcut, but a typed block becomes the canonical form once
the app supports more than one locale:

```lzi
app AcmeCRM
  locale
    default "pt-BR"
    supported "pt-BR", "en-US", "es-AR"
    fallback "pt-BR" -> "en-US"
    fallback "es-AR" -> "en-US"
```

Slot rules:

| Slot | Required | Type | Closed catalog |
|---|---|---|---|
| `default "<tag>"` | required | BCP-47 tag (string) | shape check only — closed catalog of locales is too large; doctor accepts any well-formed BCP-47 tag |
| `supported "<tag>"[, "<tag>"]+` | required, list | BCP-47 tags | shape check |
| `fallback "<src>" -> "<dst>"` | optional, repeatable | BCP-47 tag pair | shape check; doctor verifies both tags appear in `supported` |

The bare-scalar form `default_locale "pt-BR"` still parses (back-compat),
but is **deprecated when the `locale` block is present**. When both
exist, doctor emits a warning `app_locale_block_overrides_default_locale`
pointing to the block as canonical.

Profile overrides:

```lzi
profile production
  locale
    default "en-US"
    supported "en-US", "pt-BR"
```

Justification: today's bare scalar cannot express *what locales the
runtime is allowed to negotiate to*. Without a `supported` list, the
locale-negotiation middleware (§3.3) has nothing to match
`Accept-Language` against. The `fallback` chain belongs in the
language because it is **observable contract**: an LLM cold-reading the
manifest should see that `es-AR` falls back to `en-US`, not pt-BR.

**What this is not**: it is not a locale data catalog, not a CLDR
override, not a region/script/variant override schema. Region/script
detection (e.g. `es-AR` → `es-419`) is runtime work.

### 3.2 `translation` block — typed translation keys per resource/view

A `translation` block sits **inside a feature or experience** and
declares translatable keys with optional pluralization arms. The
runtime + adapter load the matching message at request time using the
declared catalog path.

```lzi
feature customer
  translation
    catalog "./i18n/customer.<locale>.json"

    key archive_archived_blocked
      pt-BR "Não é possível reatribuir um cliente arquivado"
      en-US "Cannot reassign an archived customer"

    key archive_deleted_blocked
      pt-BR "Não é possível arquivar um cliente excluído"
      en-US "Cannot archive a deleted customer"

    key enterprise_owner_required
      pt-BR "Clientes enterprise exigem um proprietário antes da ativação"
      en-US "Enterprise customers require an owner before activation"
      plural one
        pt-BR "Cliente enterprise exige um proprietário antes da ativação"
        en-US "Enterprise customer requires an owner before activation"
      plural other
        pt-BR "Clientes enterprise exigem um proprietário antes da ativação"
        en-US "Enterprise customers require an owner before activation"
```

Slot rules:

| Slot | Required | Type | Closed catalog |
|---|---|---|---|
| `catalog "<path>"` | required | file path with `<locale>` placeholder | shape check; doctor warns if file does not exist at extract time, not at `doctor` time |
| `key <name>` | required, repeatable | identifier | n/a — namespace owned by the feature |
| `<bcp47-tag> "<text>"` (inside a key) | required, one per supported locale | tag → string | tags must appear in `app.locale.supported` |
| `plural <arm>` (inside a key) | optional, repeatable | arm name | **closed**: `zero`, `one`, `two`, `few`, `many`, `other` (CLDR plural categories — the catalog is fixed by the spec, not invented by Lazuli) |

The catalog path uses `<locale>` as a placeholder; the Lazuli Go runtime resolves it
to `./i18n/customer.pt-BR.json`, `./i18n/customer.en-US.json`, etc.
The catalog **format** (JSON / YAML / .properties / fluent / ICU
MessageFormat) is a runtime/adapter decision; Lazuli only declares
the path shape.

Plural arm names are **CLDR plural categories** (`zero`, `one`, `two`,
`few`, `many`, `other`) — closed by the spec, not by Lazuli. Doctor
validates that the named arm matches one of the six. The actual plural
*rule* (which arm fires for which integer in which locale) is CLDR
data — the runtime picks it up.

**Variable interpolation**: keys may contain placeholders like
`{customer_name}`, `{count}`. The placeholders are **declared
syntactically inside the key body** but the **substitution semantics
(simple `{name}` vs ICU MessageFormat) is runtime-side**. The language
declares the contract; the runtime renders.

**What this is not**: it is not a localization editor, not a translation
memory, not a string deduplication framework, not a per-string
machine-translation hook. Those are TMS/adapter concerns.

### 3.3 `locale_negotiate` decorator on `api` and `app.runtime` units

The locator slot for `ctx.locale` already exists in the propagate
allowlist (`crates/lazuli_lsp/src/lib.rs:6704, 8252`). What is missing
is a declaration that says "the runtime must populate `ctx.locale`
from incoming requests".

```lzi
app AcmeCRM
  runtime
    unit api
      serves queries, commands, webhooks, apis
      healthcheck "/healthz"
      readiness "/readyz"
      locale_negotiate
        source accept_language
        strategy best_match
```

Slot rules:

| Slot | Required | Type | Closed catalog |
|---|---|---|---|
| `source <axis>` | optional, default `accept_language` | identifier | **closed**: `accept_language` (HTTP `Accept-Language` header), `query_param`, `cookie`, `user_profile` (read from `ctx.user.locale` if the user resource declares such a field), `subdomain` |
| `strategy <name>` | optional, default `best_match` | identifier | **closed**: `best_match` (RFC 4647 lookup), `prefix_match`, `exact_match` |
| `fallback <tag>` | optional | BCP-47 tag | doctor verifies the tag appears in `app.locale.supported`; defaults to `app.locale.default` if omitted |

The middleware reads the declared source, runs the negotiation
algorithm, writes the resolved tag into `ctx.locale`. The propagation
to async jobs/webhooks is already handled by
`app.communication propagate locale` — this declaration only owns
**ingress resolution**.

Per-`api` override (when an API needs a different negotiation strategy
from the global default):

```lzi
api customer_export
  method GET
  path "/api/customers/export"
  output @cap.File(max_size:100mb,accept:text/csv,visibility:signed)
  locale_negotiate
    source query_param
    strategy exact_match
  policy @policy.global_read
  handler "./api/export_customers.go"
```

**What this is not**: it is not a Geo-IP service, not a CDN edge rule,
not a region/script normalizer. Those are adapters.

### 3.4 Referencing translation keys from rule `message` and notification `template`

The existing translatable surfaces (rule messages, notification
templates) gain an **opt-in typed reference** to a translation key.
Backwards compatible: inline strings keep parsing.

#### 3.4.1 Rule message via key reference

Existing inline form (still legal):

```lzi
rule "archived customers cannot be reassigned"
  deny Customer.reassign when self.lifecycle_stage = CustomerStatus.archived
  message "Cannot reassign an archived customer"
```

New typed reference form:

```lzi
rule "archived customers cannot be reassigned"
  deny Customer.reassign when self.lifecycle_stage = CustomerStatus.archived
  message @translation.archive_archived_blocked
```

The `@translation.<key>` reference resolves against the surrounding
feature's `translation` block. Doctor cross-checks: (a) the key
exists, (b) it has a value for every locale in `app.locale.supported`.

#### 3.4.2 Notification template via locale-variant path

Existing form:

```lzi
notification welcome_email
  template "./outreach/welcome_email.mjml"
```

New locale-variant form (file path contains `<locale>`):

```lzi
notification welcome_email
  template "./outreach/welcome_email.<locale>.mjml"
```

Resolution: the Lazuli Go runtime picks the template based on `ctx.locale` after
negotiation. The same path-placeholder mechanism as `translation
catalog` keeps the contract uniform — both use `<locale>` as a literal
filename token.

**Why not promote rule messages to mandatory typed keys**: piloting
discipline. The fixture has three rule messages today; mandatory
typing forces every existing rule to migrate before the language is
useful. Opt-in keeps the inline form working for prototypes and
single-locale apps; doctor warns when an app declares `locale
supported` with more than one tag *and* has untyped rule messages
(`rule_message_inline_with_multilocale`).

### 3.5 Reserved namespace: `@translation.<key>`

Add `@translation` to the closed reference catalog
(`crates/lazuli_lsp/src/lib.rs:2114-2135`,
`is_allowed_reference_namespace`). It is reference-only — keys are
declared in `translation` blocks, never via `@translation` syntax.

The fully-qualified form is `<feature>.@translation.<key>` for
cross-feature references (rare; most usage is same-feature shorthand
`@translation.<key>`). Doctor verifies that the key resolves in
scope.

### 3.6 CLI: `lazuli translate extract`

A new subcommand walks the package, harvests every translatable
surface, and writes catalog stub files keyed by feature.

```text
$ lazuli translate extract examples/full-capsule --out ./i18n
extracted 7 keys to ./i18n/customer.pt-BR.json
extracted 2 keys to ./i18n/customer_outreach.pt-BR.json
3 missing translations in customer.es-AR.json
2 missing translations in customer_outreach.es-AR.json
```

Sources walked:

- `rule message "<text>"` → key candidate (heuristic: short
  identifier from the rule's quoted name)
- `rule message @translation.<key>` → already-typed reference
- `notification template "<path>"` → key candidate for the template
  filename (one per supported locale)
- `view title "<text>"` / `view empty_state "<text>"` (post-pilot —
  see §3.7 deferred)

Output: per-feature JSON catalogs matching the declared
`translation catalog "<path>"` shape. The exact JSON schema is **a
runtime contract**, not a language contract — the CLI invokes a Lazuli
Go helper to write the format that the runtime loads. Lazuli only owns
the
extraction surface (which keys exist, what locales must cover them).

Exit codes:

- `0` — extraction complete, all keys covered in every supported
  locale.
- `1` — extraction complete, but at least one locale has missing
  translations. CI gate.

The CLI is **read-only on `.lzi` source**: it never edits authored
files. New keys discovered (e.g. a new rule with a new inline message)
are added to the JSON catalog only; if the author wants to convert the
inline `message "<text>"` to `message @translation.<key>`, they do so
by hand.

### 3.7 Surface labels — pilot-gated, **deferred**

Surface labels (view `title`, action labels, empty states, validation
error display) are the **highest-volume** translation surface in any
real product. The canonical fixture has zero authored labels in
`.lzx` today (grep confirms — `full-capsule.lzx` has no
`title`/`label`/`placeholder`/`description`/`caption`/`tooltip`/`empty`/
`message` tokens). Designing `view title @translation.<key>` ahead of
pilot pressure invents surface shape.

**Defer until** the canonical fixture authors at least one explicit
view-level translatable surface (e.g. `view list title
@translation.customer_list_title`). When that happens, this proposal's
§3.5 namespace + §3.6 extraction CLI extend mechanically to cover the
new surfaces.

## IR proposto (Stage 4)

All additive — no schema breakage. Inspect projection adds new
expansions; existing consumers keep working.

### 4.1 `AppLocale` struct (new)

```rust
// crates/lazuli_ir/src/lib.rs — add after AppTracing
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AppLocale {
    /// BCP-47 tag, e.g. "pt-BR".
    pub default: String,
    /// BCP-47 tags. Must include `default`.
    pub supported: Vec<String>,
    /// Fallback edges: src -> dst. Both must appear in `supported`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fallbacks: Vec<LocaleFallback>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocaleFallback {
    pub from: String,
    pub to: String,
}
```

Position: between `AppTracing` and the existing `AppEnvVar`. Add to
`AppManifest`:

```rust
/// i18n bucket cycle — `app.locale` block. Supersedes the bare
/// scalar `default_locale` when both are present.
#[serde(default, skip_serializing_if = "Option::is_none")]
pub locale: Option<AppLocale>,
```

The legacy `default_locale: Option<String>` field stays. When `locale`
is `Some`, the analyzer copies `locale.default` into `default_locale`
for back-compat consumers and emits the
`app_locale_block_overrides_default_locale` warning if both are
authored.

### 4.2 `Translation` struct (new — feature-level)

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Translation {
    /// Catalog path with `<locale>` placeholder, e.g.
    /// `./i18n/customer.<locale>.json`.
    pub catalog: String,
    pub keys: Vec<TranslationKey>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TranslationKey {
    pub name: String,
    /// One entry per BCP-47 tag; must cover `app.locale.supported`.
    pub variants: Vec<TranslationVariant>,
    /// CLDR plural arms (`zero`/`one`/`two`/`few`/`many`/`other`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub plurals: Vec<TranslationPluralArm>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranslationVariant {
    pub locale: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranslationPluralArm {
    pub arm: String, // "zero" | "one" | "two" | "few" | "many" | "other"
    pub variants: Vec<TranslationVariant>,
}
```

Add to `Feature`:

```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
pub translation: Option<Translation>,
```

Same shape lives on `Experience` (for surface labels later — §3.7
deferred, but the IR slot can land now to avoid migration later):

```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
pub translation: Option<Translation>,
```

### 4.3 `LocaleNegotiate` struct (new)

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct LocaleNegotiate {
    /// "accept_language" | "query_param" | "cookie" | "user_profile" | "subdomain"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// "best_match" | "prefix_match" | "exact_match"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strategy: Option<String>,
    /// BCP-47 tag.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback: Option<String>,
}
```

Add to `AppRuntimeUnit`:

```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
pub locale_negotiate: Option<LocaleNegotiate>,
```

Add to `Api` (per-endpoint override):

```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
pub locale_negotiate: Option<LocaleNegotiate>,
```

### 4.4 `Rule.message_ref: Option<String>` (extension on existing struct)

`Rule` today carries `message: Option<String>` (inline text). Add:

```rust
/// When `message` references a `@translation.<key>` instead of an
/// inline string. Mutually exclusive with `message` (one of the two
/// is populated, never both).
#[serde(default, skip_serializing_if = "Option::is_none")]
pub message_ref: Option<String>,
```

Parser sets `message_ref` when the value starts with `@translation.`;
otherwise sets `message`. Doctor cross-checks the key against the
feature's `Translation.keys[]`.

### 4.5 `Notification.template_path` already carries `<locale>` token

No IR change needed — the existing `Notification.template_path:
Option<String>` field carries the path verbatim. Doctor adds a check:
when the path contains `<locale>`, all `app.locale.supported` tags
must have a matching file at extract time (`lazuli translate extract`
verifies this; `lazuli doctor` does not touch the filesystem).

### 4.6 Inspect JSON shape

```json
{
  "app": {
    "locale": {
      "default": "pt-BR",
      "supported": ["pt-BR", "en-US", "es-AR"],
      "fallbacks": [
        { "from": "pt-BR", "to": "en-US" },
        { "from": "es-AR", "to": "en-US" }
      ]
    },
    "runtime": [
      {
        "name": "api",
        "locale_negotiate": {
          "source": "accept_language",
          "strategy": "best_match"
        }
      }
    ]
  },
  "features": [
    {
      "name": "customer",
      "translation": {
        "catalog": "./i18n/customer.<locale>.json",
        "keys": [
          {
            "name": "archive_archived_blocked",
            "variants": [
              { "locale": "pt-BR", "text": "Não é possível ..." },
              { "locale": "en-US", "text": "Cannot reassign ..." }
            ]
          }
        ]
      },
      "rules": [
        {
          "name": "archived customers cannot be reassigned",
          "message_ref": "archive_archived_blocked"
        }
      ]
    }
  ]
}
```

New inspect expansions:

- `--expand=locale` — projects `app.locale` + per-feature translation
  key counts + missing-variant report.
- `--expand=translations` — projects the full per-feature
  `Translation` block including all keys and variants.

### 4.7 New cross-refs the analyzer must register

| Edge | Source | Target | Resolution |
|---|---|---|---|
| `Translation.variants[].locale` | each variant tag | `app.locale.supported[]` | doctor `translation_locale_unknown` |
| `TranslationKey` coverage | all key variants | every tag in `app.locale.supported` | doctor `translation_locale_coverage_incomplete` |
| `Rule.message_ref` | feature-local key ref | `Translation.keys[].name` in same feature | doctor `translation_key_unknown` |
| `LocaleFallback.from`/`.to` | both tags | `app.locale.supported[]` | doctor `locale_fallback_tag_unknown` |
| `LocaleNegotiate.fallback` | tag | `app.locale.supported[]` | doctor `locale_negotiate_fallback_unknown` |
| `LocaleNegotiate.source` | identifier | closed catalog | doctor `locale_negotiate_source_unknown` |
| `LocaleNegotiate.strategy` | identifier | closed catalog | doctor `locale_negotiate_strategy_unknown` |
| `Notification.template_path` containing `<locale>` | path token | `app.locale.supported[]` (filesystem check deferred to extract CLI) | LSP hint only |
| Plural arm | identifier | CLDR plural category catalog | doctor `translation_plural_arm_unknown` |
| Multi-locale + inline rule message | rule with `message`, not `message_ref`, while `app.locale.supported.len() > 1` | n/a | doctor warning `rule_message_inline_with_multilocale` |
| `default_locale` scalar + `locale` block both present | both populated | n/a | doctor warning `app_locale_block_overrides_default_locale` |

### 4.8 Diagnostics list

| Code | Severity |
|---|---|
| `app_locale_default_not_in_supported` | error |
| `app_locale_supported_duplicate` | error |
| `app_locale_block_overrides_default_locale` | warning |
| `locale_fallback_tag_unknown` | error |
| `locale_fallback_cycle` | error |
| `locale_negotiate_source_unknown` | error |
| `locale_negotiate_strategy_unknown` | error |
| `locale_negotiate_fallback_unknown` | error |
| `translation_locale_unknown` | error |
| `translation_locale_coverage_incomplete` | error |
| `translation_key_unknown` | error |
| `translation_plural_arm_unknown` | error |
| `translation_catalog_missing_locale_token` | warning |
| `rule_message_inline_with_multilocale` | warning |
| `notification_template_locale_token_with_monolocale` | warning |

15 diagnostics total. 12 are errors, 3 are warnings. All register
under the existing doctor pipeline; LSP picks up the closed-catalog
ones (`source`, `strategy`, plural arm) via existing
`OBSERVABILITY_CATALOG_VALUES`-style completion infrastructure.

## Codegen proposto (Stage 5)

The codegen surface is **thin** because most i18n is runtime-side
ICU/CLDR mechanics. Three generated artifacts:

### 5.1 `dist/go/app/locale.gen.go`

```go
// path: dist/go/app/locale.gen.go
// Code generated by Lazuli. DO NOT EDIT.
package app

import "github.com/lazuli/runtime/go/lazuli/i18n"

// LocaleContract is the lowered `app.locale` block from app.lzi.
var LocaleContract = i18n.LocaleContract{
    Default:   "pt-BR",
    Supported: []string{"pt-BR", "en-US", "es-AR"},
    Fallbacks: []i18n.Fallback{
        {From: "pt-BR", To: "en-US"},
        {From: "es-AR", To: "en-US"},
    },
}

// LocaleNegotiate per runtime unit (only "api" declares it today).
var LocaleNegotiateAPI = i18n.NegotiateContract{
    Source:   i18n.SourceAcceptLanguage,
    Strategy: i18n.StrategyBestMatch,
    Fallback: "pt-BR",
}
```

### 5.2 `dist/go/<feature>/translations.gen.go` (per feature with `translation` block)

```go
// path: dist/go/customer/translations.gen.go
// Code generated by Lazuli. DO NOT EDIT.
package customer

import "github.com/lazuli/runtime/go/lazuli/i18n"

// TranslationContract pins the key catalog + catalog path. The
// runtime loads ./i18n/customer.<locale>.json at boot.
var TranslationContract = i18n.FeatureTranslation{
    Catalog: "./i18n/customer.<locale>.json",
    Keys: []i18n.KeyContract{
        {
            Name: "archive_archived_blocked",
            Variants: map[string]string{
                "pt-BR": "Não é possível reatribuir um cliente arquivado",
                "en-US": "Cannot reassign an archived customer",
            },
        },
        // ...
    },
}
```

### 5.3 Per-rule binding (extends existing `rules.gen.go`)

For each `rule` with `message_ref`, the rendered Go binding looks up
the typed key at the resolved locale:

```go
// path: dist/go/customer/rules.gen.go (existing file, extended)
// Code generated by Lazuli. DO NOT EDIT.

var ReassignArchivedRule = lazuli.Rule{
    Name:    "archived customers cannot be reassigned",
    Predicate: ruleArchivedPredicate,
    Message: i18n.MessageRef{
        Feature: "customer",
        Key:     "archive_archived_blocked",
    }, // resolved at request time using ctx.Locale
}
```

Inline `message "<text>"` rules continue to use the literal string —
no codegen change for the back-compat path.

## Runtime proposto (Stage 6)

Four new files under `runtime/go/lazuli/i18n/`. The boundary stays
firm: the language declares **what locales exist, what translates,
how to negotiate**; the Lazuli Go runtime wires **ICU rendering, CLDR plural rules,
catalog loading, locale-aware Time/Number/Currency formatting**;
adapters export to TMS platforms (Lokalise, Crowdin, Phrase).

### 6.1 `runtime/go/lazuli/i18n/contract.go`

- **Capability**: typed `LocaleContract`, `FeatureTranslation`,
  `KeyContract`, `NegotiateContract`, `Fallback`, `MessageRef`
  structs mirroring the IR.
- **Lifecycle**: boot-time. Singletons.
- **Typed errors**:
  - `ErrLocaleNotSupported` — the negotiated tag is not in
    `LocaleContract.Supported`.
  - `ErrTranslationKeyUnknown` — the resolved key has no variant in
    the requested locale, and no fallback resolves either.
  - `ErrCatalogMissing` — the catalog file at the resolved
    `./i18n/<feature>.<locale>.json` path is missing.

### 6.2 `runtime/go/lazuli/i18n/negotiate.go`

- **Capability**: HTTP middleware that reads the declared
  `NegotiateContract.Source` (Accept-Language header, query param,
  cookie, user profile, subdomain), runs the declared `Strategy`
  algorithm (best-match per RFC 4647, prefix-match, exact-match),
  walks the `LocaleContract.Fallbacks` if the requested tag is
  unsupported, and writes the resolved tag into `ctx.Locale`.
- **Lifecycle**: per-request middleware. Mounted by the runtime unit
  that declares `locale_negotiate`.
- **Dependency**: `golang.org/x/text/language` (CLDR matching tables).

### 6.3 `runtime/go/lazuli/i18n/catalog.go`

- **Capability**: load `./i18n/<feature>.<locale>.json` files at boot,
  cache them in-memory, expose `Render(featureName, key,
  locale, args...) string`. The ICU MessageFormat parsing (variable
  interpolation, plural selection, gender selection) lives here.
- **Lifecycle**: boot-time load; per-request render.
- **Dependency**: an ICU library (`github.com/<tbd>/icu-go` or
  similar). The exact library is a runtime choice, not language.
- **Plural arms**: rendered via CLDR plural rules from
  `golang.org/x/text/feature/plural`.

### 6.4 `runtime/go/lazuli/i18n/format.go`

- **Capability**: locale-aware Time, Date, Number, Currency, Duration
  formatters. Wraps `golang.org/x/text/{message, number, currency}`.
- **Lifecycle**: per-call.
- **What the Lazuli Go runtime exposes to generated code**: helpers like
  `i18n.FormatCurrency(amount, currencyCode, locale)`,
  `i18n.FormatDateTime(t, locale)`, `i18n.FormatNumber(n, locale)`.
  Generated handlers call these when rendering responses.
- **Not in language**: which fields format as currency vs. number is
  derived from existing `@semantic.*` capabilities (e.g.
  `@semantic.Money`, `@semantic.Decimal`, `@semantic.Timestamp`) —
  no new language surface needed.

### 6.5 What the Lazuli Go runtime does NOT do

- Translation management UI (TMS) — adapter (`@plugin/lokalise/tms`,
  `@plugin/crowdin/sync`, etc.).
- Machine translation — adapter.
- Translation memory — adapter.
- Region/script auto-detection beyond CLDR matching tables — adapter.
- Right-to-left text direction injection at the React/Expo layer —
  the platform's i18n library (react-intl, FormatJS, react-native-localize).

The language commits to **three intent axes** (locales, translations,
negotiation); the Lazuli Go runtime commits to a stable interface (catalog loader +
renderer + formatter + middleware); adapters fill TMS, machine
translation, and platform-specific glue.

## Evals/Testes propostos (Stage 7)

### 7.1 Golden eval — locale negotiation

`tests/golden/i18n/locale_negotiate.jsonl`:

```jsonl
{
  "name": "accept_language_best_match_picks_pt_br",
  "request": {
    "headers": { "Accept-Language": "pt-BR,pt;q=0.9,en;q=0.5" }
  },
  "app_locale": {
    "default": "pt-BR",
    "supported": ["pt-BR", "en-US", "es-AR"]
  },
  "expect": { "ctx_locale": "pt-BR" }
}
{
  "name": "accept_language_unknown_falls_back_to_default",
  "request": {
    "headers": { "Accept-Language": "fr-FR,fr;q=0.9" }
  },
  "app_locale": {
    "default": "pt-BR",
    "supported": ["pt-BR", "en-US"]
  },
  "expect": { "ctx_locale": "pt-BR" }
}
{
  "name": "explicit_fallback_chain_walks_es_ar_to_en_us",
  "request": {
    "headers": { "Accept-Language": "es-AR" }
  },
  "app_locale": {
    "default": "pt-BR",
    "supported": ["pt-BR", "en-US"],
    "fallbacks": [{ "from": "es-AR", "to": "en-US" }]
  },
  "expect": { "ctx_locale": "en-US" }
}
```

### 7.2 Golden eval — translation key render

`tests/golden/i18n/translation_render.jsonl`:

```jsonl
{
  "name": "renders_pt_br_when_ctx_locale_is_pt_br",
  "feature": "customer",
  "key": "archive_archived_blocked",
  "ctx_locale": "pt-BR",
  "expect": { "text": "Não é possível reatribuir um cliente arquivado" }
}
{
  "name": "falls_back_to_default_when_missing_variant",
  "feature": "customer",
  "key": "archive_archived_blocked",
  "ctx_locale": "es-AR",
  "fallbacks": [{ "from": "es-AR", "to": "en-US" }],
  "expect": { "text": "Cannot reassign an archived customer" }
}
```

### 7.3 Go sync test — negotiation middleware

`runtime/go/lazuli/i18n/negotiate_test.go`:

- Build a runtime with `LocaleContract = { default: "pt-BR",
  supported: ["pt-BR", "en-US"] }`.
- Hit `/anything` with `Accept-Language: en-US,en;q=0.9` → `ctx.Locale
  == "en-US"`.
- Hit `/anything` with `Accept-Language: zh-CN` → `ctx.Locale ==
  "pt-BR"` (fallback to default).
- Hit `/anything` with no Accept-Language → `ctx.Locale == "pt-BR"`.

### 7.4 Doctor fixture — translation coverage incomplete

`crates/lazuli_cli/tests/fixtures/i18n/coverage_incomplete.lzi`:

```lzi
app AcmeCRM
  locale
    default "pt-BR"
    supported "pt-BR", "en-US", "es-AR"

feature customer
  domain
    resource Customer
      id: ID required

  translation
    catalog "./i18n/customer.<locale>.json"

    key archive_blocked
      pt-BR "Não é possível arquivar"
      en-US "Cannot archive"
      # missing es-AR variant
```

Asserts doctor emits exactly one
`translation_locale_coverage_incomplete` diagnostic naming
`es-AR` and key `archive_blocked`.

### 7.5 Doctor fixture — translation key unknown from rule

`crates/lazuli_cli/tests/fixtures/i18n/key_unknown.lzi`:

```lzi
feature customer
  domain
    rule "blocked"
      deny Customer.archive when self.deleted_at != nil
      message @translation.nonexistent_key
```

Asserts doctor emits exactly one `translation_key_unknown` diagnostic
at the `message @translation.nonexistent_key` line.

### 7.6 LSP test — locale source completion

`crates/lazuli_lsp/tests/i18n.rs`:

- Hover on `locale` keyword in `app.lzi` shows the contract summary
  (default, supported, fallbacks).
- Completion at column after `source ` (inside
  `locale_negotiate`) offers exactly: `accept_language`,
  `query_param`, `cookie`, `user_profile`, `subdomain`.
- Completion at column after `strategy ` offers: `best_match`,
  `prefix_match`, `exact_match`.
- Completion at column after `plural ` (inside a translation key)
  offers: `zero`, `one`, `two`, `few`, `many`, `other`.

## Doctor/LSP propostos (Stage 8)

### Diagnostic table

| Code | Severity | Message | Trigger | Test fixture |
|---|---|---|---|---|
| `app_locale_default_not_in_supported` | error | "`app.locale.default` `<X>` must appear in `supported`." | default tag missing from supported list | `default_not_supported.lzi` |
| `app_locale_supported_duplicate` | error | "`app.locale.supported` lists `<X>` twice." | duplicate entries | minimal `.lzi` |
| `app_locale_block_overrides_default_locale` | warning | "`app.locale` block supersedes the bare `default_locale` scalar; remove the scalar to keep one canonical source." | both fields populated | minimal `.lzi` |
| `locale_fallback_tag_unknown` | error | "fallback `<X> -> <Y>`: tag `<Z>` is not in `app.locale.supported`." | unknown tag in fallback | minimal `.lzi` |
| `locale_fallback_cycle` | error | "fallback chain creates a cycle: `<A> -> <B> -> ... -> <A>`." | cycle in fallback graph | minimal `.lzi` |
| `locale_negotiate_source_unknown` | error | "`locale_negotiate.source` `<X>` must be one of: `accept_language`, `query_param`, `cookie`, `user_profile`, `subdomain`." | source not in catalog | minimal `.lzi` |
| `locale_negotiate_strategy_unknown` | error | "`locale_negotiate.strategy` `<X>` must be one of: `best_match`, `prefix_match`, `exact_match`." | strategy not in catalog | minimal `.lzi` |
| `locale_negotiate_fallback_unknown` | error | "`locale_negotiate.fallback` `<X>` must appear in `app.locale.supported`." | fallback tag unknown | minimal `.lzi` |
| `translation_locale_unknown` | error | "translation key `<feature>.<key>` declares a variant for `<X>`, which is not in `app.locale.supported`." | variant locale not in supported | `variant_locale_unknown.lzi` |
| `translation_locale_coverage_incomplete` | error | "translation key `<feature>.<key>` is missing a variant for `<X>` (declared in `app.locale.supported`)." | required locale missing | `coverage_incomplete.lzi` |
| `translation_key_unknown` | error | "`@translation.<key>` does not resolve in feature `<feature>`. Declared keys: `<list>`." | key reference fails to resolve | `key_unknown.lzi` |
| `translation_plural_arm_unknown` | error | "plural arm `<X>` must be a CLDR category: `zero`, `one`, `two`, `few`, `many`, `other`." | arm name invalid | minimal `.lzi` |
| `translation_catalog_missing_locale_token` | warning | "translation catalog path `<X>` should contain a `<locale>` placeholder so the runtime can load per-locale files." | path has no `<locale>` substring | minimal `.lzi` |
| `rule_message_inline_with_multilocale` | warning | "rule `<X>` uses an inline `message` while `app.locale.supported` declares more than one locale. Migrate to `message @translation.<key>` to make the message translatable." | inline + multi-locale | minimal `.lzi` |
| `notification_template_locale_token_with_monolocale` | warning | "notification template path contains `<locale>` but `app.locale.supported` declares only one locale. Drop the token or expand `supported`." | path has `<locale>` + single-locale app | minimal `.lzi` |

15 diagnostics. All register under existing doctor + LSP pipelines.

### LSP hovers (new entries)

Add to `KEYWORD_HOVER`:

| Keyword | Hover summary |
|---|---|
| `locale` (in `app.lzi`) | "App locale contract: `default <tag>`, `supported <tags>`, optional `fallback <src> -> <dst>` edges. Profile-aware." |
| `supported` | "List of BCP-47 tags the app accepts. The negotiation middleware matches `Accept-Language` against this list." |
| `fallback` | "Locale fallback edge. When a translation is missing in the source tag, the runtime walks fallbacks before defaulting to `app.locale.default`." |
| `locale_negotiate` | "Per-runtime-unit (or per-api) middleware that resolves the request locale into `ctx.locale`. Closed catalog for `source` and `strategy`." |
| `translation` (in feature) | "Feature-scoped translation block. Declares a catalog path and typed keys. Each key declares one variant per locale in `app.locale.supported`, plus optional CLDR plural arms." |
| `catalog` | "Catalog path with `<locale>` placeholder. The runtime resolves it per request, e.g. `./i18n/customer.pt-BR.json`." |
| `key` (in translation) | "Translation key. References look like `@translation.<name>` (same feature) or `<feature>.@translation.<name>` (cross-feature)." |
| `plural` | "CLDR plural arm. Closed catalog: `zero`, `one`, `two`, `few`, `many`, `other`. The actual rule for which arm fires is locale data from CLDR, not language-declared." |

### Closed-catalog completions to add

- `app.locale supported ` → no fixed catalog (BCP-47 tags are open),
  but offer common tags as suggestions: `pt-BR`, `en-US`, `es-AR`,
  `es-MX`, `fr-FR`, `de-DE`, `ja-JP`, `zh-CN`.
- `locale_negotiate source ` → `accept_language`, `query_param`,
  `cookie`, `user_profile`, `subdomain`.
- `locale_negotiate strategy ` → `best_match`, `prefix_match`,
  `exact_match`.
- `translation … plural ` → `zero`, `one`, `two`, `few`, `many`,
  `other`.
- `rule … message @translation.` → completion offers authored keys
  from the surrounding feature's `translation` block.

### Namespaces (`is_allowed_reference_namespace`)

**One** new namespace required: **`@translation`**. Reference-only;
applies inside `rule message @translation.<key>` and (post-pilot)
inside surface labels. Add to
`crates/lazuli_lsp/src/lib.rs:2114-2135`.

### Highlighting (`editors/vscode/syntaxes/lazuli.tmLanguage.json`)

Add to keyword scope: `locale`, `supported`, `fallback`,
`locale_negotiate`, `translation`, `catalog`, `key` (inside
translation), `plural`. The catalog literals (`accept_language`,
`query_param`, `cookie`, `user_profile`, `subdomain`, `best_match`,
`prefix_match`, `exact_match`, `zero`, `one`, `two`, `few`, `many`,
`other`) hit existing identifier scope. The `@translation.` prefix
matches the existing `@<namespace>.` scope rule.

## CLI: `lazuli translate extract` (Stage 8.5)

A new subcommand in `crates/lazuli_cli/src/main.rs`. Walks the
package, harvests authored keys, writes catalog stub files.

```text
lazuli translate extract <path> [--out <dir>] [--locale <tag>] [--check]
```

Flags:

- `--out <dir>` (default `./i18n`) — output directory for catalog
  files.
- `--locale <tag>` — extract only one locale's stubs (default: all
  in `app.locale.supported`).
- `--check` — exit non-zero if any key is missing a variant. CI gate.

Sources walked:

1. Every `rule message @translation.<key>` reference → ensures the
   key exists in the feature's `translation` block. If missing,
   reports `translation_key_unknown`.
2. Every `notification template "<path>"` with `<locale>` token →
   one file per locale at `<path>.replace("<locale>", tag)`.
3. Every authored `key <name>` in a `translation` block → ensures
   all `app.locale.supported` tags have variants.

Output format: JSON per feature per locale, matching the catalog
path. The exact JSON schema is a runtime contract, not a language
contract — the CLI invokes a Lazuli Go helper (or, in the absence of
the helper, writes a minimal `{ "<key>": "<text>", ... }` stub).

Idempotent: re-running the CLI never overwrites authored translation
text. Missing variants are emitted as `{ "<key>": "" }` with a
warning, never destructive replacement.

## Critério de "ciclo fechado"

- [ ] Fixture exercises every authored axis: `app.locale` block,
      `locale_negotiate` decorator on `app.runtime unit api`,
      `translation` block on `feature customer` with at least one key
      per supported locale, `rule message @translation.<key>`
      reference, `notification template "./outreach/welcome_email.
      <locale>.mjml"` (extend `examples/full-capsule/app.lzi` +
      `full-capsule.lzi`).
- [ ] `lazuli check examples/full-capsule` accepts the syntax with the
      new blocks.
- [ ] `lazuli inspect --format=json --expand=locale
      examples/full-capsule` shows the `app.locale` block and
      per-feature `translation.keys` summaries.
- [ ] `lazuli inspect --format=json --expand=translations` projects
      the full translation IR with variants and plural arms.
- [ ] `lazuli doctor examples/full-capsule` emits zero new errors on
      the happy-path fixture and exactly the 15 named diagnostics on
      the matching negative fixtures.
- [ ] `lazuli translate extract examples/full-capsule --out
      ./i18n --check` exits with code 0 on the happy-path fixture and
      1 with the expected key list when a coverage gap is introduced.
- [ ] `lazuli generate` produces `dist/go/app/locale.gen.go` and
      per-feature `dist/go/<feature>/translations.gen.go` that compile
      against `runtime/go/lazuli/i18n/`.
- [ ] Lazuli Go runtime mounts the negotiation middleware, resolves
      `ctx.Locale`, loads the per-feature catalogs, renders typed
      keys with ICU MessageFormat + CLDR plural rules.
      **Runtime-team deliverable.**
- [ ] Golden evals + the negotiation `synctest` pass.
- [ ] LSP hovers + completion cover all new keywords + closed
      catalogs from Stage 8.

## Próximo passo

Human approval of this proposal + a separate `mode=implement` run.
Implementation **ordering** matters:

1. **IR extensions first** (all additive): `AppLocale`,
   `LocaleFallback`, `LocaleNegotiate`, `Translation`,
   `TranslationKey`, `TranslationVariant`, `TranslationPluralArm`,
   `Rule.message_ref`.
2. **Parser slice + analyzer** (lower `locale` block on `app`,
   `locale_negotiate` block on runtime unit + api, `translation`
   block on feature + experience).
3. **Doctor + LSP** (15 new diagnostics + 8 new hovers + 1 new
   namespace `@translation`).
4. **Inspect projection** (additive — `app.locale`,
   `translation.keys`, key-coverage report).
5. **CLI** (`lazuli translate extract`).
6. **Codegen** (2 generated artifacts: `locale.gen.go` per app +
   `translations.gen.go` per feature).
7. **Runtime** (parallel Lazuli Go runtime work — 4 new files under
   `runtime/go/lazuli/i18n/`).
8. **Highlighting** + docs (`docs/invariants.md` adds the contracts
   as normative).

The cycle closes when the runtime-team i18n cut lands and the
closed-cycle criterion checklist all green.

## Speculative — deferred until pilot pressure (Cut i18n full chain)

The following axes are catalog noise without a real multi-locale
pilot exercising them. Listed for posterity; **do not implement
ahead of pilot evidence**.

| Axis | Why deferred |
|---|---|
| Surface labels (view `title`, action labels, empty states, validation error display) | §3.7 — fixture has zero authored labels today. Promote when `.lzx` authors at least one. |
| ICU MessageFormat as a *language-level* contract | DF in audit §24. The language declares which keys exist; the runtime parses ICU. Promoting ICU syntax checks into the language adds a parser dependency for no AI-first win. |
| Gender / animacy / case variants beyond plural | Same as ICU. CLDR data drives this; the language doesn't need a surface for it. |
| Locale-specific number/date/currency formatters | DF. Generated handlers already call `i18n.FormatCurrency(...)`; no new language surface needed. |
| Timezone propagation (user + tenant) | §2.18 in roadmap. `default_timezone` exists as L0 (`app.lzi:8`); the bucket should extend `app.locale` symmetrically once a real product authors per-user timezones. |
| Right-to-left text direction | UI-platform concern (React, Expo). Not declarative. |
| Bidi text rendering | Same. |
| Locale-aware sort / collation | Generated handler concern. `golang.org/x/text/collate` is a runtime pickup. |
| TMS sync (Lokalise, Crowdin, Phrase) | DA. Adapters subscribe to `lazuli translate extract` output and push to the TMS via webhook or polling. |
| Machine translation hooks | DA. Same boundary as TMS sync. |
| Translation memory | DA. Same. |
| Per-tenant locale override (different orgs in different locales) | Pilot needed: a multi-tenant SaaS where org A is `pt-BR` and org B is `es-AR`. `ctx.tenant.locale` would be the slot; defer until pressure surfaces. |
| Per-user locale stored on the user resource | Pilot needed. Today's `LocaleNegotiate.source = user_profile` reserves the slot; the resource-side declaration is pre-pilot. |
| Pseudo-locale support for QA (e.g. `en-XA`, `en-XB`) | Pilot needed. Pseudo-locales are a QA tool, not a product feature. |
| Compile-time string-extraction checks (every translatable string must be a `@translation.<key>`) | Strictness gate. Currently soft (warning `rule_message_inline_with_multilocale`); promotion to error gated on pilot evidence that the warning is too easy to ignore. |

Each of these has a clear shape, but the canonical fixture has no
authoring pressure for them today. They wait for a real product
exercising the cycle to settle.

## Rows sugeridas para `docs/next-checklist.md`

Three additions, formatted to match the existing table style
(continuing from row 37):

```
| 38 | i18n bucket cycle — `app.locale` block + `locale_negotiate` decorator | planned | New `AppLocale`/`LocaleFallback`/`LocaleNegotiate` IR structs. `locale` block on `app.lzi` (default/supported/fallback) supersedes bare `default_locale` scalar. `locale_negotiate` on `app.runtime unit` + per-api override (source: accept_language/query_param/cookie/user_profile/subdomain; strategy: best_match/prefix_match/exact_match). 8 doctor diagnostics (`app_locale_*`, `locale_fallback_*`, `locale_negotiate_*`). LSP hovers + closed-catalog completions. Profile-aware. See `docs/proposals/bucket-i18n-cycle.md` §3.1 §3.3 §IR §Doctor/LSP. |
| 39 | i18n bucket cycle — `translation` kind + `@translation.<key>` namespace | planned | New `Translation`/`TranslationKey`/`TranslationVariant`/`TranslationPluralArm` IR structs on `Feature` (and reserved on `Experience` for surface labels post-pilot). `translation` block declares catalog path + typed keys. CLDR plural arms (`zero/one/two/few/many/other`) closed by spec. New `@translation` reference namespace. `Rule.message_ref` extends existing `Rule.message` for typed key references; back-compat preserved. 5 doctor diagnostics (`translation_locale_*`, `translation_key_unknown`, `translation_plural_arm_unknown`, `translation_catalog_missing_locale_token`). 2 warnings (`rule_message_inline_with_multilocale`, `notification_template_locale_token_with_monolocale`). Surface labels in `.lzx` deferred to post-pilot (§3.7). See `docs/proposals/bucket-i18n-cycle.md` §3.2 §3.4 §3.5 §IR §Doctor/LSP. |
| 40 | i18n bucket cycle — `lazuli translate extract` CLI + Lazuli Go runtime | planned | New CLI subcommand walks rule `message @translation.*` refs + notification `template "<path>"` with `<locale>` token + authored `translation` keys; writes per-locale catalog stubs to `<out>/<feature>.<locale>.json`. `--check` flag gates CI. The runtime team owns `runtime/go/lazuli/i18n/` package (4 files: contract.go, negotiate.go, catalog.go, format.go) — locale negotiation middleware (RFC 4647 best-match), per-feature catalog loader, ICU MessageFormat renderer (library TBD), CLDR plural rules via `golang.org/x/text/feature/plural`, locale-aware Time/Date/Number/Currency formatters via `golang.org/x/text/{message,number,currency}`. Adapter slots reserved for Lokalise/Crowdin/Phrase TMS sync (DA, pilot-gated). ICU full surface + gender + per-tenant locale + pseudo-locales deferred to Cut i18n full chain. See `docs/proposals/bucket-i18n-cycle.md` §3.6 §Runtime §Speculative. |
```
