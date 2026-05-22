# Cell A.8 Status

Commit hash at implementation time: `e4be3bb`

## Edit Sites

- Parser/AST: `crates/lazuli_syntax/src/parser.rs`, `crates/lazuli_syntax/src/ast.rs`
- IR: `crates/lazuli_ir/src/lib.rs`
- Analyzer lowering: `crates/lazuli_analyzer/src/lib.rs`
- TS codegen: `crates/lazuli_codegen_ts/src/lzx.rs`, `crates/lazuli_codegen_ts/src/lzx_view_create.rs`, `crates/lazuli_codegen_ts/src/slot_interface.rs`
- TS runtime hook bridge: `runtime/ts/lazuli/src/client.ts`, `runtime/ts/lazuli/src/index.ts`, `runtime/ts/lazuli/src/react.ts`, `runtime/ts/lazuli/src/react.web.ts`, `runtime/ts/lazuli/src/react.native.ts`, `runtime/ts/lazuli/src/exports-parity.test.ts`

## Implementation Summary

- `view create` bodies now accept one `on_success` child block.
- Supported clauses are `back`, `redirect "<path>"`, `flash <success|error|info> @translation.<key>`, `invalidates query.<name>`, and `replace`.
- The parser stores redirect strings opaquely, including `{result.X}` placeholders.
- `ViewCreate` lowers to `on_success: Option<OnSuccessSpec>` with `FlashSpec` and reused `InvalidatesSpec`.
- Generated create-view hooks now call `useLazuliAction`, merging declarative defaults before user options so user `opts` remain last-write-wins.
- The TS runtime exports `useLazuliAction`; web/native implementations handle flash, invalidation, redirect interpolation, and history back orchestration.

## Sample Translation

Authored `.lzx`:

```lzx
view create thing_create
  submit thing.command.create
  fields title
  on_success
    back
    flash success @translation.saved
    invalidates query.lookup_my_thing
```

Generated TS shape:

```ts
export const viewerThingCreateView = {
  submit: createThing,
  schema: CreateThingInputSchema,
  fields: ["title"],
  cells: {},
  onSuccess: {
    back: true,
    flash: { kind: "success", messageKey: "saved" },
    invalidates: ["thing.lookup_my_thing"],
  },
} as const;

export function useViewerThingCreateView(
  opts?: UseLazuliActionOptionsFor<typeof viewerThingCreateView.submit>,
) {
  const submit = useLazuliAction(viewerThingCreateView.submit, {
    ...viewerThingCreateView.onSuccess,
    ...opts,
  });
}
```

## Tests

- `cargo test -p lazuli_syntax -p lazuli_analyzer -p lazuli_cli` passed.
  - `lazuli_syntax`: 290 unit tests + 1 doc test.
  - `lazuli_analyzer`: 227 unit tests + 2 integration tests.
  - `lazuli_cli`: 34 lib tests + 643 bin tests + 11 integration tests.
- `cargo test -p lazuli_ir -p lazuli_codegen_ts` passed.
  - `lazuli_ir`: 51 unit tests + 30 integration tests.
  - `lazuli_codegen_ts`: 207 unit tests + 13 integration tests.
- `git diff --check` passed.

TS runtime typecheck was not completed because `runtime/ts/lazuli` has no installed `node_modules` in this worktree; `pnpm --dir runtime/ts/lazuli typecheck` reports that `tsc` is not available.
