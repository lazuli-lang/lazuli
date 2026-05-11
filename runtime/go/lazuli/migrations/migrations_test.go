// Smoke tests for the migration dispatcher shape. The full
// integration test (atlas/golang-migrate against a real DB) is a Drusa
// concern; this file pins the contract shape + the in-process
// dispatcher's tenant fanout behaviour.
package migrations

import (
	"context"
	"errors"
	"testing"
	"time"
)

type stubLogger struct {
	infos []string
	warns []string
	errs  []string
}

func (l *stubLogger) Info(msg string, _ ...any)  { l.infos = append(l.infos, msg) }
func (l *stubLogger) Warn(msg string, _ ...any)  { l.warns = append(l.warns, msg) }
func (l *stubLogger) Error(msg string, _ ...any) { l.errs = append(l.errs, msg) }

type stubDirectory struct {
	axes map[string][]string
}

func (d *stubDirectory) List(_ context.Context, axis string) ([]string, error) {
	if tenants, ok := d.axes[axis]; ok {
		return tenants, nil
	}
	return nil, ErrMigrationTenantAxisUnknown
}

type stubMigrator struct{}

func (stubMigrator) Plan(_ context.Context, _ string) (any, error) { return nil, nil }
func (stubMigrator) Apply(_ context.Context, _ string, _ any) error { return nil }

func TestDispatcherFansOutPerTenant(t *testing.T) {
	directory := &stubDirectory{axes: map[string][]string{"org": {"acme", "globex"}}}
	logger := &stubLogger{}
	d := NewDispatcher(DeployPolicy{}, stubMigrator{}, directory, logger)

	var seen []string
	d.Register(
		TenantMigrationContract{
			Feature:     "customer",
			Name:        "backfill_score",
			Target:      TenantMigrationTarget{Axis: "org"},
			Idempotency: IdempotencyKeySpec{Path: "tenant.org_id"},
			HandlerPath: "./migrations/backfill_score.go",
		},
		func(_ context.Context, tenantID string) error {
			seen = append(seen, tenantID)
			return nil
		},
	)

	if err := d.ApplyAll(context.Background()); err != nil {
		t.Fatalf("ApplyAll returned %v", err)
	}
	if want := []string{"acme", "globex"}; !equal(seen, want) {
		t.Fatalf("fanout = %v, want %v", seen, want)
	}
	if len(logger.errs) != 0 {
		t.Fatalf("unexpected error logs: %v", logger.errs)
	}
}

func TestDispatcherUnknownAxis(t *testing.T) {
	directory := &stubDirectory{axes: map[string][]string{}}
	d := NewDispatcher(DeployPolicy{}, stubMigrator{}, directory, &stubLogger{})

	d.Register(
		TenantMigrationContract{
			Feature:     "customer",
			Name:        "backfill_x",
			Target:      TenantMigrationTarget{Axis: "team"},
			Idempotency: IdempotencyKeySpec{Path: "tenant.team_id"},
		},
		func(context.Context, string) error { return nil },
	)

	err := d.ApplyAll(context.Background())
	if !errors.Is(err, ErrMigrationTenantAxisUnknown) {
		t.Fatalf("expected ErrMigrationTenantAxisUnknown, got %v", err)
	}
}

func TestDispatcherHandlerError(t *testing.T) {
	directory := &stubDirectory{axes: map[string][]string{"org": {"acme"}}}
	d := NewDispatcher(DeployPolicy{}, stubMigrator{}, directory, &stubLogger{})

	sentinel := errors.New("handler boom")
	d.Register(
		TenantMigrationContract{
			Feature:     "customer",
			Name:        "backfill_score",
			Target:      TenantMigrationTarget{Axis: "org"},
			Idempotency: IdempotencyKeySpec{Path: "tenant.org_id"},
			Timeout:     500 * time.Millisecond,
		},
		func(context.Context, string) error { return sentinel },
	)

	err := d.ApplyAll(context.Background())
	if !errors.Is(err, sentinel) {
		t.Fatalf("expected sentinel, got %v", err)
	}
}

func equal(a, b []string) bool {
	if len(a) != len(b) {
		return false
	}
	for i := range a {
		if a[i] != b[i] {
			return false
		}
	}
	return true
}
