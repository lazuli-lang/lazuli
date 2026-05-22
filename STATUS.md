# Cell A.2.1 Status

Implementation commit: `bb89ca85b1b8a51494a5f5499e87b9146c45403a`

Files changed:

- `runtime/ts/lazuli/src/message-for-error.ts`
- `runtime/ts/lazuli/src/message-for-error.test.ts`
- `runtime/ts/lazuli/src/index.ts`
- `STATUS.md`

Tests:

- Added 6 unit tests in `runtime/ts/lazuli/src/message-for-error.test.ts`
- `pnpm test` from `runtime/ts/lazuli`: 10 files passed, 49 tests passed
- `pnpm typecheck` from `runtime/ts/lazuli`: passed

Sample callsite:

```ts
import { messageForLazuliError } from "@lazuli/runtime";

export function formatLazuliError(err: unknown, t: Translator): string {
  const message = messageForLazuliError(err);
  return t(message.key, message.params) ?? message.fallback;
}
```
