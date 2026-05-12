package jsonx

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"io"
	"strings"
	"testing"
)

type testRecord struct {
	ID   int    `json:"id"`
	Name string `json:"name,omitempty"`
}

func TestEncoderWritesNDJSON(t *testing.T) {
	var out bytes.Buffer
	encoder := NewEncoder(&out)

	if err := encoder.Encode(context.Background(), testRecord{ID: 1}); err != nil {
		t.Fatalf("Encode first record: %v", err)
	}
	if err := encoder.Encode(context.Background(), testRecord{ID: 2, Name: "two"}); err != nil {
		t.Fatalf("Encode second record: %v", err)
	}

	const want = "{\"id\":1}\n{\"id\":2,\"name\":\"two\"}\n"
	if got := out.String(); got != want {
		t.Fatalf("encoded stream = %q, want %q", got, want)
	}
}

func TestEncoderHonorsCanceledContext(t *testing.T) {
	var out bytes.Buffer
	encoder := NewEncoder(&out)
	ctx, cancel := context.WithCancel(context.Background())
	cancel()

	err := encoder.Encode(ctx, testRecord{ID: 1})
	if !errors.Is(err, context.Canceled) {
		t.Fatalf("Encode error = %v, want context.Canceled", err)
	}
	if out.Len() != 0 {
		t.Fatalf("encoded %d bytes after context cancellation", out.Len())
	}
}

func TestDecoderDecodesRecords(t *testing.T) {
	decoder := NewDecoder(strings.NewReader("{\"id\":1}\n{\"id\":2,\"name\":\"two\"}\n"))

	var first testRecord
	if err := decoder.Decode(context.Background(), &first); err != nil {
		t.Fatalf("Decode first record: %v", err)
	}
	if first != (testRecord{ID: 1}) {
		t.Fatalf("first record = %+v", first)
	}
	if decoder.Line() != 1 {
		t.Fatalf("Line after first Decode = %d, want 1", decoder.Line())
	}

	var second testRecord
	if err := decoder.Decode(context.Background(), &second); err != nil {
		t.Fatalf("Decode second record: %v", err)
	}
	if second != (testRecord{ID: 2, Name: "two"}) {
		t.Fatalf("second record = %+v", second)
	}
	if decoder.Line() != 2 {
		t.Fatalf("Line after second Decode = %d, want 2", decoder.Line())
	}

	if err := decoder.Decode(context.Background(), &testRecord{}); !errors.Is(err, io.EOF) {
		t.Fatalf("Decode EOF error = %v, want io.EOF", err)
	}
}

func TestDecoderSkipsEmptyLines(t *testing.T) {
	decoder := NewDecoder(strings.NewReader("\n \t\r\n{\"id\":7}\n"), WithSkipEmptyLines(true))

	var got testRecord
	if err := decoder.Decode(context.Background(), &got); err != nil {
		t.Fatalf("Decode record: %v", err)
	}
	if got != (testRecord{ID: 7}) {
		t.Fatalf("record = %+v", got)
	}
	if decoder.Line() != 3 {
		t.Fatalf("Line after skipped empty lines = %d, want 3", decoder.Line())
	}
}

func TestDecoderDecodeErrorIncludesLineNumber(t *testing.T) {
	decoder := NewDecoder(strings.NewReader("{\"id\":1}\n{bad}\n"))

	if err := decoder.Decode(context.Background(), &testRecord{}); err != nil {
		t.Fatalf("Decode first record: %v", err)
	}

	err := decoder.Decode(context.Background(), &testRecord{})
	var decodeErr *DecodeError
	if !errors.As(err, &decodeErr) {
		t.Fatalf("Decode error type = %T, want *DecodeError", err)
	}
	if decodeErr.Line != 2 {
		t.Fatalf("DecodeError.Line = %d, want 2", decodeErr.Line)
	}
	var syntaxErr *json.SyntaxError
	if !errors.As(err, &syntaxErr) {
		t.Fatalf("Decode error = %v, want wrapped *json.SyntaxError", err)
	}
}

func TestDecoderMaxLineBytes(t *testing.T) {
	longLine := strings.Repeat("x", 5000)
	decoder := NewDecoder(strings.NewReader(longLine+"\n{\"id\":2}\n"), WithMaxLineBytes(10))

	err := decoder.Decode(context.Background(), &testRecord{})
	if !errors.Is(err, ErrLineTooLong) {
		t.Fatalf("Decode error = %v, want ErrLineTooLong", err)
	}
	var decodeErr *DecodeError
	if !errors.As(err, &decodeErr) {
		t.Fatalf("Decode error type = %T, want *DecodeError", err)
	}
	if decodeErr.Line != 1 {
		t.Fatalf("DecodeError.Line = %d, want 1", decodeErr.Line)
	}

	var got testRecord
	if err := decoder.Decode(context.Background(), &got); err != nil {
		t.Fatalf("Decode after oversized line: %v", err)
	}
	if got != (testRecord{ID: 2}) {
		t.Fatalf("record after oversized line = %+v", got)
	}
	if decoder.Line() != 2 {
		t.Fatalf("Line after oversized line = %d, want 2", decoder.Line())
	}
}

func TestDecoderHonorsCanceledContext(t *testing.T) {
	decoder := NewDecoder(strings.NewReader("{\"id\":1}\n"))
	ctx, cancel := context.WithCancel(context.Background())
	cancel()

	err := decoder.Decode(ctx, &testRecord{})
	if !errors.Is(err, context.Canceled) {
		t.Fatalf("Decode error = %v, want context.Canceled", err)
	}
	if decoder.Line() != 0 {
		t.Fatalf("Line after canceled Decode = %d, want 0", decoder.Line())
	}
}
