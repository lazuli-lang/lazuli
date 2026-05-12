package observability

import (
	"context"
	"errors"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/getsentry/sentry-go"
)

type sentryCaptureTransport struct {
	mu         sync.Mutex
	events     []*sentry.Event
	configured bool
	closed     bool
}

func (t *sentryCaptureTransport) Configure(sentry.ClientOptions) {
	t.mu.Lock()
	t.configured = true
	t.mu.Unlock()
}

func (t *sentryCaptureTransport) SendEvent(event *sentry.Event) {
	t.mu.Lock()
	t.events = append(t.events, event)
	t.mu.Unlock()
}

func (t *sentryCaptureTransport) Flush(time.Duration) bool {
	return true
}

func (t *sentryCaptureTransport) FlushWithContext(context.Context) bool {
	return true
}

func (t *sentryCaptureTransport) Close() {
	t.mu.Lock()
	t.closed = true
	t.mu.Unlock()
}

func (t *sentryCaptureTransport) Events() []*sentry.Event {
	t.mu.Lock()
	defer t.mu.Unlock()
	return append([]*sentry.Event(nil), t.events...)
}

func (t *sentryCaptureTransport) Configured() bool {
	t.mu.Lock()
	defer t.mu.Unlock()
	return t.configured
}

func (t *sentryCaptureTransport) Closed() bool {
	t.mu.Lock()
	defer t.mu.Unlock()
	return t.closed
}

func TestConfigureSentryWithoutDSNDisablesPanicReporter(t *testing.T) {
	capture := &panicReportCapture{}
	SetPanicReporter(capture)

	reporter, err := ConfigureSentry(SentryConfig{})
	if err != nil {
		t.Fatalf("ConfigureSentry returned error: %v", err)
	}
	t.Cleanup(ResetPanicReporter)

	if reporter.Enabled() {
		t.Fatal("reporter.Enabled() = true, want false without DSN")
	}
	if reporter.CaptureError(context.Background(), errors.New("boom")) != nil {
		t.Fatal("CaptureError returned event id for disabled reporter")
	}
	if !reporter.Flush(context.Background()) {
		t.Fatal("Flush = false, want true for disabled reporter")
	}

	err = RecoverScope(context.Background(), ScopeJobWorker, func(context.Context) error {
		panic("worker boom")
	})
	if err == nil {
		t.Fatal("RecoverScope err = nil, want PanicError")
	}

	_, reports := capture.snapshot()
	if len(reports) != 0 {
		t.Fatalf("reports = %d, want 0 after empty DSN disables reporter", len(reports))
	}
}

func TestConfigureSentryInstallsPanicReporter(t *testing.T) {
	transport := &sentryCaptureTransport{}
	reporter, err := ConfigureSentry(SentryConfig{
		DSN:       "http://public@example.com/1",
		Transport: transport,
	})
	if err != nil {
		t.Fatalf("ConfigureSentry returned error: %v", err)
	}
	t.Cleanup(func() {
		ResetPanicReporter()
		reporter.Close()
	})

	if !transport.Configured() {
		t.Fatal("transport was not configured")
	}

	err = RecoverScope(context.Background(), ScopeJobWorker, func(context.Context) error {
		panic("worker boom")
	})
	if err == nil {
		t.Fatal("RecoverScope err = nil, want PanicError")
	}

	events := transport.Events()
	if len(events) != 1 {
		t.Fatalf("events = %d, want 1", len(events))
	}
	event := events[0]
	if event.Level != sentry.LevelFatal {
		t.Fatalf("Level = %q, want fatal", event.Level)
	}
	if event.Message != "worker boom" {
		t.Fatalf("Message = %q, want worker boom", event.Message)
	}
	if event.Tags["lazuli.scope"] != "job_worker" {
		t.Fatalf("lazuli.scope = %q, want job_worker", event.Tags["lazuli.scope"])
	}
}

