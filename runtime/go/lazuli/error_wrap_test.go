package lazuli_test

import (
	"context"
	"errors"
	"fmt"
	"net/http"
	"testing"

	"lazuli.dev/runtime/lazuli"
	"lazuli.dev/runtime/lazuli/storage"
)

func TestC136WrapAddsSourceAwareEnvelope(t *testing.T) {
	ctx := lazuli.WithSource(context.Background(), lazuli.SourceTag{
		Feature: "customer",
		Kind:    "command",
		Name:    "create_customer",
		File:    "features/customer.lzi",
		Line:    42,
		Column:  8,
	})

	err := lazuli.Wrap(ctx, storage.ErrFileSizeExceeded)
	if !errors.Is(err, storage.ErrFileSizeExceeded) {
		t.Fatal("Wrap did not preserve errors.Is for the original sentinel")
	}

	var got *lazuli.Error
	if !errors.As(err, &got) {
		t.Fatalf("Wrap returned %T, want *lazuli.Error in errors.As chain", err)
	}
	if got.Code != "storage.file_size_exceeded" {
		t.Fatalf("Code = %q, want storage.file_size_exceeded", got.Code)
	}
	if got.Status != http.StatusRequestEntityTooLarge {
		t.Fatalf("Status = %d, want %d", got.Status, http.StatusRequestEntityTooLarge)
	}
	if got.Base.Origin != lazuli.OriginUserDSL {
		t.Fatalf("Base.Origin = %v, want OriginUserDSL", got.Base.Origin)
	}
	if got.Base.Feature != "customer" || got.Base.Kind != "command" || got.Base.Op != "create_customer" {
		t.Fatalf("Base source scope = %q/%q/%q", got.Base.Feature, got.Base.Kind, got.Base.Op)
	}
	if got.Base.Source != "features/customer.lzi:42:8" {
		t.Fatalf("Base.Source = %q, want features/customer.lzi:42:8", got.Base.Source)
	}
	if got.Base.Cause != storage.ErrFileSizeExceeded {
		t.Fatalf("Base.Cause = %v, want original sentinel", got.Base.Cause)
	}
}

func TestC136WrapfUsesExplicitBaseAndKeepsCause(t *testing.T) {
	cause := errors.New("adapter unavailable")
	ctx := lazuli.WithSource(context.Background(), lazuli.SourceTag{
		Feature: "customer",
		Kind:    "command",
		Name:    "send_receipt",
		File:    "features/customer.lzi",
		Line:    50,
		Column:  3,
	})

	err := lazuli.Wrapf(ctx, cause, lazuli.ErrorBase{
		Code:    "email.delivery_failed",
		Origin:  lazuli.OriginAdapterRuntime,
		Status:  http.StatusBadGateway,
		Feature: "billing",
		Source:  "features/billing.lzi:9:1",
	}, "send %s", "receipt")

	if !errors.Is(err, cause) {
		t.Fatal("Wrapf did not preserve errors.Is for cause")
	}
	var got *lazuli.Error
	if !errors.As(err, &got) {
		t.Fatalf("Wrapf returned %T, want *lazuli.Error in errors.As chain", err)
	}
	if got.Message != "send receipt" {
		t.Fatalf("Message = %q, want send receipt", got.Message)
	}
	if got.Base.Feature != "billing" {
		t.Fatalf("Base.Feature = %q, want explicit billing", got.Base.Feature)
	}
	if got.Base.Kind != "command" || got.Base.Op != "send_receipt" {
		t.Fatalf("Base source scope = %q/%q, want command/send_receipt", got.Base.Kind, got.Base.Op)
	}
	if got.Base.Source != "features/billing.lzi:9:1" {
		t.Fatalf("Base.Source = %q, want explicit features/billing.lzi:9:1", got.Base.Source)
	}
	if got.Base.Origin != lazuli.OriginAdapterRuntime {
		t.Fatalf("Base.Origin = %v, want OriginAdapterRuntime", got.Base.Origin)
	}
}

