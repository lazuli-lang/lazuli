# Comment hygiene

> Lazuli way idiom. A `# doctor:allow <CODE> — reason "..."` line is a **first-class, highlighted directive**, not throwaway chatter — treat it as load-bearing source.

## Reach for this

When you must suppress a doctor finding you can't (or shouldn't) fix right now, write the canonical directive — `# doctor:allow <CODE> — reason "<text>"` — on the offending line, and **always carry a reason**. It tells `lazuli doctor` (and any human or agent reading the file) *that* a lint was deliberately suppressed and *why*.

Because it has semantic weight, the VSCode grammar colors it distinctly from a plain `# note` (spec 0006). The em dash `—` is canonical; an ASCII `--` is also accepted. A bare `# doctor:allow <CODE>` (no reason tail) still highlights the directive + code but visibly lacks the `reason`/string scopes — that missing color is the at-a-glance cue for an un-reasoned allow (which spec 0007 enforces semantically).

## Before (hand-rolled) / After (idiomatic)

**Before** — the suppression renders in undifferentiated comment gray, indistinguishable from `# TODO` or `# this is a hack`:

```
# doctor:allow VOCAB-TESTS-MISSING-001 — reason "covered by integration suite"
```

**After** (spec 0006) — the same line, now with each token in its own scope so reviewers and agents can spot a newly-added or un-reasoned allow on sight:

| Token | Scope |
| --- | --- |
| `#` | `punctuation.definition.comment.lazuli` |
| `doctor:allow` | `keyword.control.directive.lazuli` |
| `<CODE>` (`VOCAB-TESTS-MISSING-001`) | `entity.name.tag.lazuli` |
| `reason` | `keyword.control.directive.reason.lazuli` |
| `"<text>"` | `string.quoted.double.lazuli` |

A plain comment (`# just a note`) is unchanged — one flat `comment.line.number-sign.lazuli` scope. This mirrors how `scope override` blocks already dignify their `reason` text.

## Enforced by

- **`test:grammar:lazuli:check`** (snapshot gate) — the grammar regression check, fixture `editors/vscode/tests/grammar/comments.lzi`, fires on any regression of the directive scoping (e.g. a future edit reordering the `comments` patterns so the `#.*$` catch-all eats the directive).

<!-- 0007 co-fills the two semantic rule rows below (e.g. DOCTOR-ALLOW-NO-REASON-001) — append, do not clobber the 0006 content above. -->

