package lazuli

import (
	"bytes"
	"context"
	"encoding/json"
	"log/slog"
	"net/http"
	"net/http/httptest"
	"testing"
	"time"
)

func TestSlowRequestMiddlewareObservesThresholdExceeded(t *testing.T) {
	observer := &recordingSlowRequestObserver{}
	handler := SlowRequestMiddleware(time.Millisecond, observer)(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		time.Sleep(2 * time.Millisecond)
		w.WriteHeader(http.StatusAccepted)
		_, _ = w.Write([]byte("accepted"))
	}))

	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodPost, "/api/v1/c/create?debug=1", nil)
	req.Header.Set(requestIDHeader, "req-123")

	handler.ServeHTTP(rec, req)

	if rec.Code != http.StatusAccepted {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusAccepted)
	}
	if got := rec.Body.String(); got != "accepted" {
		t.Fatalf("body = %q, want accepted", got)
	}

	event := observer.onlyEvent(t)
	if event.Method != http.MethodPost {
		t.Fatalf("event method = %q, want %s", event.Method, http.MethodPost)
	}
	if event.Path != "/api/v1/c/create" {
		t.Fatalf("event path = %q, want /api/v1/c/create", event.Path)
	}
	if event.Status != http.StatusAccepted {
		t.Fatalf("event status = %d, want %d", event.Status, http.StatusAccepted)
	}
	if event.RequestID != "req-123" {
		t.Fatalf("event request id = %q, want req-123", event.RequestID)
	}
	if event.Threshold != time.Millisecond {
		t.Fatalf("event threshold = %s, want 1ms", event.Threshold)
	}
	if event.Duration < time.Millisecond {
		t.Fatalf("event duration = %s, want at least 1ms", event.Duration)
	}
}

func TestSlowRequestMiddlewareSkipsFastRequests(t *testing.T) {
	observer := &recordingSlowRequestObserver{}
	handler := SlowRequestMiddleware(time.Hour, observer)(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.WriteHeader(http.StatusNoContent)
	}))

	rec := httptest.NewRecorder()
	handler.ServeHTTP(rec, httptest.NewRequest(http.MethodGet, "/", nil))

	if rec.Code != http.StatusNoContent {
		t.Fatalf("status = %d, want %d", rec.Code, http.StatusNoContent)
	}
	if got := len(observer.events); got != 0 {
		t.Fatalf("events = %d, want 0", got)
	}
}

func TestSlowRequestMiddlewareCapturesImplicitStatusAndMintedRequestID(t *testing.T) {
	observer := &recordingSlowRequestObserver{}
	handler := Chain(
		SlowRequestMiddleware(time.Nanosecond, observer),
		RequestIDMiddleware,
	)(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		time.Sleep(time.Millisecond)
		_, _ = w.Write([]byte("ok"))
	}))

	rec := httptest.NewRecorder()
	handler.ServeHTTP(rec, httptest.NewRequest(http.MethodGet, "/healthz", nil))

	responseID := rec.Result().Header.Get(requestIDHeader)
	if responseID == "" {
		t.Fatal("response request id is empty")
	}

	event := observer.onlyEvent(t)
	if event.Status != http.StatusOK {
		t.Fatalf("event status = %d, want %d", event.Status, http.StatusOK)
	}
	if event.RequestID != responseID {
		t.Fatalf("event request id = %q, want response id %q", event.RequestID, responseID)
	}
}

func TestSlowRequestMiddlewareAvoidsDuplicateWriteHeader(t *testing.T) {
	observer := &recordingSlowRequestObserver{}
	handler := SlowRequestMiddleware(time.Nanosecond, observer)(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		time.Sleep(time.Millisecond)
		w.WriteHeader(http.StatusCreated)
		w.WriteHeader(http.StatusInternalServerError)
		_, _ = w.Write([]byte("created"))
	}))

	rw := newCountingResponseWriter()
	handler.ServeHTTP(rw, httptest.NewRequest(http.MethodPost, "/widgets", nil))

	if rw.writeHeaderCalls != 1 {
		t.Fatalf("WriteHeader calls = %d, want 1", rw.writeHeaderCalls)
	}
	if rw.status != http.StatusCreated {
		t.Fatalf("status = %d, want %d", rw.status, http.StatusCreated)
	}
	if got := rw.body.String(); got != "created" {
		t.Fatalf("body = %q, want created", got)
	}

	event := observer.onlyEvent(t)
	if event.Status != http.StatusCreated {
		t.Fatalf("event status = %d, want %d", event.Status, http.StatusCreated)
	}
}