func TestC136WrapKeepsSingleEnvelope(t *testing.T) {
	sentinel := errors.New("password mismatch")
	fieldErr := &lazuli.FieldError{
		Base: lazuli.ErrorBase{
			Code:   "field_invalid",
			Origin: lazuli.OriginUserDSL,
			Cause:  sentinel,
		},
		Field:  "password",
		Reason: lazuli.FieldReasonMismatch,
	}

	if got := lazuli.Wrap(context.Background(), fieldErr); got != fieldErr {
		t.Fatalf("Wrap returned %T, want existing FieldError unchanged", got)
	}

	outer := fmt.Errorf("handler failed: %w", fieldErr)
	if got := lazuli.Wrapf(context.Background(), outer, "ignored"); got != outer {
		t.Fatalf("Wrapf returned %T, want existing wrapped FieldError unchanged", got)
	}
	if !errors.Is(outer, sentinel) {
		t.Fatal("test setup lost sentinel through FieldError")
	}
	var gotField *lazuli.FieldError
	if !errors.As(outer, &gotField) {
		t.Fatal("test setup lost FieldError through errors.As")
	}
}

func TestC136ErrorfPreservesWrappedCause(t *testing.T) {
	sentinel := errors.New("boom")
	ctx := lazuli.WithSource(context.Background(), lazuli.SourceTag{
		Feature: "customer",
		Kind:    "query",
		Name:    "lookup_customer",
		File:    "features/customer.lzi",
		Line:    64,
		Column:  2,
	})

	err := lazuli.Errorf(ctx, lazuli.ErrorBase{
		Code:   "customer.lookup_failed",
		Origin: lazuli.OriginLibInternal,
		Status: http.StatusInternalServerError,
	}, "lookup failed: %w", sentinel)

	if !errors.Is(err, sentinel) {
		t.Fatal("Errorf did not preserve %w cause")
	}
	var got *lazuli.Error
	if !errors.As(err, &got) {
		t.Fatalf("Errorf returned %T, want *lazuli.Error in errors.As chain", err)
	}
	if got.Base.Source != "features/customer.lzi:64:2" {
		t.Fatalf("Base.Source = %q, want features/customer.lzi:64:2", got.Base.Source)
	}
	if got.Base.Cause != sentinel {
		t.Fatalf("Base.Cause = %v, want sentinel", got.Base.Cause)
	}
	if got.Message != "lookup failed: boom" {
		t.Fatalf("Message = %q, want lookup failed: boom", got.Message)
	}
}

func TestC136ErrorfDoesNotDoubleWrapExistingLazuliError(t *testing.T) {
	fieldErr := &lazuli.FieldError{
		Base: lazuli.ErrorBase{
			Code:   "field_invalid",
			Origin: lazuli.OriginUserDSL,
		},
		Field: "email",
	}

	err := lazuli.Errorf(context.Background(), "handler failed: %w", fieldErr)
	var gotField *lazuli.FieldError
	if !errors.As(err, &gotField) {
		t.Fatal("Errorf did not preserve existing FieldError")
	}
	var gotEnvelope *lazuli.Error
	if errors.As(err, &gotEnvelope) {
		t.Fatal("Errorf added a second Lazuli envelope around FieldError")
	}
	if got := err.Error(); got != "handler failed: lazuli/field_invalid" {
		t.Fatalf("Error() = %q, want single formatted message", got)
	}
}

func TestC136WrapNilReturnsNil(t *testing.T) {
	if err := lazuli.Wrap(context.Background(), nil); err != nil {
		t.Fatalf("Wrap(nil) = %v, want nil", err)
	}
	if err := lazuli.Wrapf(context.Background(), nil, "ignored"); err != nil {
		t.Fatalf("Wrapf(nil) = %v, want nil", err)
	}
}
