package lazuli

import (
	"errors"
	"testing"
)

func TestTypedDecodeDecodesConcreteTarget(t *testing.T) {
	var out typedDecodeTarget

	err := TypedDecode(map[string]any{"name": "Ada", "age": 37}, &out)
	if err != nil {
		t.Fatalf("TypedDecode() error = %v, want nil", err)
	}
	if out.Name != "Ada" || out.Age != 37 {
		t.Fatalf("decoded = %#v, want Ada age 37", out)
	}
}

func TestTypedDecodeReturnsValidationOnTypeMismatch(t *testing.T) {
	var out typedDecodeTarget

	err := TypedDecode(map[string]any{"name": "Ada", "age": "old"}, &out)
	var le *Error
	if !errors.As(err, &le) {
		t.Fatalf("TypedDecode() error type = %T, want *Error", err)
	}
	if le.Status != 400 || le.Code != CodeValidationFailed {
		t.Fatalf("TypedDecode() = status %d code %q, want 400 %q", le.Status, le.Code, CodeValidationFailed)
	}
	fields, ok := le.Data.(map[string]any)["fields"].([]string)
	if !ok || len(fields) == 0 {
		t.Fatalf("TypedDecode() data = %#v, want field paths", le.Data)
	}
}

func TestTypedDecodeReturnsValidationOnNilTarget(t *testing.T) {
	err := TypedDecode(map[string]any{"name": "Ada"}, nil)
	var le *Error
	if !errors.As(err, &le) || le.Code != CodeValidationFailed {
		t.Fatalf("TypedDecode(nil target) = %v, want validation *Error", err)
	}
	fields := le.Data.(map[string]any)["fields"].([]string)
	if len(fields) != 1 || fields[0] != "target" {
		t.Fatalf("fields = %#v, want [target]", fields)
	}
}

func TestTypedDecodeReturnsValidationOnMarshalFailure(t *testing.T) {
	err := TypedDecode(map[string]any{"bad": func() {}}, &typedDecodeTarget{})
	var le *Error
	if !errors.As(err, &le) || le.Code != CodeValidationFailed {
		t.Fatalf("TypedDecode(marshal failure) = %v, want validation *Error", err)
	}
}

type typedDecodeTarget struct {
	Name string `json:"name"`
	Age  int    `json:"age"`
}
