# `lazuli doctor` exit codes

Wave 2.2 of the TDD/BDD-first proposal (2026-05-23) makes the doctor exit
code the load-bearing signal for agent and CI workflows. The matrix below
is the contract; any change is breaking and requires a proposal.

## Exit code matrix

| Code | Meaning                                                                 |
|------|-------------------------------------------------------------------------|
| 0    | Pass — no findings matched the failure gate                             |
| 1    | Findings matched the failure gate (rule fired at or above threshold)    |
| 2    | Parse error or IR construction error — the pipeline could not run      |
| 3    | Internal error (panic, unexpected I/O failure, malformed manifest)      |

Today exit codes `2` and `3` are surfaced indirectly via `anyhow` propagation
(`cargo run` bubbles up the chain). Treating them as a stable surface is a
follow-up cell.

## `--fail-on` flag

`--fail-on` is composable and accepts three shapes:

```text
--fail-on <severity>
--fail-on category:<RuleCategory>
--fail-on rule:<RULE-CODE>
```

Multiple `--fail-on` values combine with **OR**: any matching finding gates
the exit code to `1`.

### Severity values

`error`, `warning`, `info`, `hint`.

### Category values

Accepts both `PascalCase` (`TestDiscipline`, the canonical JSON form) and
`snake_case` (`test_discipline`). Known categories (see
`crates/lazuli_doctor/src/rule_category.rs`):

- `Vocabulary`
- `Correctness`
- `Security`
- `TestDiscipline`
- `Design`
- `Encryption`
- `Lifecycle`
- `Domain`
- `CrossFeature`
- `ErrorVocab`
- `Poller`
- `Report`
- `Manifest`
- `Other`

### Rule values

The exact rule code as it appears in the doctor JSON output, e.g.
`TEST-MISSING-AUTHORED-001`.

## Default behavior

When `--fail-on` is omitted, the gate falls back to `--fail-on error` — the
legacy behavior of `lazuli doctor`. This keeps existing CI green by default
while opening the door to category- or rule-specific gating.

## Examples

```bash
# Default — fails on any error.
lazuli doctor app/

# Strict mode for the TestDiscipline category only.
lazuli doctor app/ --fail-on category:TestDiscipline

# CI mode — escalate warnings to failure across the board.
lazuli doctor app/ --fail-on error --fail-on warning

# Pinpoint one rule.
lazuli doctor app/ --fail-on rule:TEST-MISSING-AUTHORED-001
```

## Schema parity

Every exit-code decision is derived from the canonical
`DoctorReport.findings[]` JSON shape (`schema_version: 1`). The CLI, the LSP
`Diagnostic.data`, and the `--watch` NDJSON stream all consume the same
payload. See `crates/lazuli_cli/src/doctor_report.rs` for the source of
truth.
