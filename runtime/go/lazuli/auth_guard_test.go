package lazuli

import (
	"errors"
	"testing"
)

func TestAuthGuardAllowsAuthenticatedUser(t *testing.T) {
	err := AuthGuard(&Ctx{User: &User{ID: 42}})
	if err != nil {
		t.Fatalf("AuthGuard() error = %v, want nil", err)
	}
}

func TestAuthGuardRejectsNilUserWithTypedError(t *testing.T) {
	err := AuthGuard(&Ctx{})
	if err == nil {
		t.Fatal("AuthGuard() error = nil, want unauthenticated")
	}
	var le *Error
	if !errors.As(err, &le) {
		t.Fatalf("AuthGuard() error type = %T, want *Error", err)
	}
	if le.Status != 401 || le.Code != CodeUnauthenticated {
		t.Fatalf("AuthGuard() = status %d code %q, want 401 %q", le.Status, le.Code, CodeUnauthenticated)
	}
}

func TestAuthGuardRejectsNilCtx(t *testing.T) {
	err := AuthGuard(nil)
	if err == nil {
		t.Fatal("AuthGuard(nil) error = nil, want unauthenticated")
	}
	var le *Error
	if !errors.As(err, &le) || le.Code != CodeUnauthenticated {
		t.Fatalf("AuthGuard(nil) = %v, want %q", err, CodeUnauthenticated)
	}
}
