# useLazuliForm + RHF adapter (`@lazuli/runtime/react/rhf` subpath)

## What

Build the M.2 prereq: a React Hook Form adapter that bridges the headless `useLazuliForm` (Wave A.3) to apps that use RHF's `<Controller>` pattern with UI primitives (`<Input>`, `<Select>`, `<DatePicker>` consuming `field.value` / `field.onChange` / `fieldState.error`).

This unblocks the hostpoint settings panel migration (14 panels can collapse from ~150 LoC to ~50 LoC each).

## Where

- Repo: `c:/Users/lucas/lazuli` (worktree `c:/tmp/migrate-rhf-adapter`)
- New file: `runtime/ts/lazuli/src/react-rhf.ts` (subpath entry)
- Update `runtime/ts/lazuli/package.json` exports to add `./react/rhf` subpath
- Spec: `c:/Users/lucas/lazuli-ops/docs/proposals/hostpoint-migration-backlog-2026-05-22.md` §M.2 unblock spec

## Why

`useLazuliForm` exposes `{values, setValue, setValues, reset, handleSubmit, isDirty, isSubmitting, error, serverErrors}` — but RHF apps want `register` + `Controller` integration. The adapter exposes both surfaces.

## How

Signature:

```ts
import { useForm, type UseFormProps } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import type { ZodSchema } from "zod";
import type { CommandSpec, QuerySpec } from "@lazuli/runtime";

export type UseLazuliFormRHFOptions<Values, SubmitInput, SubmitOutput> = {
  defaults: Values;
  hydrateFrom?: QuerySpec<unknown, Partial<Values>>;
  submit: CommandSpec<SubmitInput, SubmitOutput>;
  mapValuesToInput: (values: Values) => SubmitInput;
  schema?: ZodSchema<Values>;
  rhfOptions?: Omit<UseFormProps<Values>, "defaultValues" | "resolver">;
  onSuccess?: (out: SubmitOutput, values: Values) => void;
  onError?: (err: unknown, values: Values) => void;
};

export function useLazuliFormRHF<Values, Input, Output>(
  opts: UseLazuliFormRHFOptions<Values, Input, Output>
): {
  /** Pass to <Controller>, useFieldArray, watch, register, etc. */
  control: ReturnType<typeof useForm<Values>>["control"];
  formState: ReturnType<typeof useForm<Values>>["formState"];
  /** Wired submit handler (calls submit command + onSuccess/onError). */
  onSubmit: () => Promise<void>;
  isSubmitting: boolean;
  /** Raw error from the last submit (LazuliError or unknown). */
  submitError: unknown;
  /** Reset to defaults (or to a partial override). */
  reset: (next?: Partial<Values>) => void;
};
```

Implementation:

1. Construct `useForm` from RHF with `defaultValues: opts.defaults` and `resolver: opts.schema ? zodResolver(opts.schema) : undefined`.
2. Construct `useLazuliQuery(opts.hydrateFrom, {})` when present. On data, call `reset(mergedValues)` once (use a ref to prevent re-hydration after user typing).
3. Construct `useLazuliCommand(opts.submit)` for the mutation.
4. `onSubmit` = `handleSubmit(async (values) => { try { await mutateAsync(mapValuesToInput(values)); opts.onSuccess?.(out, values); } catch (err) { /* server validation_failed → setError per field; otherwise opts.onError */ } })`.
5. Server validation: if `LazuliError.code === 'validation_failed'` and `error.data?.fields` is `{<fieldName>: <message>}`, call `setError(fieldName, { type: 'server', message })` for each.

## Test plan

Tests in `runtime/ts/lazuli/src/react-rhf.test.tsx`:
- defaults populate `useForm` correctly
- hydrateFrom query → `reset(data)` fires once
- subsequent query refetches don't overwrite user-typed values
- onSubmit calls command with mapped input
- onSuccess fires post-mutation
- validation_failed with field errors → `formState.errors[field]` populated
- reset(partial) overrides
- typecheck via test fixture using generic shape

## Constraints

- **RHF + zod-resolver are peer deps** — add to `runtime/ts/lazuli/package.json` peerDependencies. Apps already have them; runtime doesn't bundle.
- **Subpath isolated** — `@lazuli/runtime/react/rhf` is the import. Don't pollute `@lazuli/runtime/react` core.
- **No new top-level namespace.**

## Acceptance

- `runtime/ts/lazuli/src/react-rhf.ts` created.
- `package.json` exports map includes `./react/rhf`.
- `pnpm test` adds N new test cases (suggest ≥6).
- Sample callsite in STATUS.md showing hostpoint's `HostPersonal.tsx` (149 LoC) collapsed to ~50 LoC using the adapter.
- `STATUS.md` at worktree root.

## Commit message template

```
runtime: @lazuli/runtime/react/rhf — useLazuliFormRHF adapter

Bridges Wave A.3's headless useLazuliForm to apps using react-hook-form
with <Controller> bindings to UI primitives. Unblocks M.2 settings
panel migration (each panel: useForm + zodResolver + manual hydrate +
manual save + manual error → useLazuliFormRHF in ~3 lines).

- New subpath: @lazuli/runtime/react/rhf
- RHF + zod-resolver as peer deps
- Server validation_failed → setError per field
- Hydrate-once semantics (ref-guarded)

N tests cover defaults/hydrate/submit/serverErrors/reset paths.
```
