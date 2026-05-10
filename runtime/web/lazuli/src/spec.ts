// Typed spec objects that generated `dist/web/<feature>/<feature>.gen.ts`
// produces, one per command and query. The runtime client and React hooks
// consume these specs to know the canonical name + which queries to
// invalidate post-mutation, while keeping input/output types attached.
//
// `Input` and `Output` are phantom-typed: they only exist at compile time
// so callers can't pass the wrong shape. At runtime the spec is a plain
// JSON-serialisable record.

export interface CommandSpec<Input, Output> {
  readonly kind: "command";
  readonly name: string;
  readonly invalidates: readonly string[];
  // brand keeps the type parameters from being erased to `unknown`.
  readonly _input?: Input;
  readonly _output?: Output;
}

export interface QuerySpec<Args, Result> {
  readonly kind: "query";
  readonly name: string;
  readonly _args?: Args;
  readonly _result?: Result;
}

export interface DefineCommandOptions {
  readonly invalidates?: readonly string[];
}

export function defineCommand<Input, Output>(
  name: string,
  options: DefineCommandOptions = {},
): CommandSpec<Input, Output> {
  return {
    kind: "command",
    name,
    invalidates: options.invalidates ?? [],
  };
}

export function defineQuery<Args, Result>(
  name: string,
): QuerySpec<Args, Result> {
  return {
    kind: "query",
    name,
  };
}
