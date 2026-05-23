package lazuli

import (
	"context"
	"errors"
	"fmt"
	"testing"
)

func TestErrorHierarchy(t *testing.T) {
	t.Run("error_base_implements_error_interface", func(t *testing.T) {
		var err error = &Error{Base: ErrorBase{Code: "field_invalid", Message: "bad email"}}
		if got, want := err.Error(), "field_invalid: bad email"; got != want {
			t.Fatalf("Error() = %q, want %q", got, want)
		}
	})

	t.Run("error_base_unwrap_returns_cause", func(t *testing.T) {
		cause := errors.New("cause")
		err := &Error{Base: ErrorBase{Code: "internal", Cause: cause}}
		if got := err.Unwrap(); got != cause {
			t.Fatalf("Unwrap() = %v, want %v", got, cause)
		}
	})

	t.Run("field_error_unwrap_returns_cause", func(t *testing.T) {
		cause := errors.New("cause")
		err := &FieldError{Base: ErrorBase{Cause: cause}}
		if got := err.Unwrap(); got != cause {
			t.Fatalf("Unwrap() = %v, want %v", got, cause)
		}
	})

	t.Run("field_error_error_base_accessor_returns_base", func(t *testing.T) {
		base := ErrorBase{Code: "field_invalid", Surface: SurfaceUserDSL, Message: "bad email"}
		err := &FieldError{Base: base}
		if got := err.ErrorBase(); got != base {
			t.Fatalf("ErrorBase() = %#v, want %#v", got, base)
		}
	})

	t.Run("errors_as_recovers_field_error_from_wrapped_chain", func(t *testing.T) {
		err := fmt.Errorf("wrapped: %w", &FieldError{Field: "email"})
		var got *FieldError
		if !errors.As(err, &got) {
			t.Fatal("errors.As did not recover FieldError")
		}
		if got.Field != "email" {
			t.Fatalf("Field = %q, want email", got.Field)
		}
	})

	t.Run("errors_is_recovers_sentinel_via_chain", func(t *testing.T) {
		sentinel := errors.New("sentinel")
		err := fmt.Errorf("wrapped: %w", &FieldError{Base: ErrorBase{Cause: sentinel}})
		if !errors.Is(err, sentinel) {
			t.Fatal("errors.Is did not recover sentinel")
		}
	})

	t.Run("field_reason_string_round_trip", func(t *testing.T) {
		tests := map[FieldReason]string{
			FieldReasonRequired:      "required",
			FieldReasonInvalidFormat: "invalid_format",
			FieldReasonOutOfRange:    "out_of_range",
			FieldReasonMismatch:      "mismatch",
			FieldReasonUnknownEnum:   "unknown_enum",
		}
		for reason, want := range tests {
			if got := reason.String(); got != want {
				t.Fatalf("%v.String() = %q, want %q", uint8(reason), got, want)
			}
		}
	})

	t.Run("surface_string_round_trip", func(t *testing.T) {
		tests := map[Surface]string{
			SurfaceUserDSL:        "user_dsl",
			SurfaceLibInternal:    "lib_internal",
			SurfaceCodegenBug:     "codegen_bug",
			SurfaceAdapterRuntime: "adapter_runtime",
		}
		for surface, want := range tests {
			if got := surface.String(); got != want {
				t.Fatalf("%v.String() = %q, want %q", uint8(surface), got, want)
			}
		}
	})

	t.Run("policy_error_basic", func(t *testing.T) {
		err := &PolicyError{Rule: "customer.create.role_admin_only", Subject: "user-1", Resource: "customer-1"}
		if got, want := err.Error(), "policy_denied: rule=customer.create.role_admin_only subject=user-1 resource=customer-1"; got != want {
			t.Fatalf("Error() = %q, want %q", got, want)
		}
	})

	t.Run("tenant_error_basic", func(t *testing.T) {
		err := &TenantError{Axis: "org_id", Expected: "org-1", Actual: "org-2"}
		if got, want := err.Error(), "tenant_mismatch: axis=org_id expected=org-1 actual=org-2"; got != want {
			t.Fatalf("Error() = %q, want %q", got, want)
		}
	})

	t.Run("adapter_error_basic", func(t *testing.T) {
		err := &AdapterError{Adapter: "@lazuli/plugin-example/payment-gateway", Op: "create_preference", RetryBudgetConsumed: 2, RetryBudgetMax: 3}
		if got, want := err.Error(), "adapter_error: @lazuli/plugin-example/payment-gateway.create_preference (retries=2/3)"; got != want {
			t.Fatalf("Error() = %q, want %q", got, want)
		}
	})

	t.Run("lib_bug_error_basic", func(t *testing.T) {
		err := &LibBugError{Component: "lazuli.dev/runtime/lazuli/auth", Invariant: "token parser returned nil"}
		if got, want := err.Error(), "lib_bug: component=lazuli.dev/runtime/lazuli/auth invariant=token parser returned nil"; got != want {
			t.Fatalf("Error() = %q, want %q", got, want)
		}
	})

	t.Run("error_base_from_context_populates_from_source_tag", func(t *testing.T) {
		ctx := WithSource(context.Background(), SourceTag{
			Capsule: "crm",
			Feature: "customer",
			Kind:    "command",
			Op:      "create_customer",
			Source:  "features/customer.lzi:42:1",
		})

		got := ErrorBaseFromContext(ctx, ErrorBase{Code: "field_invalid"})
		if got.Capsule != "crm" || got.Feature != "customer" || got.Kind != "command" || got.Op != "create_customer" || got.Source != "features/customer.lzi:42:1" {
			t.Fatalf("ErrorBaseFromContext() = %#v, want source fields populated", got)
		}
	})

	t.Run("error_base_from_context_preserves_existing_base_fields", func(t *testing.T) {
		ctx := WithSource(context.Background(), SourceTag{
			Capsule: "crm",
			Feature: "customer",
			Kind:    "command",
			Op:      "create_customer",
			Source:  "features/customer.lzi:42:1",
		})
		base := ErrorBase{
			Capsule: "billing",
			Feature: "invoice",
			Kind:    "job",
			Op:      "settle",
			Source:  "features/invoice.lzi:7:1",
		}

		if got := ErrorBaseFromContext(ctx, base); got != base {
			t.Fatalf("ErrorBaseFromContext() = %#v, want %#v", got, base)
		}
	})

	t.Run("error_base_from_context_with_empty_context_leaves_fields_empty", func(t *testing.T) {
		got := ErrorBaseFromContext(context.Background(), ErrorBase{Code: "internal"})
		if got.Capsule != "" || got.Feature != "" || got.Kind != "" || got.Op != "" || got.Source != "" {
			t.Fatalf("ErrorBaseFromContext() = %#v, want source fields empty", got)
		}
	})
}
