package observability

import (
	"context"
	"errors"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync"
	"testing"
)

type panicReportCapture struct {
	mu       sync.Mutex
	reports  []PanicReport
	contexts []context.Context
}

func (c *panicReportCapture) ReportPanic(ctx context.Context, report PanicReport) {
	c.mu.Lock()
	defer c.mu.Unlock()
	c.contexts = append(c.contexts, ctx)
	c.reports = append(c.reports, report)
}

func (c *panicReportCapture) snapshot() ([]context.Context, []PanicReport) {
	c.mu.Lock()
	defer c.mu.Unlock()

	contexts := append([]context.Context(nil), c.contexts...)
	reports := append([]PanicReport(nil), c.reports...)
	return contexts, reports
}

func installPanicReporter(t *testing.T, reporter PanicReporter) {
	t.Helper()

	SetPanicReporter(reporter)
	t.Cleanup(ResetPanicReporter)
}

func TestRecoverHTTPReportsPanic(t *testing.T) {
	capture := &panicReportCapture{}
	installPanicReporter(t, capture)

	type contextKey string
	const requestKey contextKey = "request"

	handler := RecoverHTTP(http.HandlerFunc(func(http.ResponseWriter, *http.Request) {
		panic("boom")
	}))
	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodPatch, "/api/v1/c/create?ignored=1", nil)
	req = req.WithContext(context.WithValue(req.Context(), requestKey, "request-context"))

	handler.ServeHTTP(rec, req)

	if rec.Code != http.StatusInternalServerError {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusInternalServerError)
	}
	if got := rec.Header().Get("Content-Type"); got != "application/json" {
		t.Fatalf("Content-Type = %q, want application/json", got)
	}
	if got := rec.Body.String(); got != `{"error":{"code":"internal_panic"}}` {
		t.Fatalf("body = %q, want internal_panic envelope", got)
	}

	contexts, reports := capture.snapshot()
	if len(reports) != 1 {
		t.Fatalf("reports = %d, want 1", len(reports))
	}
	if got := contexts[0].Value(requestKey); got != "request-context" {
		t.Fatalf("report context value = %v, want request-context", got)
	}

	report := reports[0]
	if report.Recovered != "boom" {
		t.Fatalf("Recovered = %v, want boom", report.Recovered)
	}
	if report.Scope != ScopeHTTPCommand {
		t.Fatalf("Scope = %v, want %v", report.Scope, ScopeHTTPCommand)
	}
	if report.RequestMethod != http.MethodPatch {
		t.Fatalf("RequestMethod = %q, want %s", report.RequestMethod, http.MethodPatch)
	}
	if report.RequestPath != "/api/v1/c/create" {
		t.Fatalf("RequestPath = %q, want /api/v1/c/create", report.RequestPath)
	}
	if len(report.Stack) == 0 || !strings.Contains(string(report.Stack), "TestRecoverHTTPReportsPanic") {
		t.Fatalf("Stack = %q, want test stack trace", string(report.Stack))
	}
	if report.Time.IsZero() {
		t.Fatal("Time is zero, want recovery timestamp")
	}
}

func TestRecoverScopeReportsPanic(t *testing.T) {
	capture := &panicReportCapture{}
	installPanicReporter(t, capture)

	type contextKey string
	const requestKey contextKey = "scope"
	ctx := context.WithValue(context.Background(), requestKey, "scope-context")

	err := RecoverScope(ctx, ScopeWebhookHandler, func(context.Context) error {
		panic("webhook boom")
	})

	panicErr, ok := err.(*PanicError)
	if !ok {
		t.Fatalf("err = %T, want *PanicError", err)
	}
	if panicErr.Recovered != "webhook boom" {
		t.Fatalf("PanicError.Recovered = %v, want webhook boom", panicErr.Recovered)
	}
	if panicErr.Scope != ScopeWebhookHandler {
		t.Fatalf("PanicError.Scope = %v, want %v", panicErr.Scope, ScopeWebhookHandler)
	}
	if len(panicErr.Stack) == 0 || !strings.Contains(string(panicErr.Stack), "TestRecoverScopeReportsPanic") {
		t.Fatalf("PanicError.Stack = %q, want test stack trace", string(panicErr.Stack))
	}

	contexts, reports := capture.snapshot()
	if len(reports) != 1 {
		t.Fatalf("reports = %d, want 1", len(reports))
	}
	if got := contexts[0].Value(requestKey); got != "scope-context" {
		t.Fatalf("report context value = %v, want scope-context", got)
	}

	report := reports[0]
	if report.Recovered != "webhook boom" {
		t.Fatalf("Recovered = %v, want webhook boom", report.Recovered)
	}
	if report.Scope != ScopeWebhookHandler {
		t.Fatalf("Scope = %v, want %v", report.Scope, ScopeWebhookHandler)
	}
	if report.RequestMethod != "" {
		t.Fatalf("RequestMethod = %q, want empty", report.RequestMethod)
	}
	if report.RequestPath != "" {
		t.Fatalf("RequestPath = %q, want empty", report.RequestPath)
	}
	if report.Time.IsZero() {
		t.Fatal("Time is zero, want recovery timestamp")
	}
}

func TestRecoverScopeReturnsFunctionErrorWithoutReporting(t *testing.T) {
	capture := &panicReportCapture{}
	installPanicReporter(t, capture)

	expected := errors.New("expected")
	err := RecoverScope(context.Background(), ScopeJobWorker, func(context.Context) error {
		return expected
	})
	if !errors.Is(err, expected) {
		t.Fatalf("err = %v, want %v", err, expected)
	}

	_, reports := capture.snapshot()
	if len(reports) != 0 {
		t.Fatalf("reports = %d, want 0", len(reports))
	}
}

func TestResetPanicReporterClearsReporter(t *testing.T) {
	capture := &panicReportCapture{}
	SetPanicReporter(capture)
	ResetPanicReporter()
	t.Cleanup(ResetPanicReporter)

	err := RecoverScope(context.Background(), ScopeJobWorker, func(context.Context) error {
		panic("worker boom")
	})
	if err == nil {
		t.Fatal("err = nil, want *PanicError")
	}

	_, reports := capture.snapshot()
	if len(reports) != 0 {
		t.Fatalf("reports = %d, want 0", len(reports))
	}
}

func TestPanicReporterPanicDoesNotReplaceRecoveryResult(t *testing.T) {
	installPanicReporter(t, PanicReporterFunc(func(context.Context, PanicReport) {
		panic("reporter failed")
	}))

	err := RecoverScope(context.Background(), ScopeJobWorker, func(context.Context) error {
		panic("worker boom")
	})

	panicErr, ok := err.(*PanicError)
	if !ok {
		t.Fatalf("err = %T, want *PanicError", err)
	}
	if panicErr.Recovered != "worker boom" {
		t.Fatalf("Recovered = %v, want worker boom", panicErr.Recovered)
	}
}
