package runtime

import (
	"errors"
	"testing"

	"lazuli.dev/runtime/lazuli"
)

func TestErrReferencedInUseHasCanonicalCode(t *testing.T) {
	if ErrReferencedInUse.Code != CodeReferencedInUse {
		t.Fatalf("bare sentinel code = %q, want %q", ErrReferencedInUse.Code, CodeReferencedInUse)
	}
}

func TestNewReferencedInUseErrorPinsDomainCode(t *testing.T) {
	err := NewReferencedInUseError("CATEGORY_HAS_CUSTOMERS")
	if err.Code != "CATEGORY_HAS_CUSTOMERS" {
		t.Fatalf("code = %q, want CATEGORY_HAS_CUSTOMERS", err.Code)
	}
	// The domain-coded variant still Is-es back to the generic sentinel.
	if !errors.Is(err, ErrReferencedInUse) {
		t.Fatal("domain-coded error must match ErrReferencedInUse sentinel")
	}
}

func TestNewReferencedInUseErrorEmptyFallsBack(t *testing.T) {
	if got := NewReferencedInUseError("").Code; got != CodeReferencedInUse {
		t.Fatalf("empty code fallback = %q, want %q", got, CodeReferencedInUse)
	}
}

func TestReferencedInUseErrorProjectsTo409Envelope(t *testing.T) {
	err := NewReferencedInUseError("CATEGORY_HAS_CUSTOMERS")
	var le *lazuli.Error
	if !errors.As(err, &le) {
		t.Fatal("must project onto *lazuli.Error")
	}
	if le.Status != 409 {
		t.Fatalf("status = %d, want 409", le.Status)
	}
	if le.Code != "CATEGORY_HAS_CUSTOMERS" {
		t.Fatalf("envelope code = %q, want CATEGORY_HAS_CUSTOMERS", le.Code)
	}
	if le.MessageKey != "CATEGORY_HAS_CUSTOMERS" {
		t.Fatalf("envelope MessageKey = %q, want CATEGORY_HAS_CUSTOMERS", le.MessageKey)
	}
}

func TestBareSentinelProjectsTo409Envelope(t *testing.T) {
	var le *lazuli.Error
	if !errors.As(ErrReferencedInUse, &le) {
		t.Fatal("bare sentinel must project onto *lazuli.Error")
	}
	if le.Status != 409 || le.Code != CodeReferencedInUse {
		t.Fatalf("bare envelope = (%d, %q), want (409, %q)", le.Status, le.Code, CodeReferencedInUse)
	}
}
