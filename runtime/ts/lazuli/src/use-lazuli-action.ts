import type { UseMutationResult } from "@tanstack/react-query";
import { useCallback, useContext, useMemo } from "react";

import { LazuliFlashAdapterContext, type FlashKind } from "./flash-adapter.js";
import { LazuliRouterAdapterContext } from "./router-adapter.js";
import type { CommandSpec } from "./spec.js";

export type UseLazuliActionOptions<I, O> = {
  mapInput?: (raw: I) => I;
  invalidates?: string[];
  redirect?: string | ((out: O) => string);
  back?: boolean;
  replace?: boolean;
  flash?: { kind: FlashKind; messageKey: string };
  onSuccess?: (out: O, input: I) => void;
  onError?: (err: unknown, input: I) => void;
  throwOnError?: boolean;
};

export type UseLazuliActionResult<I, O> = {
  mutate: (input: I) => Promise<O>;
  mutateAsync: (input: I) => Promise<O>;
  isPending: boolean;
  error: unknown;
  reset: () => void;
};

type UseLazuliCommandHook = <I, O>(
  spec: CommandSpec<I, O>,
  options?: {
    onSuccess?: (out: O, input: I, context: unknown) => unknown;
  },
) => UseMutationResult<O, Error, I>;

export function createUseLazuliAction(useLazuliCommand: UseLazuliCommandHook) {
  return function useLazuliAction<I, O>(
    spec: CommandSpec<I, O>,
    options: UseLazuliActionOptions<I, O> = {},
  ): UseLazuliActionResult<I, O> {
    // Wave §1 (2026-05-23): mutually-exclusive navigation outcomes.
    // `redirect` and `back` are different navigation directions; firing
    // both is undefined behavior. `replace` only makes sense paired
    // with `redirect` (it modifies the navigation history mode).
    // The check runs once per render in dev — production builds strip
    // it via the NODE_ENV guard.
    assertNavigationExclusivity(spec.name, options);

    const router = useContext(LazuliRouterAdapterContext);
    const flash = useContext(LazuliFlashAdapterContext);
    const extraInvalidates = options.invalidates;

    const mergedSpec = useMemo<CommandSpec<I, O>>(() => {
      if (!extraInvalidates || extraInvalidates.length === 0) return spec;
      return {
        ...spec,
        invalidates: [...new Set([...spec.invalidates, ...extraInvalidates])],
      };
    }, [extraInvalidates, spec]);

    const successFlash = options.flash;
    const redirect = options.redirect;
    const replace = options.replace;
    const back = options.back;
    const onSuccess = options.onSuccess;
    const actionOnSuccess = useCallback(
      async (out: O, input: I) => {
        if (redirect !== undefined) {
          const raw = typeof redirect === "function" ? redirect(out) : redirect;
          // Wave A.2.2 — string `redirect` may contain `{result.X}`
          // placeholders that interpolate from the command output. Matches
          // the declarative `on_success / redirect "/X/{result.id}"` block
          // emitted by `.lzx` codegen (Wave A.8). Function-form `redirect`
          // bypasses interpolation since the caller already controls the
          // output access.
          const to =
            typeof redirect === "function" ? raw : interpolateResultPath(raw, out);
          const navOptions = replace === undefined ? undefined : { replace };
          router.navigate(to, navOptions);
        }
        if (back) {
          router.back();
        }
        if (successFlash) {
          flash.show(successFlash.kind, successFlash.messageKey);
        }
        await onSuccess?.(out, input);
      },
      [back, flash, onSuccess, redirect, replace, router, successFlash],
    );

    const command = useLazuliCommand<I, O>(mergedSpec, { onSuccess: actionOnSuccess });
    const mapInput = options.mapInput;
    const onError = options.onError;
    // Wave §1 (2026-05-23): throw by default. `onError` observes but
    // does not silence; explicit `throwOnError: false` is the only
    // way to swallow. The previous default (silently absorb) hid real
    // bugs — handlers that wanted to fail loudly had to remember to
    // opt in; handlers that wanted to swallow now have to opt in
    // explicitly (the safer asymmetry).
    const throwOnError = options.throwOnError ?? true;
    const mutateAsync = command.mutateAsync;
    const run = useCallback(
      async (input: I): Promise<O> => {
        const mapped = mapInput ? mapInput(input) : input;
        try {
          return await mutateAsync(mapped);
        } catch (err) {
          onError?.(err, mapped);
          if (throwOnError) {
            throw err;
          }
          return undefined as O;
        }
      },
      [mapInput, mutateAsync, onError, throwOnError],
    );

    return {
      mutate: run,
      mutateAsync: run,
      isPending: command.isPending,
      error: command.error,
      reset: command.reset,
    };
  };
}

/**
 * Wave §1 — fail loudly when the action options declare contradictory
 * navigation outcomes. `redirect` and `back` are different nav
 * directions; declaring both is undefined behavior in the current
 * implementation (both fire, observable order). `replace` is a
 * modifier for `redirect` only — declaring it without `redirect` is a
 * no-op that hints at a typo / leftover from a refactor.
 *
 * Throws in development so the bug shows up before the action ever
 * runs. Production builds (NODE_ENV=production) skip the check — by
 * then any drift has had its chance to be caught by tests + dev
 * environments.
 */
function assertNavigationExclusivity<I, O>(
  specName: string,
  options: UseLazuliActionOptions<I, O>,
): void {
  if (typeof process !== "undefined" && process.env?.NODE_ENV === "production") {
    return;
  }
  if (options.redirect !== undefined && options.back) {
    throw new Error(
      `useLazuliAction(${specName}): \`redirect\` and \`back\` are mutually exclusive; declaring both is undefined behavior. Pick one navigation outcome.`,
    );
  }
  if (options.replace && options.redirect === undefined) {
    throw new Error(
      `useLazuliAction(${specName}): \`replace\` requires \`redirect\` (it modifies the navigation history mode). Either add \`redirect\` or drop \`replace\`.`,
    );
  }
}

/**
 * Interpolate `{result.X}` / `{result.X.Y}` placeholders in a path
 * template against the command's output object. Undefined / null fields
 * substitute as empty strings; substituted values are URL-encoded so a
 * value like `"foo/bar"` doesn't escape its path segment.
 *
 * Templates without placeholders return verbatim — common case is free.
 */
export function interpolateResultPath(template: string, result: unknown): string {
  if (!template.includes("{result.")) return template;
  return template.replace(/\{result\.([A-Za-z0-9_.]+)\}/g, (_match, path: string) => {
    const value = readResultPath(result, path);
    return value === undefined || value === null ? "" : encodeURIComponent(String(value));
  });
}

function readResultPath(value: unknown, path: string): unknown {
  let current: unknown = value;
  for (const segment of path.split(".")) {
    if (current === null || typeof current !== "object") {
      return undefined;
    }
    current = (current as Record<string, unknown>)[segment];
  }
  return current;
}
