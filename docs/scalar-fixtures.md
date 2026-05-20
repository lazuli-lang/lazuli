# Scalar Fixtures

## TL;DR

Scalar fixtures are plugin-owned generators for `@semantic.*` values.
Codegen discovers them from configured plugins and imports each plugin's
`./fixtures` export when generated tests need valid or invalid scalar values.
Doctor checks discovery and provider shape before generated tests rely on them.

## Contract

Plugins expose fixture providers that satisfy the `ScalarFixtureProvider<T>`
interface from `@lazuli/runtime/scalars`:

```ts
export interface ScalarFixtureProvider<T = string> {
  // Produce a fresh valid value of the type. Every plugin must implement this.
  generate(): T;
  // Optional batch helper. Callers can derive a default from generate().
  generateMany?(n: number): T[];
  // Optional stable canonical value for snapshots and examples.
  readonly example?: T;
  // Optional generator for a value that fails validation.
  invalid?(): T;
}
```

`generate()` is the only required member. Codegen may derive `generateMany(n)`
by calling `generate()` repeatedly when the provider omits it.

## Plugin export convention

The npm package must publish a `./fixtures` export next to its main runtime
entrypoint:

```json
{
  "exports": {
    "./fixtures": {
      "types": "./dist/fixtures.d.ts",
      "default": "./dist/fixtures.js"
    }
  }
}
```

The module should export a named `fixtures` map:

```ts
import type { ScalarFixtures } from '@lazuli/runtime/scalars';

export const fixtures: ScalarFixtures = {
  MySemanticType: { generate: () => 'valid-value' },
};
```

## Reference example

`@plugin/scalars-br` at
`c:/Users/lucas/dev/lazuli-plugin-scalars-br/` is the reference plugin.

Its `src/fixtures.ts` currently mirrors the runtime contract locally and
exports `fixtures: ScalarFixtures`. The map keys match the plugin manifest's
semantic type names:

```ts
export const fixtures: ScalarFixtures = {
  BrazilianCPF: { generate: () => generateBrazilianCPF(), example: '111.444.777-35', invalid: () => '000.000.000-00' },
  BrazilianCNPJ: { generate: () => generateBrazilianCNPJ(), example: '11.222.333/0001-81', invalid: () => '00.000.000/0000-00' },
  BrazilianCEP: { generate: () => generateBrazilianCEP(), example: '01310-100', invalid: () => '0000-0000' },
  BrazilianPhone: { generate: () => generateBrazilianPhone(), example: '11987654321', invalid: () => '+55-not-a-phone' },
};
```

Each provider supplies `generate()` for valid data, `example` for stable
snapshots/docs, and `invalid()` for negative validation tests. None of the
Brazilian providers need a custom `generateMany()`.

## Discovery

Codegen reads the app's `Lazurite.toml [plugins]` block to find active plugins:

```toml
[plugins]
"@plugin/scalars-br" = { module = "github.com/lazuli-lang/lazuli-plugin-scalars-br", version = "v0.1.0" }
```

For each plugin, codegen reads the plugin's `manifest.toml` and its
`[[semantic_types]]` entries:

```toml
[[semantic_types]]
name = "BrazilianCPF"
alias = "@semantic.BrazilianCPF"
carrier_type = "String"
validator = "ValidateCPF"
formatter = "FormatCPF"
```

The `name` is the fixture-map key. The `alias` is the Lazuli source-level
semantic reference. Generated tests import the package's `./fixtures` module
only when a reachable field or command input uses one of those aliases.

## Doctor diagnostics

| Code | Trigger | Resolution |
|---|---|---|
| `SCALAR-FIXTURES-001` | A configured plugin declares `[[semantic_types]]` but its package has no resolvable `./fixtures` export. | Add the package export or remove scalar fixture generation for that plugin. |
| `SCALAR-FIXTURES-002` | The `fixtures` map is missing a provider for a manifest `semantic_types.name`. | Add the missing key with at least `generate()`. |
| `SCALAR-FIXTURES-003` | A provider does not satisfy `ScalarFixtureProvider<T>` shape. | Fix `generate`, `generateMany`, `example`, or `invalid` to match the runtime contract. |
