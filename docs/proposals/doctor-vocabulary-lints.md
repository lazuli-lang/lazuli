# Doctor Vocabulary Lint Rules

**Status:** PASS 9.0/10 (graded 2026-05-14)
**Scope:** `crates/lazuli_cli/src/doctor/vocab/` — vocabulary-level diagnostics distinct from the
structural/auth/migration families already in `doctor.rs`.

---

## Motivation

The Lazuli DSL encodes intent through vocabulary. When authors approximate a vocabulary concept
using a workaround, the code compiles and the doctor passes, but the IR diverges from its intended
shape. Over time these divergences accumulate into schema drift, harder migrations, and less
accurate codegen.

Vocabulary lints are a separate doctor family (`VOCAB-*`) that detect when a well-known DSL
construct (`union`, `enum`, `record`) would be a better fit than the current spelling, and suggest
the refactor verbatim.

---

## File layout

```
crates/lazuli_cli/src/doctor/
  vocab/
    mod.rs              # Registry stub — exports each rule as pub mod + re-exports check fn
    vocab_union_001.rs  # VOCAB-UNION-001
```

The `doctor.rs` module declares `pub mod vocab;`; the orchestrator wires each rule's `check`
function into `DoctorPackage::diagnostics()` post-merge.

---

## Diagnostic shape

Terminal output follows the existing doctor format:

```
features/payment/payment.lzi:1:1: warning [VOCAB-UNION-001]: resource `Payment` declares enum field `kind` plus 2 optional field(s) (bridge_fee, bridge_url) only meaningful for one tag — consider a discriminated `union` type
```

Severity: **warning** in `default` and `strict` profiles. Not emitted in `permissive` profile
(author is explicitly opting out of vocab guidance).

---

## VOCAB-UNION-001: enum + correlated-optional-fields drift

### Problem

```lzi
enum PaymentKind
  Bridge
  Direct

resource Payment
  kind:       PaymentKind
  amount:     Decimal
  bridge_fee: Decimal?       # only meaningful when kind = Bridge
  bridge_url: Text?          # only meaningful when kind = Bridge
```

A `resource` that carries tag-correlated optional fields is a discriminated union in disguise.
The Lazuli `union` type (when available) expresses this precisely, enabling exhaustive codegen
dispatch and schema pruning per tag.

### Suggested refactor

```lzi
union Payment
  Bridge
    amount:    Decimal
    bridge_fee: Decimal
    bridge_url: Text
  Direct
    amount:    Decimal
```

### Detection heuristic (v0.1)

A resource fires VOCAB-UNION-001 when it declares:

1. **An `enum`-typed field** (TypeRef::EnumRef — the "kind" axis).
2. **≥1 optional field** (required == false) in the same resource.
3. **That optional field's name carries a tag-prefix** matching a variant name of the enum type
   (heuristic: field name starts with `<variant.to_lowercase()>_`).

Detection signals shipped in v0.1:

- **(b) Inline pragma** `# only-when kind=<tag>` adjacent to the field — currently not
  represented in the IR; deferred to a future source-map pass. Note this in the rule body; do
  not implement.
- **(c) Name-convention heuristic** — field name prefix matches a lowercased enum variant name
  followed by `_`. This is the primary detection path for v0.1.

Signal **(a)** (handler-graph branch analysis) is deferred; the doctor does not have
handler-function IR yet.

### False-positive guards

| Guard | Rule |
|---|---|
| (i) Optional field with no prefix match to any variant | Do NOT emit. A `notes: Text?` field alongside `status: ShipStatus` is universally optional. |
| (ii) Enum type not resolved in the current feature | Skip the resource (cross-feature enum refs are out of scope for the IR-local walk). |
| (iii) No optional fields in the resource | Do NOT emit. |

### Severity per profile

| Profile | Severity |
|---|---|
| default | warning |
| strict | warning |
| permissive | suppressed |

### Acceptance criteria

- `cargo check --all-targets` passes.
- `cargo test -p lazuli_cli --lib doctor::vocab::vocab_union_001` passes.
- File ≤ 200 effective LOC (target: ~100-150).
- Diagnostic output matches the verbatim shape in §"Diagnostic shape".
- Inline tests: ≥1 positive + 2 negative (false-positive guards i and iii).
