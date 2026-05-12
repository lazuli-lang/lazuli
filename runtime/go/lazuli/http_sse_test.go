package lazuli

import (
	"errors"
	"net/http"
	"net/http/httptest"
	"testing"
	"time"
)

func TestWriteSSEFormatsFrameAndHeaders(t *testing.T) {
	rec := httptest.NewRecorder()

	err := WriteSSE(rec, SSEEvent{
		Event: "customer.updated",
		Data:  "first line\nsecond line",
		ID:    "evt-123",
		Retry: 1500 * time.Millisecond,
	})
	if err != nil {
		t.Fatalf("WriteSSE error = %v", err)
	}

	const wantBody = "event: customer.updated\nid: evt-123\nretry: 1500\ndata: first line\ndata: second line\n\n"
	if got := rec.Body.String(); got != wantBody {
		t.Fatalf("body = %q, want %q", got, wantBody)
	}

	result := rec.Result()
	if got := result.Header.Get("Content-Type"); got != "text/event-stream" {
		t.Fatalf("Content-Type = %q, want text/event-stream", got)
	}
	if got := result.Header.Get("Cache-Control"); got != "no-cache" {
		t.Fatalf("Cache-Control = %q, want no-cache", got)
	}
	if got := result.Header.Get("Connection"); got != "keep-alive" {
		t.Fatalf("Connection = %q, want keep-alive", got)
	}
	if got := result.Header.Get("X-Accel-Buffering"); got != "no" {
		t.Fatalf("X-Accel-Buffering = %q, want no", got)
	}
	if !rec.Flushed {
		t.Fatal("response was not flushed")
	}
}

func TestWriteSSEPreservesDataLineBreaks(t *testing.T) {
	rec := httptest.NewRecorder()

	err := WriteSSE(rec, SSEEvent{Data: "one\r\n\r\nthree\r"})
	if err != nil {
		t.Fatalf("WriteSSE error = %v", err)
	}

	const wantBody = "data: one\ndata: \ndata: three\ndata: \n\n"
	if got := rec.Body.String(); got != wantBody {
		t.Fatalf("body = %q, want %q", got, wantBody)
	}
}

func TestWriteSSERejectsInvalidRetry(t *testing.T) {
	tests := []time.Duration{
		-time.Second,
		time.Microsecond,
	}

	for _, retry := range tests {
		rec := httptest.NewRecorder()
		err := WriteSSE(rec, SSEEvent{Data: "test", Retry: retry})
		if !errors.Is(err, ErrSSEInvalidRetry) {
			t.Fatalf("WriteSSE retry %s error = %v, want ErrSSEInvalidRetry", retry, err)
		}
		if rec.Body.Len() != 0 {
			t.Fatalf("WriteSSE retry %s wrote body %q", retry, rec.Body.String())
		}
	}
}

func TestWriteSSERejectsMultilineEventFields(t *testing.T) {
	tests := []SSEEvent{
		{Event: "created\nid: injected", Data: "test"},
		{ID: "evt-1\rretry: 1", Data: "test"},
	}

	for _, event := range tests {
		rec := httptest.NewRecorder()
		err := WriteSSE(rec, event)
		if err == nil {
			t.Fatal("WriteSSE error = nil, want invalid field error")
		}
		if rec.Body.Len() != 0 {
			t.Fatalf("WriteSSE wrote body %q", rec.Body.String())
		}
	}
}

func TestWriteSSEReturnsWriterError(t *testing.T) {
	wantErr := errors.New("write failed")
	writer := &failingResponseWriter{
		header: make(http.Header),
		err:    wantErr,
	}

	err := WriteSSE(writer, SSEEvent{Data: "test"})
	if !errors.Is(err, wantErr) {
		t.Fatalf("WriteSSE error = %v, want %v", err, wantErr)
	}
}

type failingResponseWriter struct {
	header http.Header
	err    error
}

func (w *failingResponseWriter) Header() http.Header {
	return w.header
}

func (w *failingResponseWriter) Write([]byte) (int, error) {
	return 0, w.err
}

func (w *failingResponseWriter) WriteHeader(int) {}
