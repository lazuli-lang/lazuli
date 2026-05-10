# Lazuli Testing Strategy

Lazuli should generate tests for declarative behavior and contracts for custom behavior.

## Generated Tests

Declarative constructs can produce generated tests:

- resource validation
- constraints
- query tenant scope
- soft-delete filtering
- command policy calls
- rule denial
- workflow allowed/denied transitions
- event emission shape
- surface source wiring

## Extension Tests

Extensions are authored code. Lazuli should generate contract tests or harnesses, not pretend to know behavior.

Example:

```txt
extension customer.risk_score
expected Function[Customer, Integer]
test harness generated under .lazuli/generated/tests/customer/risk_score_test.go
```

## Capsule Fixtures

Each fixture should answer:

- What does this feature own?
- What is intentionally delegated to extension code?
- What should `lazuli inspect` output?
- What should `lazuli plan` flag as risky?
- Which generated tests should exist?

## Strict CI

Recommended CI steps:

```bash
lazuli check --security-profile strict
lazuli doctor .
lazuli plan --check
lazuli generate
go test ./...
npm test
```

The check and doctor steps should fail before target-language tests when the
semantic contract is broken. `check` is file-local; `doctor` loads a capsule
package and verifies cross-file contracts such as `.lzx` audience reachability
against `.lzi` command policies.
