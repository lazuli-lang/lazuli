# Lazuli for VS Code

Language support for the [Lazuli](https://github.com/lazuli-lang/lazuli) AI-first DSL — `.lzi` (feature capsules), `.lzx` (experiences/surfaces), and `Lazurite.toml` (project manifest).

## Features

### Syntax highlighting

- **Three distinct file icons** so `.lzi`, `.lzx`, and `Lazurite.toml` pop in the file tree (lapis-lazuli blue / citrine gold / rose-quartz pink).
- Context-aware grammar (~70 named-block kinds) for every Lazuli construct — declarations, sub-blocks, statement keywords, modifiers, type catalogs (primitive / UI / extension / domain), decorators (`@policy.X`, `@scope.X`, `@fn.X`, `@cap.X`, `@semantic.X`, etc.), HTTP method constants, closed-catalog values (cookie `same_site`, headers, encryption, auth, digest, on_delete cascade rules…), expression operators (`@>`, `<@`, `?|`, `?&`, `matches`, `when`, `between`).
- Reference paths split: known roots like `ctx` / `input` / `output` / `payload` / `route` get a distinct context accent; `.tenant.id` after them colors as a property chain.
- Model paths (`Customer.ID`, `Item.tags`) split into type root + member chain.
- Theme-friendly scope names — verified against Default Dark+/Light+/Dark Modern/Light Modern/One Dark Pro/Atom One Dark/Monokai/Solarized.
- Lazurite.toml gets a TOML overlay highlighting the 9 known table headers + 24 known keys + `@plugin/X` module references distinctly from arbitrary user TOML.

### Language server (LSP)

Bundles the Lazuli language server (`lazuli lsp`) so you get **live**:

- **Diagnostics** — error / warning / hint squiggles for everything `lazuli doctor` would catch.
- **Hover** — keyword docs sourced from the framework's authoritative catalog.
- **Completion** — closed-catalog completion for keyword children.
- File-local lints for app blocks, headers, cookies, secret rotation, audit, cache, etc.

The bundled binary is matched to the framework version; it auto-detects in this order:

1. User setting `lazuli.lspPath` (explicit override — point at your local dev build)
2. Bundled `server/lazuli.exe` (shipped in the .vsix)
3. `lazuli` on `PATH`

### Snippets

25 ready-to-fill snippets for the common patterns:

- Top-level kinds: `feature`, `app`, `registry`, `workspace`, `profile`
- Block kinds: `resource`, `record`, `enum`, `querylist`, `querylookup`, `command`, `api`, `view`, `webhook`, `job`, `agent`, `notification`, `route`
- Sub-blocks: `policies`, `emits`, `route_slot`, `field`, `audit`, `ratelimit`, `policy_ref`

Type the prefix (e.g. `command`) and press `Tab` to expand into a working skeleton with placeholder tabstops + closed-catalog choice picks.

## Settings

| Setting | Default | Description |
|---|---|---|
| `lazuli.lspPath` | `""` | Path to the lazuli executable used as the language server. When empty, falls back to bundled binary, then `PATH`. |
| `lazuli.trace.server` | `"off"` | Trace LSP messages between VS Code and the lazuli server (`off` / `messages` / `verbose`). |

## Requirements

- VS Code `^1.90.0`
- The bundled language server is Windows-x64 only in this release. macOS / Linux users should set `lazuli.lspPath` to a local `lazuli` build (`cargo install lazuli_cli`).

## Known limitations

- L0 #6 view-body grammar (terminal cells / drawer / search-segmented / sort / selection / bulk_actions / settings) is fully colored, but the LSP file-local diagnostics for these constructs are still being rolled out.
- SQL inside `query.sql` is referenced via `sql "./path.sql"` (external file) — those `.sql` files get standard SQL highlighting from VS Code's bundled grammar; there is no inline triple-quoted SQL form in Lazuli today.

## Reporting issues

Please file issues at <https://github.com/lazuli-lang/lazuli/issues> with:

- Extension version (Settings → Extensions → Lazuli → "About this extension")
- A minimal `.lzi` snippet that reproduces the problem
- The output of "Developer: Inspect Editor Tokens and Scopes" (Ctrl+Shift+P) for the misbehaving token

## License

MIT — see `LICENSE`.

---

*This extension is part of the Lazuli framework — a 2026 design experiment in AI-first software DSLs (declarative authoring, narrow-but-deep vocabulary, generated thin Go runtime + thin TypeScript SDK). See the [main repository](https://github.com/lazuli-lang/lazuli) for details.*
