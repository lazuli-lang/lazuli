# Wave A.7 Status

Implementation commit hash: `bd27711`

## Edit sites

- Codegen emitter: `crates/lazuli_codegen_ts/src/lzx_route_params.rs`
- Codegen registration: `crates/lazuli_codegen_ts/src/lib.rs`
- CLI artifact wiring: `crates/lazuli_cli/src/main.rs`
- Codegen golden: `crates/lazuli_codegen_ts/tests/golden/route-params/host.routes.gen.ts`
- Runtime hook: `runtime/ts/lazuli/src/use-lazuli-route-params.ts`
- Runtime exports: `runtime/ts/lazuli/src/react.ts`, `react.web.ts`, `react.native.ts`, `exports-parity.test.ts`
- Runtime scalar alias: `runtime/ts/lazuli/src/types.ts`, `index.ts`

## Tests

- Codegen golden: 1 mixed route-param parser golden.
- Runtime hook: 3 tests covering valid params, invalid throw, invalid redirect via router adapter.
- Export parity: 2 tests covering contracted web runtime exports.
- `cargo test -p lazuli_codegen_ts`: passed, 220 tests.
- `cargo test -p lazuli_cli`: passed.
- `pnpm --dir runtime/ts/lazuli typecheck`: passed.
- `pnpm --dir runtime/ts/lazuli test`: passed, 46 tests.

## Sample generated parser

```ts
export interface HostServiceEditParams {
  propertyId: ID;
  serviceId: ID;
  kind: ServiceKind;
  startsAt: Date;
  day: Date;
}

export function parseHostServiceEditParams(raw: Record<string, string>): HostServiceEditParams | null {
  const propertyId = parseId(raw.property_id); if (propertyId == null) return null;
  const serviceId = parseId(raw.service_id); if (serviceId == null) return null;
  const kind = SERVICE_KIND_VALUES.includes((raw.kind ?? "") as ServiceKind) ? raw.kind as ServiceKind : null; if (kind == null) return null;
  const startsAt = new Date(raw.starts_at ?? ""); if (Number.isNaN(startsAt.getTime())) return null;
  const day = new Date(raw.day ?? ""); if (Number.isNaN(day.getTime())) return null;
```

## Pilot ergonomics

Before:

```ts
const propertyId = Number(params.property_id) as unknown as ID;
const serviceId = tryID(params.service_id);
if (serviceId == null) navigate("/host");
```

After:

```ts
const { propertyId, serviceId } = useLazuliRouteParams(
  "host_service_edit",
  parseHostServiceEditParams,
  router.match.params,
  { redirectOnInvalid: "/host" },
);
```
