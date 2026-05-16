package lazuli

import (
	"context"
	"errors"
	"testing"
)

// Each subtest resets the singleton so the suite is order-independent.
func resetTenantResolver() {
	defaultTenantResolver = nil
}

func TestResolveDefaultTenant_NoResolverRegistered(t *testing.T) {
	resetTenantResolver()
	got, err := resolveDefaultTenant(context.Background())
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if got != nil {
		t.Fatalf("expected nil tenant when no resolver registered, got %+v", got)
	}
}

func TestWithDefaultTenant_ReturnsRegisteredTenant(t *testing.T) {
	resetTenantResolver()
	want := &Tenant{OrgID: 42}
	WithDefaultTenant(func(ctx context.Context) (*Tenant, error) {
		return want, nil
	})
	got, err := resolveDefaultTenant(context.Background())
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if got == nil || got.OrgID != want.OrgID {
		t.Fatalf("expected tenant %+v, got %+v", want, got)
	}
}

func TestWithDefaultTenant_PropagatesError(t *testing.T) {
	resetTenantResolver()
	sentinel := errors.New("resolver boom")
	WithDefaultTenant(func(ctx context.Context) (*Tenant, error) {
		return nil, sentinel
	})
	got, err := resolveDefaultTenant(context.Background())
	if got != nil {
		t.Fatalf("expected nil tenant on error, got %+v", got)
	}
	if !errors.Is(err, sentinel) {
		t.Fatalf("expected sentinel error, got %v", err)
	}
}

func TestWithDefaultTenant_NilReturnPreservesNilTenant(t *testing.T) {
	// Resolver may return (nil, nil) to signal "no default; let the
	// caller decide" — must propagate without erroring.
	resetTenantResolver()
	WithDefaultTenant(func(ctx context.Context) (*Tenant, error) {
		return nil, nil
	})
	got, err := resolveDefaultTenant(context.Background())
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if got != nil {
		t.Fatalf("expected nil tenant, got %+v", got)
	}
}

func TestWithDefaultTenant_LastRegistrationWins(t *testing.T) {
	resetTenantResolver()
	WithDefaultTenant(func(ctx context.Context) (*Tenant, error) {
		return &Tenant{OrgID: 1}, nil
	})
	WithDefaultTenant(func(ctx context.Context) (*Tenant, error) {
		return &Tenant{OrgID: 2}, nil
	})
	got, _ := resolveDefaultTenant(context.Background())
	if got == nil || got.OrgID != 2 {
		t.Fatalf("expected tenant org id 2 (last registration), got %+v", got)
	}
}
