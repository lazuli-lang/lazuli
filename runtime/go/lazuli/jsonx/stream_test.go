package jsonx

import (
	"context"
	"encoding/json"
	"errors"
	"io"
	"net/http/httptest"
	"strings"
	"testing"
)

func TestArrayWriterStreamsValues(t *testing.T) {
	var buf strings.Builder
	array := NewArrayWriter(&buf)

	if err := array.Write(map[string]int{"id": 1}); err != nil {
		t.Fatalf("Write first item error = %v", err)
	}
	if err := array.Write("two"); err != nil {
		t.Fatalf("Write second item error = %v", err)
	}
	if err := array.Write(nil); err != nil {
		t.Fatalf("Write third item error = %v", err)
	}
	if err := array.Close(); err != nil {
		t.Fatalf("Close error = %v", err)
	}

	const want = `[{"id":1},"two",null]`
	if got := buf.String(); got != want {
		t.Fatalf("body = %q, want %q", got, want)
	}
	if !json.Valid([]byte(buf.String())) {
		t.Fatalf("body is invalid json: %q", buf.String())
	}
}

func TestArrayWriterClosesEmptyArray(t *testing.T) {
	var buf strings.Builder
	array := NewArrayWriter(&buf)

	if err := array.Close(); err != nil {
		t.Fatalf("Close error = %v", err)
	}

	if got := buf.String(); got != "[]" {
		t.Fatalf("body = %q, want []", got)
	}
}

func TestArrayWriterRejectsUnsupportedValueWithoutWriting(t *testing.T) {
	var buf strings.Builder
	array := NewArrayWriter(&buf)

	err := array.Write(func() {})
	if err == nil {
		t.Fatal("Write error = nil, want encoding error")
	}
	if got := buf.String(); got != "" {
		t.Fatalf("body = %q, want empty", got)
	}

	if err := array.Write(1); err != nil {
		t.Fatalf("Write valid item after encoding error = %v", err)
	}
	if err := array.Close(); err != nil {
		t.Fatalf("Close error = %v", err)
	}
	if got := buf.String(); got != "[1]" {
		t.Fatalf("body = %q, want [1]", got)
	}
}

func TestArrayWriterPropagatesWriterError(t *testing.T) {
	wantErr := errors.New("write failed")
	array := NewArrayWriter(failingWriter{err: wantErr})

	err := array.Write(1)
	if !errors.Is(err, wantErr) {
		t.Fatalf("Write error = %v, want %v", err, wantErr)
	}

	err = array.Close()
	if !errors.Is(err, wantErr) {
		t.Fatalf("Close error = %v, want %v", err, wantErr)
	}
}

func TestArrayWriterAbortStopsFurtherWrites(t *testing.T) {
	var buf strings.Builder
	array := NewArrayWriter(&buf)

	if err := array.Write(1); err != nil {
		t.Fatalf("Write error = %v", err)
	}
	array.Abort()

	if err := array.Write(2); !errors.Is(err, ErrStreamAborted) {
		t.Fatalf("Write after Abort error = %v, want ErrStreamAborted", err)
	}
	if err := array.Close(); !errors.Is(err, ErrStreamAborted) {
		t.Fatalf("Close after Abort error = %v, want ErrStreamAborted", err)
	}
	if got := buf.String(); got != "[1" {
		t.Fatalf("body = %q, want aborted partial stream", got)
	}
}

func TestObjectWriterStreamsFields(t *testing.T) {
	var buf strings.Builder
	object := NewObjectWriter(&buf)

	if err := object.Write(`a"b`, 1); err != nil {
		t.Fatalf("Write first field error = %v", err)
	}
	if err := object.Write("nested", map[string]bool{"ok": true}); err != nil {
		t.Fatalf("Write second field error = %v", err)
	}
	if err := object.Close(); err != nil {
		t.Fatalf("Close error = %v", err)
	}

	const want = `{"a\"b":1,"nested":{"ok":true}}`
	if got := buf.String(); got != want {
		t.Fatalf("body = %q, want %q", got, want)
	}
	if !json.Valid([]byte(buf.String())) {
		t.Fatalf("body is invalid json: %q", buf.String())
	}
}

func TestStreamArrayUsesCustomEncoder(t *testing.T) {
	var buf strings.Builder

	err := StreamArray(context.Background(), &buf, []int{1, 2, 3}, func(w io.Writer, value int) error {
		return json.NewEncoder(w).Encode(value)
	})
	if err != nil {
		t.Fatalf("StreamArray error = %v", err)
	}

	if got := buf.String(); got != "[1,2,3]" {
		t.Fatalf("body = %q, want [1,2,3]", got)
	}
}

func TestStreamArrayNilEncoderUsesJSONMarshal(t *testing.T) {
	var buf strings.Builder

	err := StreamArray(context.Background(), &buf, []string{"a", "b"}, nil)
	if err != nil {
		t.Fatalf("StreamArray error = %v", err)
	}

	if got := buf.String(); got != `["a","b"]` {
		t.Fatalf("body = %q, want %q", got, `["a","b"]`)
	}
}

func TestStreamArrayRejectsInvalidCustomJSON(t *testing.T) {
	var buf strings.Builder

	err := StreamArray(context.Background(), &buf, []int{1}, func(w io.Writer, value int) error {
		_, err := io.WriteString(w, "not json")
		return err
	})
	if !errors.Is(err, ErrInvalidJSON) {
		t.Fatalf("StreamArray error = %v, want ErrInvalidJSON", err)
	}
	if got := buf.String(); got != "" {
		t.Fatalf("body = %q, want empty", got)
	}
}

func TestStreamArrayReturnsContextCancellation(t *testing.T) {
	var buf strings.Builder
	ctx, cancel := context.WithCancel(context.Background())
	calls := 0

	err := StreamArray(ctx, &buf, []int{1, 2}, func(w io.Writer, value int) error {
		calls++
		if err := json.NewEncoder(w).Encode(value); err != nil {
			return err
		}
		cancel()
		return nil
	})
	if !errors.Is(err, context.Canceled) {
		t.Fatalf("StreamArray error = %v, want context.Canceled", err)
	}
	if calls != 1 {
		t.Fatalf("encoder calls = %d, want 1", calls)
	}
	if got := buf.String(); got != "[1" {
		t.Fatalf("body = %q, want aborted partial stream", got)
	}
}

func TestWritersSetJSONStreamHeadersAndFlush(t *testing.T) {
	rec := httptest.NewRecorder()
	array := NewArrayWriter(rec)

	if err := array.Write(1); err != nil {
		t.Fatalf("Write error = %v", err)
	}
	if err := array.Close(); err != nil {
		t.Fatalf("Close error = %v", err)
	}

	result := rec.Result()
	if got := result.Header.Get("Content-Type"); got != contentTypeJSON {
		t.Fatalf("Content-Type = %q, want %q", got, contentTypeJSON)
	}
	if got := result.Header.Get("X-Accel-Buffering"); got != xAccelBufferingOff {
		t.Fatalf("X-Accel-Buffering = %q, want %q", got, xAccelBufferingOff)
	}
	if !rec.Flushed {
		t.Fatal("response was not flushed")
	}
}

func TestNilWriterReturnsErrNilWriter(t *testing.T) {
	array := NewArrayWriter(nil)

	err := array.Write(1)
	if !errors.Is(err, ErrNilWriter) {
		t.Fatalf("Write error = %v, want ErrNilWriter", err)
	}
}

type failingWriter struct {
	err error
}

func (w failingWriter) Write([]byte) (int, error) {
	return 0, w.err
}
