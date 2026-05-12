package migrations

import (
	"fmt"
	"time"
)

// MigrationContract is the short codegen-facing alias for the tenant
// migration runtime contract.
type MigrationContract = TenantMigrationContract

// Target is the short codegen-facing alias for a tenant migration target.
type Target = TenantMigrationTarget

// On creates a MigrationContract builder for the given resource axis.
//
//	m := migrations.On("org_id").Named("backfill_lifecycle").Timeout("10m").Retry(3, migrations.BackoffExponential).Handler("./migrations/x.go").Build()
func On(axis string) MigrationBuilder {
	return MigrationBuilder{target: Target{Axis: axis}}
}

// MigrationBuilder lowers fluent codegen output into a MigrationContract.
type MigrationBuilder struct {
	feature     string
	name        string
	target      Target
	timeout     string
	idempotency string
	handlerPath string
	retry       *RetryPolicy
	preHook     string
	postHook    string
}

// Named sets the migration identifier.
func (b MigrationBuilder) Named(name string) MigrationBuilder {
	b.name = name
	return b
}

// Feature sets the owning feature name.
func (b MigrationBuilder) Feature(feat string) MigrationBuilder {
	b.feature = feat
	return b
}

// Timeout sets the per-tenant execution timeout. Build parses the value with
// time.ParseDuration and panics if the literal is invalid.
func (b MigrationBuilder) Timeout(s string) MigrationBuilder {
	b.timeout = s
	return b
}

// Retry sets the retry policy. A negative retry count is a programmer error.
func (b MigrationBuilder) Retry(count int, backoff BackoffStrategy) MigrationBuilder {
	if count < 0 {
		panic(fmt.Sprintf("migrations: retry count must be non-negative, got %d", count))
	}
	retry := RetryPolicy{Count: uint32(count), Backoff: backoff}
	b.retry = &retry
	return b
}

// Handler sets the handler path resolved by generated boot wiring.
func (b MigrationBuilder) Handler(path string) MigrationBuilder {
	b.handlerPath = path
	return b
}

// Idempotent sets the idempotency key path evaluated against the migration
// envelope.
func (b MigrationBuilder) Idempotent(path string) MigrationBuilder {
	b.idempotency = path
	return b
}

// PreHook records a deploy hook path for codegen that shares this fluent
// surface. Build omits it because MigrationContract has no per-migration hook
// slot; deploy-level hooks live on DeployPolicy.
func (b MigrationBuilder) PreHook(path string) MigrationBuilder {
	b.preHook = path
	return b
}

// PostHook records a deploy hook path for codegen that shares this fluent
// surface. Build omits it because MigrationContract has no per-migration hook
// slot; deploy-level hooks live on DeployPolicy.
func (b MigrationBuilder) PostHook(path string) MigrationBuilder {
	b.postHook = path
	return b
}

// Build returns the MigrationContract represented by the builder.
func (b MigrationBuilder) Build() MigrationContract {
	var timeout time.Duration
	if b.timeout != "" {
		parsed, err := time.ParseDuration(b.timeout)
		if err != nil {
			panic(fmt.Sprintf("migrations: invalid timeout %q: %v", b.timeout, err))
		}
		timeout = parsed
	}

	return MigrationContract{
		Feature:     b.feature,
		Name:        b.name,
		Target:      b.target,
		Idempotency: IdempotencyKeySpec{Path: b.idempotency},
		Retry:       b.retry,
		Timeout:     timeout,
		HandlerPath: b.handlerPath,
	}
}

func _migrationBuilderInlineChecks() {
	var _ MigrationContract = On("org_id").
		Feature("customer").
		Named("backfill_lifecycle").
		Timeout("10m").
		Retry(3, BackoffExponential).
		Idempotent("tenant.org_id").
		Handler("./migrations/backfill_lifecycle.go").
		Build()

	var _ MigrationContract = On("team_id").Named("seed_team_tables").Build()

	var _ MigrationBuilder = On("org_id").
		PreHook("./hooks/migration_pre.sh").
		PostHook("./hooks/migration_post.sh")
}
