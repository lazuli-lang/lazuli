# Lazuli Quickstart

## Prereqs

- Go 1.25+
- Rust 1.83+ (for the Lazuli compiler)
- Postgres 16+ (needed to run the generated server; codegen and build smoke do not need it)

## Install

```bash
cargo install --path crates/lazuli_cli
```

## Smallest possible app

Run these from the Lazuli repository root:

```bash
lazuli new my-app
cd my-app
lazuli generate go . --out dist/go/
cd dist/go
go mod edit -replace=lazuli.dev/runtime=../../../runtime/go
go mod tidy
go build -o ./server ./...
```

Start Postgres and the server:

```bash
docker run --rm --name lazuli-postgres \
  -e POSTGRES_USER=lazuli \
  -e POSTGRES_PASSWORD=lazuli \
  -e POSTGRES_DB=lazuli \
  -p 5432:5432 \
  -d postgres:16-alpine

LAZULI_DB='postgres://lazuli:lazuli@localhost:5432/lazuli?sslmode=disable' \
LAZULI_ADDR=:8080 \
./server
```

Visit http://localhost:8080/healthz.

## Smoke fixtures

- `examples/crm.lzi` - smallest single-file fixture.
- `examples/full-capsule/` - generated Go build smoke fixture.
- `examples/marketplace-mini/` - slim marketplace shape (account / listing / order / payment).

## Layout of generated code

`dist/go/<feature>/<kind>.gen.go` per kind: `resource`, `command`, `query`, `auth`, `api`, `job`, `webhook`, `notification`, `storage`, `migration`, `events`, ...

Root files:

- `dist/go/go.mod`
- `dist/go/main.go`
- `dist/go/lazuli_app.gen.go`
- `dist/go/migrations/*.sql`

## Running with Postgres

```bash
LAZULI_DB='postgres://lazuli:lazuli@localhost:5432/lazuli?sslmode=disable' \
LAZULI_ADDR=:8080 \
./server
```

## Migrations

`lazuli generate go` writes migration SQL under `dist/go/migrations/`.

```bash
lazuli generate go . --out dist/go/
ls dist/go/migrations/
for file in dist/go/migrations/*.sql; do
  psql "$LAZULI_DB" -f "$file"
done
```

## Common verbs

```bash
lazuli new <project>
lazuli init <path>
lazuli parse <input>
lazuli check <input>
lazuli doctor <input>
lazuli compile <input> --out <dir>
lazuli generate go <input> --out <dist> [--module <module>] [--check]
lazuli generate openapi <input> [--out <file>] [--api-version <version>]
lazuli inspect <input> --expand=auth,jobs,webhooks
lazuli plan <input> --check <checkpoint>
lazuli changelog --from a.json --to b.json [--output report.md]
lazuli translate extract <input> --out i18n [--locale en-US] [--check]
```
