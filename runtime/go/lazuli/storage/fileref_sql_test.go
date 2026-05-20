package storage_test

import (
	"strings"
	"testing"

	"lazuli.dev/runtime/lazuli/storage"
)

const fileRefPayload = `{"key":"k1","content_type":"image/jpeg","size":123}`

var fileRefWant = storage.FileRef{Key: "k1", ContentType: "image/jpeg", Size: 123}

func TestFileRefScan_NilZeroesValue(t *testing.T) {
	t.Parallel()

	f := storage.FileRef{Key: "existing", ContentType: "image/png", Size: 42}
	if err := f.Scan(nil); err != nil {
		t.Fatalf("Scan(nil) failed: %v", err)
	}
	if f != (storage.FileRef{}) {
		t.Fatalf("Scan(nil) = %#v, want zero FileRef", f)
	}
}

func TestFileRefScan_BytesRoundtrip(t *testing.T) {
	t.Parallel()

	var f storage.FileRef
	if err := f.Scan([]byte(fileRefPayload)); err != nil {
		t.Fatalf("Scan([]byte) failed: %v", err)
	}
	if f != fileRefWant {
		t.Fatalf("Scan([]byte) = %#v, want %#v", f, fileRefWant)
	}
}

func TestFileRefScan_StringRoundtrip(t *testing.T) {
	t.Parallel()

	var f storage.FileRef
	if err := f.Scan(fileRefPayload); err != nil {
		t.Fatalf("Scan(string) failed: %v", err)
	}
	if f != fileRefWant {
		t.Fatalf("Scan(string) = %#v, want %#v", f, fileRefWant)
	}
}

func TestFileRefScan_UnsupportedTypeError(t *testing.T) {
	t.Parallel()

	var f storage.FileRef
	err := f.Scan(42)
	if err == nil {
		t.Fatal("Scan(42) succeeded, want error")
	}
	if !strings.Contains(err.Error(), "unsupported type") {
		t.Fatalf("Scan(42) error = %q, want unsupported type", err)
	}
}

func TestFileRefScan_MalformedJSONError(t *testing.T) {
	t.Parallel()

	var f storage.FileRef
	if err := f.Scan([]byte("not json")); err == nil {
		t.Fatal("Scan(malformed JSON) succeeded, want error")
	}
}

func TestFileRefValue_EmptyKeyReturnsNil(t *testing.T) {
	t.Parallel()

	got, err := (storage.FileRef{}).Value()
	if err != nil {
		t.Fatalf("Value() failed: %v", err)
	}
	if got != nil {
		t.Fatalf("Value() = %#v, want nil", got)
	}
}

func TestFileRefValue_PopulatedRoundtripsThroughScan(t *testing.T) {
	t.Parallel()

	want := storage.FileRef{
		Key:         storage.Key("uploads/photo.jpg"),
		ContentType: "image/jpeg",
		Size:        456,
	}
	value, err := want.Value()
	if err != nil {
		t.Fatalf("Value() failed: %v", err)
	}

	var got storage.FileRef
	if err := got.Scan(value); err != nil {
		t.Fatalf("Scan(Value()) failed: %v", err)
	}
	if got != want {
		t.Fatalf("Scan(Value()) = %#v, want %#v", got, want)
	}
}
