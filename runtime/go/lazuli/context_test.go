package lazuli

import (
	"context"
	"testing"
)

func TestWithSource(t *testing.T) {
	t.Run("with_source_attaches_tag_to_context", func(t *testing.T) {
		want := SourceTag{
			Capsule: "crm",
			Feature: "customer",
			Kind:    "command",
			Op:      "create_customer",
			Source:  "features/customer.lzi:42:1",
		}

		got := SourceTagFromContext(WithSource(context.Background(), want))
		if got != want {
			t.Fatalf("SourceTagFromContext() = %#v, want %#v", got, want)
		}
	})

	t.Run("source_tag_from_context_returns_zero_value_when_absent", func(t *testing.T) {
		if got := SourceTagFromContext(context.Background()); got != (SourceTag{}) {
			t.Fatalf("SourceTagFromContext() = %#v, want zero value", got)
		}
	})

	t.Run("with_source_overwrites_previous_tag", func(t *testing.T) {
		first := SourceTag{Capsule: "crm", Feature: "customer", Kind: "command", Op: "create"}
		second := SourceTag{Capsule: "crm", Feature: "invoice", Kind: "command", Op: "settle"}

		ctx := WithSource(context.Background(), first)
		ctx = WithSource(ctx, second)

		if got := SourceTagFromContext(ctx); got != second {
			t.Fatalf("SourceTagFromContext() = %#v, want %#v", got, second)
		}
	})
}
