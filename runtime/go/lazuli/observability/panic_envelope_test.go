package observability_test

import (
	"context"
	"errors"
	"fmt"
	"strings"
	"testing"
	"time"

	"lazuli.dev/runtime/lazuli"
	"lazuli.dev/runtime/lazuli/observability"
)

func TestPanicEnvelopeFromReportIncludesTypedErrorRequestAndRedactedStack(t *testing.T) {
	cause := errors.New("field cause")
	recovered := fmt.Errorf("handler panic: %w", &lazuli.FieldError{
		Base: lazuli.ErrorBase{
			Code:    "field_invalid",
			Status:  400,
			Message: "email is invalid",
			Origin:  lazuli.OriginUserDSL,
			Feature: "customer",
			Kind:    "command",
			Op:      "create_customer",
			Source:  "features/customer.lzi:42:8",
			Cause:   cause,
		},
		Field:     "email",
		Path:      "input.email",
		Reason:    lazuli.FieldReasonInvalidFormat,
		InputType: "string",
	})
	recoveredAt := time.Date(2026, 5, 12, 20, 10, 0, 0, time.UTC)
	report := observability.PanicReport{
		Recovered:     recovered,
		Stack:         []byte("goroutine 1 [running]:\nmain.fn()\n\tC:/Users/lucas/project/customer.gen.go:99 +0x1\n"),
		Scope:         observability.ScopeHTTPCommand,
		RequestMethod: "PATCH",
		RequestPath:   "/api/v1/customer",
		Time:          recoveredAt,
	}

	envelope := observability.PanicEnvelopeFromReport(context.Background(), report, observability.PanicEnvelopeOptions{
		StackMode: observability.PanicStackRedacted,
		RequestID: "req-1",
		TraceID:   "trace-1",
	})

	if envelope.Error.Code != "field_invalid" {
		t.Fatalf("Error.Code = %q, want field_invalid", envelope.Error.Code)
	}
	if envelope.Error.Status != 400 {
		t.Fatalf("Error.Status = %d, want 400", envelope.Error.Status)
	}
	if envelope.Error.Message != "email is invalid" {
		t.Fatalf("Error.Message = %q, want email is invalid", envelope.Error.Message)
	}
	if envelope.Error.Origin != "user_dsl" {
		t.Fatalf("Error.Origin = %q, want user_dsl", envelope.Error.Origin)
	}
	if envelope.Error.Feature != "customer" || envelope.Error.Kind != "command" || envelope.Error.Op != "create_customer" {
		t.Fatalf("operation = %s/%s/%s, want customer/command/create_customer", envelope.Error.Feature, envelope.Error.Kind, envelope.Error.Op)
	}
	if envelope.Error.Source != "features/customer.lzi:42:8" {
		t.Fatalf("Error.Source = %q, want typed source", envelope.Error.Source)
	}

	if envelope.Debug.Scope != "http_command" {
		t.Fatalf("Debug.Scope = %q, want http_command", envelope.Debug.Scope)
	}
	if envelope.Debug.Request == nil {
		t.Fatal("Debug.Request = nil, want request metadata")
	}
	if envelope.Debug.Request.Method != "PATCH" || envelope.Debug.Request.Path != "/api/v1/customer" {
		t.Fatalf("request route = %s %s, want PATCH /api/v1/customer", envelope.Debug.Request.Method, envelope.Debug.Request.Path)
	}
	if envelope.Debug.Request.RequestID != "req-1" || envelope.Debug.Request.TraceID != "trace-1" {
		t.Fatalf("request ids = %s/%s, want req-1/trace-1", envelope.Debug.Request.RequestID, envelope.Debug.Request.TraceID)
	}
	if !envelope.Debug.StackRedacted {
		t.Fatal("Debug.StackRedacted = false, want true")
	}
	if strings.Contains(envelope.Debug.Stack, "C:/Users/lucas/project") {
		t.Fatalf("Debug.Stack = %q, want local path redacted", envelope.Debug.Stack)
	}
	if !strings.Contains(envelope.Debug.Stack, "customer.gen.go:99") {
		t.Fatalf("Debug.Stack = %q, want source basename retained", envelope.Debug.Stack)
	}
	if envelope.Debug.RecoveredAt == nil || !envelope.Debug.RecoveredAt.Equal(recoveredAt) {
		t.Fatalf("Debug.RecoveredAt = %v, want %v", envelope.Debug.RecoveredAt, recoveredAt)
	}
}

