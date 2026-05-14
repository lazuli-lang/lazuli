# Proposal — Design Tokens (`design.lzi`)

**Status:** L0 v0.2 PASS @ 9.31/10 — 2026-05-14 (v0.1 graded 8.79 PASS-with-notes via `lazuli-language-architect`; v0.2 applied 4 blockers + 5 polish items inline; re-grade confirmed 9.31, no blockers. All 8 dimensions ≥ 9.0. 4 deferable polish items tracked in L2 cells.)
**Author:** Claude Opus 4.7 (orchestrator)
**Audit-ready target:** ≥ 9.0 via `lazuli-language-architect`
**Depends on:** `docs/proposals/lazurite-frontend-folder-canon.md` (L0 #1 — defines `design.lzi` location at project root)
**Honors:** `docs/invariants.md:14-15` (boundary discipline); `docs/design-principles.md` Rule Zero (Vocabulary Over Mechanism)

---

## §1. Status & motivation

The frontend story for Lazuli (per `docs/proposals/lazurite-frontend-folder-canon.md`) leaves a gap: **how does a Lazurite product declare its visual primitives** — colors, typography, spacing, radius, shadow, motion, breakpoints, z-index — so they emit consistently across web, mobile, email templates, and design tools (Figma)?

Today the answer is "user writes a Tailwind config by hand, hopes mobile copies the values, no Figma round-trip". Three problems:

1. **Drift**: Tailwind config + RN StyleSheet + email CSS + Figma library all hand-maintained. Updating brand color = 4 edits in 4 stacks, easy to miss one.
2. **AI-hostile**: an LLM authoring UI cannot read a custom Tailwind config and know what colors/spacings the product allows. It guesses `bg-violet-650` (doesn't exist) or hardcodes hex.
3. **No Doctor rule** can fire "use token, not raw hex" if there are no declared tokens to compare against. The token catalog must be authored canonically before enforcement is possible.

This proposal introduces `design.lzi` — a project-root declarative tokens catalog peer to `app.lzi` / `registry.lzi`. Lazuli emits Tailwind preset / CSS variables / RN tokens / Figma JSON from a single source. Doctor enforces "use declared tokens" via comparison against the catalog.

**Why now:** Pleiades web ships next. Atelier and Erudito follow. Three products without a shared token contract will diverge in week one; converging post-divergence is harder than starting canonical.

**Boundary discipline:** This proposal defines token *vocabulary* — closed catalog of declarative atoms. It does NOT define component shape, layout rules, animation choreography, or theme switching strategy. Per the guiding principle:

> Lazurite owns structure and glue; product code owns interaction and rendering.

Tokens are the *atomic glue* between design intent and rendered UI. The rendering itself (components, layout, motion choreography) stays in product code.

---

## §2. Scope

### In scope

1. **Top-level `design.lzi`** at project root, peer to `app.lzi` / `registry.lzi`.
2. **Eight canonical token groups**: `color`, `typography`, `space`, `radius`, `shadow`, `motion`, `breakpoint`, `z`. Each group is a closed catalog of constraints.
3. **Semantic grouping inside groups** (typography has `family` / `scale` / `weight` / `tracking`; motion has `duration` / `easing`; color has variant sub-blocks for hover/active/foreground).
4. **Theme variants** via `dark` suffix on color tokens (e.g. `base "#ffffff" dark "#09090b"`). Single variant axis in v0; multi-axis (`light/dark/high-contrast`) deferred.
5. **Brand variants** via `design <brand> extends <base>` — single-line inheritance with token-level override. Cut B-gated (post-pilot).
6. **Core cross-target emitters** (always shipped, no plugin needed):
   - `tokens.ts` — typed const + type aliases (`ColorToken`, `SpaceToken`, etc.)
   - `tokens.css` — CSS variables with `[data-theme="dark"]` override block
   - `tailwind.gen.ts` — Tailwind v3 preset
   - `tailwind.theme.css` — Tailwind v4 `@theme { ... }` block
   - mobile-specific `tokens.ts` — RN-shaped (px numbers, no rem)
7. **Plugin emitters** (`@plugin/design-<target>`) — opt-in:
   - `@plugin/design-figma` — W3C Design Tokens JSON (Figma Tokens Studio round-trip)
   - `@plugin/design-style-dictionary` — Amazon Style Dictionary source format
   - `@plugin/design-panda` — Panda CSS config tokens slice
   - `@plugin/design-vanilla-extract` — vanilla-extract `themeContract.css.ts`
   - `@plugin/design-tamagui` — Tamagui tokens config
   - `@plugin/design-restyle` — Shopify Restyle theme (RN)
8. **Doctor rules**: `design-token-undefined`, `design-token-hex-leak`, `design-token-px-leak`. Severity escalates from `prototype` (warning) to `production` (error).
9. **Import/export round-trip**: `lazuli design import --from <path>` lifts Figma JSON / Style Dictionary into `design.lzi`. `lazuli design export --target figma --out <path>` reverses.
10. **`lazurite.toml [design]` configuration** for target selection (`tailwind-v3` vs `tailwind-v4`, mobile yes/no, plugin emitters opt-in).

### Non-goals

1. **Component primitives** — no `button`, `card`, `dialog` token entries. Components live in `app/ui/` (per L0 #1 §4.2).
2. **Layout systems** — no `grid`, `flex`, or `container` tokens. CSS strategy is user choice.
3. **Animation choreography** — `duration` and `easing` are token primitives; named animation sequences (e.g., `slide-in-from-right`) are not.
4. **Theme switching mechanism** — `data-theme` attribute approach is documented but the React hook for switching is **NOT** emitted by `design.lzi`; user implements `useTheme()` in `app/theme/theme_provider.tsx`.
5. **Multi-theme axes beyond `dark`** — `high-contrast`, `compact`, `cozy`, custom user-named themes — deferred. v0 ships only `light/dark`.
6. **Sub-token composition** — `primary.hover derived from primary.base shade(-10)` — deferred to a future cut. v0 requires explicit hex per state.
7. **Token granularity per component** — `button.primary.hover.shadow` — never. Components consume base tokens; component-specific tokens are an anti-pattern (`docs/design-principles.md` §"Self-Contained Declarations").
8. **CSS-in-JS style strategy** — Tailwind / vanilla-extract / Panda / styled-components — emitter choice via `lazurite.toml`, not core opinion.
9. **Locale-aware tokens** (RTL spacing, language-specific font stacks) — deferred. v0 is single-direction LTR.

---

## §3. Canonical `design.lzi` shape

```lazuli
design pleiades

  color
    primary
      base "#7c3aed"
      hover "#6d28d9"
      active "#5b21b6"
      foreground "#ffffff"

    secondary
      base "#0891b2"
      hover "#0e7490"
      foreground "#ffffff"

    background
      base "#ffffff" dark "#09090b"
      muted "#f4f4f5" dark "#18181b"
      subtle "#e4e4e7" dark "#27272a"

    foreground
      base "#09090b" dark "#fafafa"
      muted "#71717a" dark "#a1a1aa"
      subtle "#a1a1aa" dark "#71717a"

    success "#16a34a"
    warning "#ea580c"
    danger  "#dc2626"
    info    "#0891b2"

  typography
    family
      sans "Inter, system-ui, sans-serif"
      mono "JetBrains Mono, monospace"
      serif "Charter, serif"

    scale
      xs    size 0.75rem,  line_height 1rem
      sm    size 0.875rem, line_height 1.25rem
      base  size 1rem,     line_height 1.5rem
      lg    size 1.125rem, line_height 1.75rem
      xl    size 1.25rem,  line_height 1.75rem
      "2xl" size 1.5rem,   line_height 2rem
      "3xl" size 1.875rem, line_height 2.25rem
      "4xl" size 2.25rem,  line_height 2.5rem

    weight
      regular 400
      medium 500
      semibold 600
      bold 700

    tracking
      tight -0.025em
      normal 0
      wide 0.025em

  space
    "1" 0.25rem
    "2" 0.5rem
    "3" 0.75rem
    "4" 1rem
    "6" 1.5rem
    "8" 2rem
    "12" 3rem
    "16" 4rem
    "24" 6rem

  radius
    sm 0.125rem
    base 0.25rem
    md 0.375rem
    lg 0.5rem
    xl 0.75rem
    full 9999px

  shadow
    sm "0 1px 2px 0 rgb(0 0 0 / 0.05)"
    base "0 1px 3px 0 rgb(0 0 0 / 0.1)"
    md "0 4px 6px -1px rgb(0 0 0 / 0.1)"
    lg "0 10px 15px -3px rgb(0 0 0 / 0.1)"
    xl "0 20px 25px -5px rgb(0 0 0 / 0.1)"

  motion
    duration
      fast 150ms
      base 200ms
      slow 350ms

    easing
      in "cubic-bezier(0.4, 0, 1, 1)"
      out "cubic-bezier(0, 0, 0.2, 1)"
      in_out "cubic-bezier(0.4, 0, 0.2, 1)"

  breakpoint
    sm 640px
    md 768px
    lg 1024px
    xl 1280px
    "2xl" 1536px

  z
    docked 10
    dropdown 1000
    sticky 1100
    modal 1300
    toast 1500
```

### §3.1 Closed catalog of token groups

**Lexical rule** (normative): token names that start with a digit (`"2xl"`, `"3xl"`, `"16"`) MUST be quoted as STRING tokens. Lazuli's `IDENT_LOWER` lexer rejects digit-leading idents (`docs/grammar.lzi.md:58`), so the quote is required to land as a name, not a number. Names not starting with a digit (`xs`, `sm`, `base`, `primary`) are unquoted idents.

| Group | Sub-groups | Token shape | Purpose |
|---|---|---|---|
| `color` | (none — semantic names) | hex string OR `{ base, hover, active, foreground }` sub-block; optional `dark <hex>` per value | Brand + semantic + surface colors |
| `typography` | `family`, `scale`, `weight`, `tracking` | per sub-group: name + value (font stack / size+line_height / number / em offset) | Type system |
| `space` | (none) | name → rem value | Spacing scale |
| `radius` | (none) | name → rem value | Border radius scale |
| `shadow` | (none) | name → CSS box-shadow string (quoted) | Elevation scale |
| `motion` | `duration`, `easing` | per sub-group: name → ms / cubic-bezier string | Transitions + animations |
| `breakpoint` | (none) | name → px value | Responsive cutoffs |
| `z` | (none) | name → integer | Stacking order scale |

**Eight top-level groups.** All closed. Adding a ninth requires Lazuli core proposal — distros and products cannot extend the catalog.

### §3.2 Color variant sub-block

A color token can be either a single hex (`success "#16a34a"`) or a sub-block with named states. The closed state catalog:

| State | Required? | Meaning |
|---|---|---|
| `base` | Yes | Default state |
| `hover` | Optional | Mouse hover, touch press start |
| `active` | Optional | Mouse down, touch press end |
| `foreground` | Optional | Text/icon color when this color is the background |

**No other state names allowed.** Adding `disabled`, `focus`, `selected`, etc. is a separate L0 proposal — pilot evidence required. Per Rule Zero, opening the catalog before evidence creates dialect drift.

### §3.3 `dark` modifier

Any color value (top-level or sub-block) may carry a `dark <hex>` suffix:

```lazuli
background
  base "#ffffff" dark "#09090b"
  muted "#f4f4f5" dark "#18181b"

danger "#dc2626"   # no dark variant → same in both themes
```

Emission:
- CSS: `--color-background-base: #ffffff;` in `:root`; `--color-background-base: #09090b;` in `[data-theme="dark"]`.
- Tailwind: Tailwind's `darkMode: ["class", '[data-theme="dark"]']` (v3) or `@variant dark` (v4) picks up the CSS-var-backed colors.
- RN: `tokens.color.background.base.light` and `.dark` — user-side theme provider switches.

### §3.4 Typography sub-groups

`typography` is the only group with rich sub-grouping. The four sub-groups are closed:

```
typography
  family    {sans, serif, mono, ...}     # font stacks
  scale     {xs, sm, base, lg, ...}      # size + line_height paired (Tailwind-default ergonomics)
  weight    {regular, medium, semibold, bold, ...}
  tracking  {tight, normal, wide, ...}   # letter-spacing
```

`scale` entries are typed-pair: `size <rem>, line_height <rem|number>`. Tailwind v3+ supports this two-value `fontSize` shape natively. The pairing is intentional — designers think in size+leading combos (text-base IS a baked size+leading), so the token reflects that.

### §3.5 Motion sub-groups

```
motion
  duration  {fast, base, slow, ...}   # ms values
  easing    {in, out, in_out, ...}    # cubic-bezier strings (quoted) or named curves
```

Named easing curves (CSS built-ins: `ease`, `ease-in`, `linear`, `step-start`) accepted as unquoted identifier. Cubic-bezier is quoted because it contains commas.

### §3.6 Brand variants (post-pilot, Cut B — keyword reserved in v0)

For white-label / multi-tenant scenarios, future shape:

```lazuli
# themes/hostpoint.lzi (Cut B, NOT v0)
design hostpoint
  extends pleiades              # inherit all tokens
  color
    primary
      base "#10b981"            # override only
      hover "#059669"
    # other primary states inherited
```

Inheritance is **token-level**, not block-level: `primary.base` and `primary.hover` override, `primary.active` and `primary.foreground` inherit from `pleiades`. No deep merging beyond named-key override.

`lazurite.toml` selects active theme:
```toml
[design]
active = "pleiades"          # or "hostpoint" for white-label build
```

**v0 parser behavior** (normative): the `extends` keyword is **reserved** by the v0 grammar. A `design <X>` block declaring `extends <Y>` parses as legal syntax but the lowering rejects with diagnostic `DESIGN-EXTENDS-CUT-B`: "Theme inheritance via `extends` ships in Cut B (post-pilot). For v0, declare a standalone `design <X>` block with full token values." Reserving the keyword prevents Cut B from being a breaking change to v0 grammar — the syntax surface is fixed; only the lowering opens up.

`themes/<name>.lzi` directory path is also reserved by the v0 doctor — files there parse as legal locations but doctor warns `DESIGN-THEME-DIR-CUT-B` if any non-comment content exists. Reserving the directory prevents path-shape churn when Cut B lands.

Out of v0 implementation scope — the keyword + path are pre-paved so Cut B is additive.

---

## §4. Cross-target emitters

### §4.1 Core emitters (always)

Generated paths follow L0 #1 §4 dist layout (which canonicalizes both `dist/ts-web/` and `dist/ts-mobile/` as peer per-frontend dist roots; see `docs/proposals/lazurite-frontend-folder-canon.md` §4 tree, "GENERATED" subtree):

```
dist/ts-web/design/
├── tokens.ts                  # typed const + type aliases
├── tokens.css                 # CSS variables + dark override
├── tailwind.gen.ts            # Tailwind v3 preset (when target="tailwind-v3")
├── tailwind.theme.css         # Tailwind v4 @theme block (when target="tailwind-v4")
└── allowlist.json             # closed enum of legal Tailwind classes — Doctor reads for `design-token-undefined`

dist/ts-mobile/design/
└── tokens.ts                  # RN-shaped (px numbers); only emitted when [frontends.<mobile-target>] declared
```

The `allowlist.json` artifact is the canonical source for "which Tailwind classes does this product allow"; Doctor `design-token-undefined` consults it (§6.1 detail). Emitter writes both human-readable artifacts (preset/theme.css) and the machine-checkable allowlist as a paired emission.

### §4.2 `tokens.ts` — typed runtime const

```typescript
// Code generated by lazuli; DO NOT EDIT.
export const tokens = {
  color: {
    primary: {
      base: "#7c3aed",
      hover: "#6d28d9",
      active: "#5b21b6",
      foreground: "#ffffff",
    },
    background: {
      base: { light: "#ffffff", dark: "#09090b" },
      muted: { light: "#f4f4f5", dark: "#18181b" },
    },
    success: "#16a34a",
  },
  typography: {
    family: {
      sans: "Inter, system-ui, sans-serif",
      mono: "JetBrains Mono, monospace",
    },
    scale: {
      xs:   { size: "0.75rem",  lineHeight: "1rem" },
      base: { size: "1rem",     lineHeight: "1.5rem" },
    },
    weight: { regular: 400, medium: 500, semibold: 600, bold: 700 },
  },
  space: { "1": "0.25rem", "2": "0.5rem", "4": "1rem" },
  radius: { sm: "0.125rem", base: "0.25rem", md: "0.375rem" },
  shadow: { sm: "0 1px 2px 0 rgb(0 0 0 / 0.05)" },
  motion: {
    duration: { fast: "150ms", base: "200ms", slow: "350ms" },
    easing: { out: "cubic-bezier(0, 0, 0.2, 1)" },
  },
  breakpoint: { sm: "640px", md: "768px" },
  z: { docked: 10, modal: 1300 },
} as const;

export type ColorToken = keyof typeof tokens.color;
export type SpaceToken = keyof typeof tokens.space;
export type RadiusToken = keyof typeof tokens.radius;
export type ShadowToken = keyof typeof tokens.shadow;
export type FontFamilyToken = keyof typeof tokens.typography.family;
export type FontWeightToken = keyof typeof tokens.typography.weight;
export type TextScaleToken = keyof typeof tokens.typography.scale;
export type MotionDurationToken = keyof typeof tokens.motion.duration;
export type MotionEasingToken = keyof typeof tokens.motion.easing;
export type BreakpointToken = keyof typeof tokens.breakpoint;
export type ZToken = keyof typeof tokens.z;
```

User code consuming:
```typescript
import { tokens, type ColorToken } from "@/dist/ts-web/design/tokens";

function Box({ bg }: { bg: ColorToken }) {
  return <div style={{ backgroundColor: tokens.color[bg].base }} />;
}
```

### §4.3 `tokens.css` — CSS variables

```css
/* Code generated by lazuli; DO NOT EDIT. */
:root {
  --color-primary-base: #7c3aed;
  --color-primary-hover: #6d28d9;
  --color-primary-active: #5b21b6;
  --color-primary-foreground: #ffffff;

  --color-background-base: #ffffff;
  --color-background-muted: #f4f4f5;

  --color-success: #16a34a;

  --font-sans: "Inter, system-ui, sans-serif";
  --font-mono: "JetBrains Mono, monospace";

  --text-xs: 0.75rem;
  --text-xs--line-height: 1rem;
  --text-base: 1rem;
  --text-base--line-height: 1.5rem;

  --space-1: 0.25rem;
  --space-4: 1rem;

  --radius-base: 0.25rem;

  --shadow-base: 0 1px 3px 0 rgb(0 0 0 / 0.1);

  --duration-base: 200ms;

  --z-modal: 1300;
}

[data-theme="dark"] {
  --color-background-base: #09090b;
  --color-background-muted: #18181b;
  --color-foreground-base: #fafafa;
  --color-foreground-muted: #a1a1aa;
}
```

Naming convention: `--<group>-<name>` (or `--<group>-<name>-<state>`). Kebab-case in CSS to match Tailwind v4 conventions.

### §4.4 `tailwind.gen.ts` — Tailwind v3 preset

```typescript
// Code generated by lazuli; DO NOT EDIT.
import type { Config } from "tailwindcss";

export const lazuliPreset: Partial<Config> = {
  darkMode: ["class", '[data-theme="dark"]'],
  theme: {
    extend: {
      colors: {
        primary: {
          DEFAULT: "var(--color-primary-base)",
          hover: "var(--color-primary-hover)",
          active: "var(--color-primary-active)",
          foreground: "var(--color-primary-foreground)",
        },
        background: {
          DEFAULT: "var(--color-background-base)",
          muted: "var(--color-background-muted)",
        },
        success: "var(--color-success)",
      },
      fontFamily: {
        sans: "var(--font-sans)",
        mono: "var(--font-mono)",
      },
      fontSize: {
        xs:   ["0.75rem",  { lineHeight: "1rem" }],
        base: ["1rem",     { lineHeight: "1.5rem" }],
      },
      fontWeight: {
        regular: "400",
        medium: "500",
        semibold: "600",
        bold: "700",
      },
      spacing: {
        "1": "0.25rem",
        "4": "1rem",
      },
      borderRadius: {
        sm: "0.125rem",
        DEFAULT: "0.25rem",
        md: "0.375rem",
      },
      boxShadow: {
        sm: "0 1px 2px 0 rgb(0 0 0 / 0.05)",
      },
      transitionDuration: {
        fast: "150ms",
        DEFAULT: "200ms",
        slow: "350ms",
      },
      transitionTimingFunction: {
        out: "cubic-bezier(0, 0, 0.2, 1)",
      },
      screens: {
        sm: "640px",
        md: "768px",
      },
      zIndex: {
        docked: "10",
        modal: "1300",
      },
    },
  },
};
```

User `tailwind.config.ts`:
```typescript
import { lazuliPreset } from "@/dist/ts-web/design/tailwind.gen";

export default {
  presets: [lazuliPreset],
  content: ["./features/**/*.tsx", "./app/**/*.tsx"],
};
```

Colors backed by CSS variables (`var(--color-...)`) so dark mode swaps work via `data-theme` without re-parsing Tailwind classes.

### §4.5 `tailwind.theme.css` — Tailwind v4

Syntax targets Tailwind v4 stable (released 2025-01). The `@theme` block + `--<group>-<name>` CSS-variable-as-theme-token form is the canonical v4 model. The paired `--text-<size>` + `--text-<size>--line-height` shape is per the v4 docs at release; **Cell B (the emitter) MUST re-verify against the latest stable Tailwind v4 docs at implementation time** since `--text-*--line-height` evolved during v4 beta and may have shifted in stable. If shifted, Cell B updates emission; the proposal's intent (one CSS-var-backed @theme block consuming the same tokens as v3 preset) is unchanged.

```css
/* Code generated by lazuli; DO NOT EDIT. */
@import "./tokens.css";        /* loads CSS variables */

@theme {
  --color-primary: var(--color-primary-base);
  --color-primary-hover: var(--color-primary-hover);
  --color-primary-foreground: var(--color-primary-foreground);

  --color-background: var(--color-background-base);
  --color-background-muted: var(--color-background-muted);

  --color-success: var(--color-success);

  --font-sans: var(--font-sans);
  --font-mono: var(--font-mono);

  --text-base: var(--text-base);
  --text-base--line-height: var(--text-base--line-height);

  --spacing-1: var(--space-1);
  --spacing-4: var(--space-4);

  --radius-md: var(--radius-md);

  --shadow-base: var(--shadow-base);

  --duration-base: var(--duration-base);

  --breakpoint-sm: 640px;
  --breakpoint-md: 768px;

  --z-modal: var(--z-modal);
}
```

Tailwind v4 reads `@theme` directly as the source of utility classes. User `globals.css`:

```css
@import "tailwindcss";
@import "@/dist/ts-web/design/tailwind.theme.css";
```

No `tailwind.config.ts` needed for v4. `lazurite.toml [design] target = "tailwind-v4"` switches emission. Tailwind v4 is the recommended default; v3 emitter stays for projects on v3.

### §4.6 Mobile `tokens.ts` (RN/Expo)

```typescript
// Code generated by lazuli; DO NOT EDIT.
// React Native shape — px numbers, no rem; line-heights as numbers.

export const tokens = {
  color: {
    primary: {
      base: "#7c3aed",
      hover: "#6d28d9",
      active: "#5b21b6",
      foreground: "#ffffff",
    },
    background: {
      base: { light: "#ffffff", dark: "#09090b" },
      muted: { light: "#f4f4f5", dark: "#18181b" },
    },
  },
  typography: {
    family: {
      sans: "Inter",       // RN resolves system font fallback
      mono: "JetBrainsMono-Regular",
    },
    scale: {
      xs:   { fontSize: 12, lineHeight: 16 },
      base: { fontSize: 16, lineHeight: 24 },
    },
    weight: { regular: "400", medium: "500", semibold: "600", bold: "700" },
  },
  space: { "1": 4, "2": 8, "4": 16 },         // 1rem = 16px
  radius: { sm: 2, base: 4, md: 6 },
  shadow: {
    base: {
      shadowColor: "#000",
      shadowOffset: { width: 0, height: 1 },
      shadowOpacity: 0.1,
      shadowRadius: 3,
      elevation: 2,                            // Android
    },
  },
  motion: {
    duration: { fast: 150, base: 200, slow: 350 },   // ms numbers
    easing: { out: "ease-out" },                     // RN limited easing
  },
} as const;
```

Conversion rules from `design.lzi`:
- `rem` → `px` (1rem = 16px assumed; configurable via `lazurite.toml [design] rem_base = 16`).
- **`shadow` mobile emission**: the CSS string in `design.lzi` follows the closed single-layer grammar `<offset-x> <offset-y> <blur-radius> [<spread-radius>] <color>` (e.g. `"0 1px 3px 0 rgb(0 0 0 / 0.1)"`). Multi-layer shadows (`"0 1px 2px ..., 0 4px 6px ..."`) are **rejected at lowering** (`DESIGN-SHADOW-MULTI-LAYER`) — declare separate tokens (`shadow.elevated_outer`, `shadow.elevated_inner`) and compose at component level. The closed grammar deterministically maps to RN `{ shadowColor, shadowOffset: { width, height }, shadowOpacity, shadowRadius, elevation }`.
- `cubic-bezier(...)` CSS → RN `Easing.bezier(a, b, c, d)` (caller adapts; the token in `design.lzi` stays in CSS cubic-bezier string form).
- `breakpoint` and `z` not emitted to mobile (RN dimensions are runtime, not media queries; z is `zIndex` on a per-component basis).

### §4.7 Plugin emitters

Opt-in via `lazurite.toml [plugins]`:

```toml
[plugins]
"@plugin/design-figma" = { module = "github.com/lazuli-lang/lazuli-plugin-design-figma", version = "v0.1.0" }
"@plugin/design-panda" = { module = "github.com/lazuli-lang/lazuli-plugin-design-panda", version = "v0.1.0" }
```

Plugin contract:
- Receives the lowered `Design` IR slice.
- Emits one or more files under `dist/ts-web/design/<plugin>/`.
- Filename + content determined by the plugin.

Out of L0 #2 scope: plugin implementations themselves. The plugin protocol is documented in `docs/plugin-authoring.md`; this L0 only declares which plugins are *expected* to exist.

---

## §5. `lazurite.toml [design]` configuration

```toml
[design]
target = "tailwind-v4"        # or "tailwind-v3", "vanilla-css" (only tokens.css), "panda" (via plugin)
mobile = true                 # emit dist/ts-mobile/design/tokens.ts
rem_base = 16                 # rem→px conversion for mobile
plugins = ["@plugin/design-figma"]    # opt-in plugin emitters
emit_check_dark = true        # emit [data-theme="dark"] override block
```

Defaults: `target = "tailwind-v4"`, `mobile = false` (only emit when at least one mobile frontend declared), `rem_base = 16`, `plugins = []`, `emit_check_dark = true`.

---

## §6. Doctor rules

### §6.1 Token enforcement

| Code | Trigger | Severity | Resolution |
|---|---|---|---|
| `design-token-undefined` | `.tsx` uses Tailwind utility class not present in `dist/ts-web/design/allowlist.json` (e.g. `bg-purple-500` when `purple` isn't declared in `design.lzi`) | warning in strict, error in production | Use a declared token or extend `design.lzi`. Doctor reads `allowlist.json` (emitted by Cell B alongside the preset) — does NOT re-parse the Tailwind preset itself. |
| `design-token-hex-leak` | Hex literal in `.tsx` style prop or inline class (`bg-[#7c3aed]`, `style={{ color: "#7c3aed" }}`) | warning in strict, error in production | Define the token in `design.lzi`, use `bg-primary` / `tokens.color.primary.base` |
| `design-token-px-leak` | `px`/`rem`/`em` literal in style prop (`style={{ padding: "12px" }}`) | warning | Use `p-3` / `tokens.space["3"]` |
| `design-token-fontfamily-leak` | `fontFamily` string in style prop not matching a declared family | warning in strict, error in production | Reference declared family or add to `typography.family` |
| `design-token-shadow-leak` | `box-shadow` string literal in style prop | warning | Use `shadow-md` / `tokens.shadow.md` |

### §6.2 Catalog hygiene

| Code | Trigger | Severity | Resolution |
|---|---|---|---|
| `design-token-unused` | Declared token referenced by zero call-sites across `features/**/*.tsx` + `app/**/*.tsx` | info | Remove the token or document why it's reserved |
| `design-token-duplicate-value` | Two tokens have the exact same hex/value (e.g. `success "#16a34a"` and `green "#16a34a"`) | info | Consolidate to one token; aliases via `extends` (Cut B) when needed. Info-level pre-pilot since aliasing/dedup mechanism arrives in Cut B; nagging today is premature. |
| `design-token-missing-dark` | Color token has no `dark` variant when at least one other token in the same group does | info | Decide: intentional (same in both themes) or oversight. Suppress via `# lazuli-allow: design-token-missing-dark — same in both themes` |

### §6.3 Escape hatch

For genuinely one-off design needs (a one-time marketing splash, a third-party widget needing a specific color):

```typescript
// lazuli-allow: design-token-hex-leak — third-party Mapbox marker; outside design system
<MapMarker color="#FF6B35" />
```

Inline comment with rule code + reason. Doctor suppresses for that one line.

---

## §7. Import / export round-trip

### §7.1 `lazuli design import`

```bash
lazuli design import --from app/design/tokens.figma.json
lazuli design import --from app/design/tokens.sd.json --format style-dictionary
```

Reads external token catalog, writes/overwrites `design.lzi`. Supported formats:
- Figma Tokens Studio (W3C Design Tokens spec) — core.
- Style Dictionary JSON source format — via `@plugin/design-style-dictionary`.

Conflict resolution: existing `design.lzi` tokens take precedence on key collision unless `--overwrite` flag passed. Diff printed to stderr.

### §7.2 `lazuli design export`

```bash
lazuli design export --target figma --out app/design/tokens.figma.json
lazuli design export --target style-dictionary --out app/design/tokens.sd.json
```

Reads `design.lzi`, writes external format. For Figma: emits W3C Design Tokens JSON consumable by Tokens Studio. Round-trip property: `lazuli design export --target figma | lazuli design import --from -` is a no-op (modulo formatting).

### §7.3 `lazuli design diff`

```bash
lazuli design diff --against app/design/tokens.figma.json
```

Compares external catalog to `design.lzi`. Output lists tokens missing in either direction, value changes, and schema changes. Useful when designer updates Figma library and dev imports to sync.

---

## §8. Examples

### §8.1 Pleiades brand

```lazuli
design pleiades
  color
    primary
      base "#7c3aed"
      hover "#6d28d9"
      foreground "#ffffff"
    background
      base "#ffffff" dark "#09090b"
      muted "#f4f4f5" dark "#18181b"
    foreground
      base "#09090b" dark "#fafafa"
      muted "#71717a" dark "#a1a1aa"
    success "#16a34a"
    warning "#ea580c"
    danger  "#dc2626"

  typography
    family
      sans "Inter, system-ui, sans-serif"
      mono "JetBrains Mono, monospace"
    scale
      sm    size 0.875rem, line_height 1.25rem
      base  size 1rem,     line_height 1.5rem
      lg    size 1.125rem, line_height 1.75rem
      xl    size 1.25rem,  line_height 1.75rem
      "2xl" size 1.5rem,   line_height 2rem
    weight
      regular 400
      medium 500
      semibold 600
      bold 700

  space
    "1" 0.25rem
    "2" 0.5rem
    "3" 0.75rem
    "4" 1rem
    "6" 1.5rem
    "8" 2rem

  radius
    sm 0.125rem
    base 0.25rem
    md 0.375rem
    lg 0.5rem

  shadow
    sm "0 1px 2px 0 rgb(0 0 0 / 0.05)"
    base "0 1px 3px 0 rgb(0 0 0 / 0.1)"
    md "0 4px 6px -1px rgb(0 0 0 / 0.1)"

  motion
    duration
      fast 150ms
      base 200ms
    easing
      out "cubic-bezier(0, 0, 0.2, 1)"

  breakpoint
    sm 640px
    md 768px
    lg 1024px

  z
    dropdown 1000
    modal 1300
```

### §8.2 React component consuming tokens (valid)

```tsx
import { tokens } from "@/dist/ts-web/design/tokens";

export function SlugBadge({ count }: { count: number }) {
  return (
    <span className="bg-primary text-primary-foreground rounded-md px-2 py-0.5 text-sm font-medium">
      {count}
    </span>
  );
}
```

All classes resolve through the Tailwind preset. Doctor passes.

### §8.3 Doctor failure example

```tsx
// ❌ Three doctor warnings
export function BadButton() {
  return (
    <button
      className="bg-purple-500"                  // design-token-undefined
      style={{
        color: "#7c3aed",                        // design-token-hex-leak
        padding: "12px",                         // design-token-px-leak
      }}
    >
      Click me
    </button>
  );
}

// ✅ Fixed
export function GoodButton() {
  return (
    <button className="bg-primary text-primary-foreground p-3">
      Click me
    </button>
  );
}
```

---

## §9. Open questions / Future work

### §9.1 Custom state variants

v0 ships `base/hover/active/foreground` only. `disabled`, `focus`, `selected`, `loading` — pilot evidence required before adding to the closed catalog. Workaround: declare separate colors (`primary_disabled "#9ca3af"`).

### §9.2 Token aliasing within `design.lzi`

```lazuli
# Hypothetical syntax — NOT v0
color
  primary
    base ref(blue.600)
  blue
    "600" "#7c3aed"
```

Defer. Requires reference-resolver in the IR + cycle detection. Pilot evidence needed.

### §9.3 Token transformations

`primary.hover derived from primary.base shade(-10)` — programmatic color manipulation. Defer; this is "designers want a single source for color palettes" territory. Workaround today: emit hex explicitly per state (which IS the W3C Design Tokens spec recommendation — tokens are atomic, transformations live in the design tool).

### §9.4 RTL-aware spacing

`space.start` / `space.end` instead of `space.left` / `space.right` — but Lazuli's `space` is direction-agnostic by design (single numeric scale). CSS logical properties (`margin-inline-start`) work automatically with the current emission. Future: explicit RTL token group if locale-specific needs surface.

### §9.5 Per-component token granularity

`button.primary.hover.shadow` — rejected (see §2 non-goals). Component-specific tokens are an anti-pattern: a `Button.shadow` token leaks component implementation into the design system. Components consume base tokens; component behavior lives in `app/ui/button.tsx`.

### §9.6 Multi-theme axes

`high-contrast`, `compact/cozy` density variants, accessibility themes — defer. Single `light/dark` axis covers 95% of v0 use cases.

### §9.7 Token versioning

When `design.lzi` adds a token, downstream `dist/ts-web/design/tokens.ts` adds a key — semver minor. When `design.lzi` removes a token, breaking. Out of v0; relies on `lazuli changelog` (existing) once design tokens land in inspect output.

### §9.8 Plugin emitter spec

Each `@plugin/design-<target>` plugin needs a stable contract (input IR slice, output file conventions). The contract spec is part of the L2 implementation cell, not this L0.

---

## §10. References

- `docs/proposals/lazurite-frontend-folder-canon.md` — defines `design.lzi` location and `app/theme/` consumption point.
- `docs/proposals/lzx-integration-codegen.md` (L0 #3, pending) — consumes design tokens in audience-scoped SDK projections.
- `docs/design-principles.md` — Rule Zero (Vocabulary Over Mechanism), Self-Contained Declarations.
- `docs/invariants.md` — closed catalog discipline.
- `docs/architecture.md` §"Lazuli vs Lazurite" — framework/distro boundary.
- W3C Design Tokens Community Group spec — Figma JSON format.
- Amazon Style Dictionary — historic precedent for cross-target token compilation.
- Tailwind CSS v3 + v4 documentation — emission target reference.
- Memory: `project_lazuli_drusa_philosophy.md`, `project_plugin_namespace_policy.md`, `feedback_grade_before_commit.md`, `pleiades-buildable-session-2026-05-14`.

---

## §11. Acceptance criteria

L0 PASS condition: the proposal answers, for any Lazurite-shaped product, the following deterministically:

1. **Where do design tokens live?** → `design.lzi` at project root.
2. **What's the closed catalog of token groups?** → eight: `color`, `typography`, `space`, `radius`, `shadow`, `motion`, `breakpoint`, `z` (§3.1).
3. **How does dark mode work?** → `dark <hex>` suffix per color value; emits `[data-theme="dark"]` CSS override; user-side theme provider switches the attribute (§3.3, non-goal §2).
4. **How does a React component use a token?** → import `tokens` from `dist/ts-web/design/tokens.ts` OR Tailwind class via preset (§8.2).
5. **What does Doctor do when a user writes `style={{ color: "#7c3aed" }}`?** → emits `design-token-hex-leak` (§6.1).
6. **How does Tailwind v3 vs v4 work?** → `lazurite.toml [design] target = "tailwind-v4"` (default) emits `@theme` CSS; `target = "tailwind-v3"` emits `lazuliPreset` JS (§4.4, §4.5).
7. **How does Figma sync?** → `@plugin/design-figma` (opt-in) emits W3C Design Tokens JSON; `lazuli design import/export` round-trips (§7).
8. **What's the mobile emission?** → `dist/ts-mobile/design/tokens.ts` with px numbers, RN-shaped shadow objects, easing-name strings (§4.6).

If all eight answers are mechanical from the proposal text, L0 #2 passes.

L2 implementation cells (post-PASS):
- **Cell A**: `design.lzi` parser + lowering to IR (~8 token groups, semantic sub-blocks, dark suffix).
- **Cell B**: Core emitters — `tokens.ts`, `tokens.css`, `tailwind.gen.ts`, `tailwind.theme.css`, mobile `tokens.ts`.
- **Cell C**: Doctor rules `design-token-undefined`, `design-token-hex-leak`, `design-token-px-leak` (path + import + AST scan).
- **Cell D**: `lazuli design import` / `export` / `diff` subcommands.
- **Cell E**: Plugin protocol documentation in `docs/plugin-authoring.md` for `@plugin/design-<target>`.
- **Cell F** (post-pilot): `extends <base>` for brand variants (Cut B).
- **Cell G** (post-pilot): plugin implementations themselves — `@plugin/design-figma`, `@plugin/design-style-dictionary`, etc. Each in its own repo, separate L0.