func TestSlowRequestMiddlewareDisabledLeavesHandlerUnchanged(t *testing.T) {
	tests := []struct {
		name      string
		threshold time.Duration
		observer  SlowRequestObserver
	}{
		{name: "zero threshold", threshold: 0, observer: &recordingSlowRequestObserver{}},
		{name: "negative threshold", threshold: -time.Second, observer: &recordingSlowRequestObserver{}},
		{name: "nil observer", threshold: time.Nanosecond, observer: nil},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			handler := SlowRequestMiddleware(tt.threshold, tt.observer)(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
				w.WriteHeader(http.StatusAccepted)
			}))

			rec := httptest.NewRecorder()
			handler.ServeHTTP(rec, httptest.NewRequest(http.MethodGet, "/", nil))

			if rec.Code != http.StatusAccepted {
				t.Fatalf("status = %d, want %d", rec.Code, http.StatusAccepted)
			}
		})
	}
}

func TestSlowRequestLoggerWritesStructuredWarning(t *testing.T) {
	var buf bytes.Buffer
	logger := slog.New(slog.NewJSONHandler(&buf, nil))

	SlowRequestLogger(logger).ObserveSlowRequest(context.Background(), SlowRequestEvent{
		Method:    http.MethodPatch,
		Path:      "/orders/42",
		Status:    http.StatusGatewayTimeout,
		RequestID: "req-log",
		Duration:  1500 * time.Millisecond,
		Threshold: time.Second,
	})

	var record map[string]any
	if err := json.Unmarshal(bytes.TrimSpace(buf.Bytes()), &record); err != nil {
		t.Fatalf("log JSON decode error: %v; log = %q", err, buf.String())
	}
	assertSlowRequestLogString(t, record, "level", "WARN")
	assertSlowRequestLogString(t, record, "msg", "lazuli slow http request")
	assertSlowRequestLogString(t, record, "method", http.MethodPatch)
	assertSlowRequestLogString(t, record, "path", "/orders/42")
	assertSlowRequestLogString(t, record, "request_id", "req-log")
	assertSlowRequestLogNumber(t, record, "status", http.StatusGatewayTimeout)
	assertSlowRequestLogNumber(t, record, "duration_ms", 1500)
	assertSlowRequestLogNumber(t, record, "threshold_ms", 1000)
}

type recordingSlowRequestObserver struct {
	events []SlowRequestEvent
}

func (o *recordingSlowRequestObserver) ObserveSlowRequest(_ context.Context, event SlowRequestEvent) {
	o.events = append(o.events, event)
}

func (o *recordingSlowRequestObserver) onlyEvent(t *testing.T) SlowRequestEvent {
	t.Helper()

	if got := len(o.events); got != 1 {
		t.Fatalf("events = %d, want 1: %#v", got, o.events)
	}
	return o.events[0]
}

type countingResponseWriter struct {
	header           http.Header
	body             bytes.Buffer
	status           int
	writeHeaderCalls int
}

func newCountingResponseWriter() *countingResponseWriter {
	return &countingResponseWriter{header: make(http.Header)}
}

func (w *countingResponseWriter) Header() http.Header {
	return w.header
}

func (w *countingResponseWriter) WriteHeader(status int) {
	w.writeHeaderCalls++
	if w.status == 0 {
		w.status = status
	}
}

func (w *countingResponseWriter) Write(p []byte) (int, error) {
	if w.status == 0 {
		w.WriteHeader(http.StatusOK)
	}
	return w.body.Write(p)
}

func assertSlowRequestLogString(t *testing.T, record map[string]any, key, want string) {
	t.Helper()

	got, ok := record[key].(string)
	if !ok {
		t.Fatalf("log %s = %#v, want string %q", key, record[key], want)
	}
	if got != want {
		t.Fatalf("log %s = %q, want %q", key, got, want)
	}
}

func assertSlowRequestLogNumber(t *testing.T, record map[string]any, key string, want float64) {
	t.Helper()

	got, ok := record[key].(float64)
	if !ok {
		t.Fatalf("log %s = %#v, want number %v", key, record[key], want)
	}
	if got != want {
		t.Fatalf("log %s = %v, want %v", key, got, want)
	}
}
