package lazuli

import (
	"errors"
	"fmt"
	"testing"
)

func TestTypedFieldErrorAsAndIs(t *testing.T) {
	sentinel := errors.New("password mismatch")
	err := fmt.Errorf("handler failed: %w", &FieldError{
		Base: ErrorBase{
			Code:    "field_invalid",
			Message: "password does not match confirmation",
			Source:  "features/auth.lzi:42:8",
			Origin:  OriginUserDSL,
			Cause:   sentinel,
		},
		Field:     "password",
		Path:      "input.identity.password",
		Reason:    FieldReasonMismatch,
		InputType: "string",
	})

	var got *FieldError
	if !errors.As(err, &got) {
		t.Fatal("errors.As did not recover FieldError")
	}
	if got.Base.Code != "field_invalid" {
		t.Fatalf("Base.Code = %q, want field_invalid", got.Base.Code)
	}
	if got.Base.Message != "password does not match confirmation" {
		t.Fatalf("Base.Message = %q, want password does not match confirmation", got.Base.Message)
	}
	if got.Base.Source != "features/auth.lzi:42:8" {
		t.Fatalf("Base.Source = %q, want features/auth.lzi:42:8", got.Base.Source)
	}
	if got.Base.Origin != OriginUserDSL {
		t.Fatalf("Base.Origin = %v, want %v", got.Base.Origin, OriginUserDSL)
	}
	if got.Field != "password" {
		t.Fatalf("Field = %q, want password", got.Field)
	}
	if got.Path != "input.identity.password" {
		t.Fatalf("Path = %q, want input.identity.password", got.Path)
	}
	if got.Reason != FieldReasonMismatch {
		t.Fatalf("Reason = %v, want %v", got.Reason, FieldReasonMismatch)
	}
	if got.InputType != "string" {
		t.Fatalf("InputType = %q, want string", got.InputType)
	}
	if !errors.Is(err, sentinel) {
		t.Fatal("errors.Is did not walk FieldError Base.Cause")
	}
}

func TestTypedErrorUnwrapsCause(t *testing.T) {
	sentinel := errors.New("policy denied")
	err := &PolicyError{
		Base: ErrorBase{
			Code:   "policy_denied",
			Origin: OriginUserDSL,
			Cause:  sentinel,
		},
		Rule:     "can_create_invoice",
		Subject:  "user:7",
		Resource: "invoice",
		Tenant:   "tenant:1",
	}

	if !errors.Is(err, sentinel) {
		t.Fatal("errors.Is did not walk PolicyError Base.Cause")
	}
	if errors.Unwrap(err) != sentinel {
		t.Fatal("errors.Unwrap did not return PolicyError Base.Cause")
	}
}

func TestTypedErrorImpliedOriginInvariants(t *testing.T) {
	tests := []struct {
		name string
		err  error
		want Origin
	}{
		{
			name: "field",
			err: &FieldError{
				Base: ErrorBase{Origin: OriginUserDSL},
			},
			want: OriginUserDSL,
		},
		{
			name: "policy",
			err: &PolicyError{
				Base: ErrorBase{Origin: OriginUserDSL},
			},
			want: OriginUserDSL,
		},
		{
			name: "tenant",
			err: &TenantError{
				Base: ErrorBase{Origin: OriginUserDSL},
			},
			want: OriginUserDSL,
		},
		{
			name: "adapter",
			err: &AdapterError{
				Base: ErrorBase{Origin: OriginAdapterRuntime},
			},
			want: OriginAdapterRuntime,
		},
		{
			name: "lib bug",
			err: &LibBugError{
				Base: ErrorBase{Origin: OriginLibInternal},
			},
			want: OriginLibInternal,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, ok := c121TypedErrorImpliedOrigin(tt.err)
			if !ok {
				t.Fatalf("%T has no implied origin", tt.err)
			}
			if got != tt.want {
				t.Fatalf("implied origin = %v, want %v", got, tt.want)
			}
			if !c121TypedErrorOriginMatches(tt.err) {
				t.Fatalf("%T Base.Origin does not match implied origin %v", tt.err, tt.want)
			}
		})
	}
}

func TestTypedErrorImpliedOriginInvariantRejectsMismatch(t *testing.T) {
	tests := []struct {
		name string
		err  error
	}{
		{
			name: "field",
			err: &FieldError{
				Base: ErrorBase{Origin: OriginLibInternal},
			},
		},
		{
			name: "policy",
			err: &PolicyError{
				Base: ErrorBase{Origin: OriginAdapterRuntime},
			},
		},
		{
			name: "tenant",
			err: &TenantError{
				Base: ErrorBase{Origin: OriginCodegenBug},
			},
		},
		{
			name: "adapter",
			err: &AdapterError{
				Base: ErrorBase{Origin: OriginUserDSL},
			},
		},
		{
			name: "lib bug",
			err: &LibBugError{
				Base: ErrorBase{Origin: OriginAdapterRuntime},
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if c121TypedErrorOriginMatches(tt.err) {
				t.Fatalf("%T with mismatched Base.Origin passed invariant", tt.err)
			}
		})
	}
}

func c121TypedErrorImpliedOrigin(err error) (Origin, bool) {
	switch err.(type) {
	case *FieldError, *PolicyError, *TenantError:
		return OriginUserDSL, true
	case *AdapterError:
		return OriginAdapterRuntime, true
	case *LibBugError:
		return OriginLibInternal, true
	default:
		return 0, false
	}
}

func c121TypedErrorOriginMatches(err error) bool {
	base, ok := c121TypedErrorBase(err)
	if !ok {
		return false
	}
	implied, ok := c121TypedErrorImpliedOrigin(err)
	return ok && base.Origin == implied
}

func c121TypedErrorBase(err error) (ErrorBase, bool) {
	switch e := err.(type) {
	case *FieldError:
		return e.Base, true
	case *PolicyError:
		return e.Base, true
	case *TenantError:
		return e.Base, true
	case *AdapterError:
		return e.Base, true
	case *LibBugError:
		return e.Base, true
	default:
		return ErrorBase{}, false
	}
}