func TestPanicEnvelopeFromPanicErrorUsesOperationLabelsAndOmitsStack(t *testing.T) {
	ctx, end := observability.StartOp(context.Background(), observability.OpTag{
		Feature: "billing",
		Kind:    "job",
		Name:    "send_invoice",
		Source:  "features/billing.lzi:7:3",
	})
	defer end()

	panicErr := &observability.PanicError{
		Recovered: "worker boom",
		Stack:     []byte("stack should be omitted"),
		Scope:     observability.ScopeJobWorker,
	}

	envelope := observability.PanicEnvelopeFromPanicError(ctx, panicErr, observability.PanicEnvelopeOptions{})

	if envelope.Error.Code != "internal_panic" {
		t.Fatalf("Error.Code = %q, want internal_panic", envelope.Error.Code)
	}
	if envelope.Error.Status != 500 {
		t.Fatalf("Error.Status = %d, want 500", envelope.Error.Status)
	}
	if envelope.Error.Origin != "lib_internal" {
		t.Fatalf("Error.Origin = %q, want lib_internal", envelope.Error.Origin)
	}
	if envelope.Error.Feature != "billing" || envelope.Error.Kind != "job" || envelope.Error.Op != "send_invoice" {
		t.Fatalf("operation = %s/%s/%s, want billing/job/send_invoice", envelope.Error.Feature, envelope.Error.Kind, envelope.Error.Op)
	}
	if envelope.Error.Source != "features/billing.lzi:7:3" {
		t.Fatalf("Error.Source = %q, want context source", envelope.Error.Source)
	}
	if envelope.Debug.Scope != "job_worker" {
		t.Fatalf("Debug.Scope = %q, want job_worker", envelope.Debug.Scope)
	}
	if envelope.Debug.Stack != "" {
		t.Fatalf("Debug.Stack = %q, want omitted stack", envelope.Debug.Stack)
	}

	wrapped := fmt.Errorf("outer: %w", panicErr)
	fromErr, ok := observability.PanicEnvelopeFromError(ctx, wrapped, observability.PanicEnvelopeOptions{})
	if !ok {
		t.Fatal("PanicEnvelopeFromError ok = false, want true")
	}
	if fromErr.Error.Source != envelope.Error.Source {
		t.Fatalf("wrapped Source = %q, want %q", fromErr.Error.Source, envelope.Error.Source)
	}
}

func TestPanicEnvelopeOptionsOmitSourceAndTruncateRawStack(t *testing.T) {
	report := observability.PanicReport{
		Recovered: "boom",
		Stack:     []byte("0123456789abcdef"),
		Scope:     observability.ScopeWebhookHandler,
	}
	ctx, end := observability.StartOp(context.Background(), observability.OpTag{
		Feature: "crm",
		Kind:    "webhook",
		Name:    "stripe",
		Source:  "features/crm.lzi:9:2",
	})
	defer end()

	envelope := observability.PanicEnvelopeFromReport(ctx, report, observability.PanicEnvelopeOptions{
		OmitSource:    true,
		StackMode:     observability.PanicStackRaw,
		MaxStackBytes: 8,
	})

	if envelope.Error.Source != "" {
		t.Fatalf("Error.Source = %q, want omitted source", envelope.Error.Source)
	}
	if envelope.Error.Feature != "crm" || envelope.Error.Kind != "webhook" || envelope.Error.Op != "stripe" {
		t.Fatalf("operation = %s/%s/%s, want crm/webhook/stripe", envelope.Error.Feature, envelope.Error.Kind, envelope.Error.Op)
	}
	if envelope.Debug.Scope != "webhook_handler" {
		t.Fatalf("Debug.Scope = %q, want webhook_handler", envelope.Debug.Scope)
	}
	if envelope.Debug.StackRedacted {
		t.Fatal("Debug.StackRedacted = true, want false")
	}
	if !envelope.Debug.StackTruncated {
		t.Fatal("Debug.StackTruncated = false, want true")
	}
	if !strings.HasPrefix(envelope.Debug.Stack, "01234567") {
		t.Fatalf("Debug.Stack = %q, want raw truncated prefix", envelope.Debug.Stack)
	}
}