func TestSentryReporterCapturePanicAddsLazuliContext(t *testing.T) {
	transport := &sentryCaptureTransport{}
	reporter, err := NewSentryReporter(SentryConfig{
		DSN:         "http://public@example.com/1",
		Environment: "test",
		Release:     "runtime-test",
		ServerName:  "test-host",
		Tags: map[string]string{
			"component": "runtime",
		},
		Transport: transport,
		PanicEnvelopeOptions: PanicEnvelopeOptions{
			StackMode: PanicStackRedacted,
		},
	})
	if err != nil {
		t.Fatalf("NewSentryReporter returned error: %v", err)
	}
	t.Cleanup(reporter.Close)

	ctx, end := StartOp(context.Background(), OpTag{
		Feature:        "customer",
		Kind:           "command",
		Name:           "create",
		Source:         "features/customer.lzi:12:3",
		PatternID:      "command-pgx-insert",
		PatternVersion: "v1",
	})
	defer end()

	recoveredAt := time.Date(2026, 5, 12, 20, 10, 0, 0, time.UTC)
	eventID := reporter.CapturePanic(ctx, PanicReport{
		Recovered:     errors.New("database exploded"),
		Stack:         []byte("goroutine 1 [running]:\nmain.create()\n\tC:/private/project/customer.gen.go:77 +0x1\n"),
		Scope:         ScopeHTTPCommand,
		RequestMethod: "POST",
		RequestPath:   "/api/customer",
		Time:          recoveredAt,
	})
	if eventID == nil {
		t.Fatal("CapturePanic returned nil event id")
	}

	events := transport.Events()
	if len(events) != 1 {
		t.Fatalf("events = %d, want 1", len(events))
	}
	event := events[0]
	if event.Environment != "test" || event.Release != "runtime-test" || event.ServerName != "test-host" {
		t.Fatalf("event env/release/server = %q/%q/%q, want test/runtime-test/test-host", event.Environment, event.Release, event.ServerName)
	}
	if event.Tags["component"] != "runtime" {
		t.Fatalf("component tag = %q, want runtime", event.Tags["component"])
	}
	if event.Tags["lazuli.scope"] != "http_command" {
		t.Fatalf("lazuli.scope = %q, want http_command", event.Tags["lazuli.scope"])
	}
	if event.Tags["lazuli.error_code"] != "internal_panic" {
		t.Fatalf("lazuli.error_code = %q, want internal_panic", event.Tags["lazuli.error_code"])
	}
	if event.Tags["lazuli.feature"] != "customer" || event.Tags["lazuli.kind"] != "command" || event.Tags["lazuli.op"] != "create" {
		t.Fatalf("operation tags = %s/%s/%s, want customer/command/create", event.Tags["lazuli.feature"], event.Tags["lazuli.kind"], event.Tags["lazuli.op"])
	}
	if event.Tags["lazuli.pattern_id"] != "command-pgx-insert" || event.Tags["lazuli.pattern_version"] != "v1" {
		t.Fatalf("pattern tags = %s/%s, want command-pgx-insert/v1", event.Tags["lazuli.pattern_id"], event.Tags["lazuli.pattern_version"])
	}
	if event.Transaction != "customer.command.create" {
		t.Fatalf("Transaction = %q, want customer.command.create", event.Transaction)
	}
	if event.Request == nil || event.Request.Method != "POST" || event.Request.URL != "/api/customer" {
		t.Fatalf("Request = %#v, want POST /api/customer", event.Request)
	}
	if len(event.Exception) == 0 || event.Exception[0].Value != "database exploded" {
		t.Fatalf("Exception = %#v, want database exploded", event.Exception)
	}
	if !sentryEventHasStacktrace(event) {
		t.Fatalf("event has no stacktrace: threads=%#v exceptions=%#v", event.Threads, event.Exception)
	}

	panicContext := event.Contexts["lazuli_panic"]
	if panicContext["scope"] != "http_command" {
		t.Fatalf("panic context scope = %v, want http_command", panicContext["scope"])
	}
	stack, ok := panicContext["stack"].(string)
	if !ok {
		t.Fatalf("panic context stack = %T, want string", panicContext["stack"])
	}
	if strings.Contains(stack, "C:/private/project") {
		t.Fatalf("stack = %q, want redacted path", stack)
	}
	if !strings.Contains(stack, "customer.gen.go:77") {
		t.Fatalf("stack = %q, want basename and line", stack)
	}
	if panicContext["recovered_at"] != "2026-05-12T20:10:00Z" {
		t.Fatalf("recovered_at = %v, want timestamp", panicContext["recovered_at"])
	}
}

func sentryEventHasStacktrace(event *sentry.Event) bool {
	if event == nil {
		return false
	}
	for _, thread := range event.Threads {
		if thread.Stacktrace != nil {
			return true
		}
	}
	for _, exception := range event.Exception {
		if exception.Stacktrace != nil {
			return true
		}
	}
	return false
}

func TestSentryReporterCaptureErrorAddsOperationTags(t *testing.T) {
	transport := &sentryCaptureTransport{}
	reporter, err := NewSentryReporter(SentryConfig{
		DSN:       "http://public@example.com/1",
		Transport: transport,
	})
	if err != nil {
		t.Fatalf("NewSentryReporter returned error: %v", err)
	}
	t.Cleanup(reporter.Close)

	ctx, end := StartOp(context.Background(), OpTag{
		Feature: "billing",
		Kind:    "job",
		Name:    "send_invoice",
		Source:  "features/billing.lzi:7:3",
	})
	defer end()

	eventID := reporter.CaptureError(ctx, errors.New("smtp rejected message"))
	if eventID == nil {
		t.Fatal("CaptureError returned nil event id")
	}

	events := transport.Events()
	if len(events) != 1 {
		t.Fatalf("events = %d, want 1", len(events))
	}
	event := events[0]
	if event.Level != sentry.LevelError {
		t.Fatalf("Level = %q, want error", event.Level)
	}
	if len(event.Exception) == 0 || event.Exception[0].Value != "smtp rejected message" {
		t.Fatalf("Exception = %#v, want smtp rejected message", event.Exception)
	}
	if event.Tags["lazuli.feature"] != "billing" || event.Tags["lazuli.kind"] != "job" || event.Tags["lazuli.op"] != "send_invoice" {
		t.Fatalf("operation tags = %s/%s/%s, want billing/job/send_invoice", event.Tags["lazuli.feature"], event.Tags["lazuli.kind"], event.Tags["lazuli.op"])
	}
	if event.Tags["lazuli.source"] != "features/billing.lzi:7:3" {
		t.Fatalf("source tag = %q, want features/billing.lzi:7:3", event.Tags["lazuli.source"])
	}
	if event.Transaction != "billing.job.send_invoice" {
		t.Fatalf("Transaction = %q, want billing.job.send_invoice", event.Transaction)
	}
}

func TestSentryReporterCloseClosesTransport(t *testing.T) {
	transport := &sentryCaptureTransport{}
	reporter, err := NewSentryReporter(SentryConfig{
		DSN:       "http://public@example.com/1",
		Transport: transport,
	})
	if err != nil {
		t.Fatalf("NewSentryReporter returned error: %v", err)
	}

	if !reporter.Flush(context.Background()) {
		t.Fatal("Flush = false, want true")
	}
	reporter.Close()
	if !transport.Closed() {
		t.Fatal("transport was not closed")
	}
}
