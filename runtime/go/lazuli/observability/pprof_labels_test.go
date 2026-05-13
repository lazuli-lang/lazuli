package observability

import (
	"context"
	"runtime/pprof"
	"testing"

	"lazuli.dev/runtime/lazuli"
)

func TestStartOpAttachesLabels(t *testing.T) {
	ctx := lazuli.WithSource(context.Background(), lazuli.SourceTag{
		Capsule: "crm",
		Feature: "customer",
		Kind:    "command",
		Op:      "create_customer",
	})
	ctx, end := StartOp(ctx)
	defer end()

	for key, want := range map[string]string{
		"capsule": "crm",
		"feature": "customer",
		"kind":    "command",
		"op":      "create_customer",
	} {
		got, ok := pprof.Label(ctx, key)
		if !ok {
			t.Fatalf("missing pprof label %q", key)
		}
		if got != want {
			t.Fatalf("label %q = %q, want %q", key, got, want)
		}
	}
}

func TestStartOpWithoutTagIsNoOp(t *testing.T) {
	ctx := context.Background()
	ctx2, end := StartOp(ctx)
	end()
	if ctx2 != ctx {
		t.Error("StartOp without tag should return ctx unchanged")
	}
}
