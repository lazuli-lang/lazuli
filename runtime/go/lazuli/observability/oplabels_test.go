package observability

import (
	"context"
	"runtime/pprof"
	"testing"
)

func TestStartOpAppliesPprofLabels(t *testing.T) {
	ctx, end := StartOp(context.Background(), OpTag{
		Feature:        "customer",
		Kind:           "command",
		Name:           "create_customer",
		Source:         "features/customer.lzi:42:1",
		PatternID:      "command_pgx_insert",
		PatternVersion: "v1",
	})
	defer end()

	assertOpLabel(t, ctx, opLabelFeatureKey, "customer")
	assertOpLabel(t, ctx, opLabelKindKey, "command")
	assertOpLabel(t, ctx, opLabelNameKey, "create_customer")
	assertOpLabel(t, ctx, opLabelSourceKey, "features/customer.lzi:42:1")
	assertOpLabel(t, ctx, opLabelPatternIDKey, "command_pgx_insert")
	assertOpLabel(t, ctx, opLabelPatternVersionKey, "v1")
}

func TestStartOpPreservesExistingLabels(t *testing.T) {
	base := pprof.WithLabels(context.Background(), pprof.Labels(
		"tenant", "acme",
		opLabelFeatureKey, "old_feature",
	))

	ctx, end := StartOp(base, OpTag{Feature: "customer"})
	defer end()

	assertOpLabel(t, ctx, "tenant", "acme")
	assertOpLabel(t, ctx, opLabelFeatureKey, "customer")
	assertOpLabel(t, ctx, opLabelKindKey, "")
	assertOpLabel(t, ctx, opLabelNameKey, "")
	assertOpLabel(t, ctx, opLabelSourceKey, "")
	assertOpLabel(t, ctx, opLabelPatternIDKey, "")
	assertOpLabel(t, ctx, opLabelPatternVersionKey, "")
}

func TestStartOpEndIsIdempotent(t *testing.T) {
	_, end := StartOp(context.Background(), OpTag{Feature: "customer"})

	end()
	end()
}

func TestStartOpAcceptsNilContext(t *testing.T) {
	ctx, end := StartOp(nil, OpTag{Name: "list_customers"})
	defer end()

	assertOpLabel(t, ctx, opLabelNameKey, "list_customers")
}

func assertOpLabel(t *testing.T, ctx context.Context, key, want string) {
	t.Helper()

	got, ok := pprof.Label(ctx, key)
	if !ok {
		t.Fatalf("label %q missing", key)
	}
	if got != want {
		t.Fatalf("label %q = %q, want %q", key, got, want)
	}
}
