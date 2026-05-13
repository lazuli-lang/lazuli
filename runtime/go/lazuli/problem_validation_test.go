package lazuli

import (
	"encoding/json"
	"reflect"
	"testing"
)

func TestValidationProblemExtensionsNormalizesAndSortsViolations(t *testing.T) {
	extensions := ValidationProblemExtensions(
		ProblemValidationViolation{
			Location: " query ",
			Field:    " page ",
			Code:     " minimum ",
			Message:  " must be at least 1 ",
		},
		ProblemValidationViolation{},
		ProblemValidationViolation{
			Field:     " email ",
			Path:      " input.email ",
			Code:      " required ",
			Message:   " is required ",
			InputType: " string ",
		},
		ProblemValidationViolation{
			Location: "body",
			Field:    "email",
			Code:     "invalid_format",
			Message:  "bad",
		},
	)

	raw, err := json.Marshal(extensions)
	if err != nil {
		t.Fatalf("Marshal extensions error: %v", err)
	}
	want := `{"code":"validation_failed","violations":[{"field":"email","path":"input.email","code":"required","message":"is required","input_type":"string"},{"location":"body","field":"email","code":"invalid_format","message":"bad"},{"location":"query","field":"page","code":"minimum","message":"must be at least 1"}]}`
	if string(raw) != want {
		t.Fatalf("extensions JSON = %s, want %s", raw, want)
	}
}

func TestValidationProblemExtensionsKeepsEmptyViolationSliceDeterministic(t *testing.T) {
	raw, err := json.Marshal(ValidationProblemExtensions())
	if err != nil {
		t.Fatalf("Marshal empty extensions error: %v", err)
	}
	want := `{"code":"validation_failed","violations":[]}`
	if string(raw) != want {
		t.Fatalf("empty extensions JSON = %s, want %s", raw, want)
	}
}

func TestValidationProblemExtensionsFromViolationsOmitsStatus(t *testing.T) {
	extensions := ValidationProblemExtensionsFromViolations(
		ValidationViolation{
			Location: "query",
			Field:    "page",
			Code:     "minimum",
			Message:  "page must be at least 1",
			Status:   422,
		},
		ValidationViolation{
			Location: "header",
			Field:    "X-Request-ID",
			Code:     "required",
			Message:  "request id is required",
			Status:   400,
		},
	)

	raw, err := json.Marshal(extensions)
	if err != nil {
		t.Fatalf("Marshal violation extensions error: %v", err)
	}
	want := `{"code":"validation_failed","violations":[{"location":"header","field":"X-Request-ID","code":"required","message":"request id is required"},{"location":"query","field":"page","code":"minimum","message":"page must be at least 1"}]}`
	if string(raw) != want {
		t.Fatalf("violation extensions JSON = %s, want %s", raw, want)
	}
}

func TestProblemValidationViolationsFromFields(t *testing.T) {
	got := ProblemValidationViolationsFromFields(
		ProblemValidationField{
			Field:     "password",
			Path:      "input.password",
			Code:      "too_short",
			Message:   "password is too short",
			InputType: "string",
		},
		ProblemValidationField{
			Field:   "email",
			Path:    "input.email",
			Code:    "required",
			Message: "email is required",
		},
	)

	want := []ProblemValidationViolation{
		{
			Field:   "email",
			Path:    "input.email",
			Code:    "required",
			Message: "email is required",
		},
		{
			Field:     "password",
			Path:      "input.password",
			Code:      "too_short",
			Message:   "password is too short",
			InputType: "string",
		},
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("ProblemValidationViolationsFromFields() = %#v, want %#v", got, want)
	}
}

func TestProblemValidationViolationsFromFieldErrorsMapsReasonFallback(t *testing.T) {
	got := ProblemValidationViolationsFromFieldErrors(
		nil,
		&FieldError{
			Base: ErrorBase{
				Message: "password does not match confirmation",
				Code:    "password_mismatch",
			},
			Field:     "password",
			Path:      "input.password",
			Reason:    FieldReasonMismatch,
			InputType: "string",
		},
		&FieldError{
			Base: ErrorBase{
				Message: "email must be valid",
			},
			Field:     "email",
			Path:      "input.email",
			Reason:    FieldReasonInvalidFormat,
			InputType: "string",
		},
	)

	want := []ProblemValidationViolation{
		{
			Field:     "email",
			Path:      "input.email",
			Code:      "invalid_format",
			Message:   "email must be valid",
			InputType: "string",
		},
		{
			Field:     "password",
			Path:      "input.password",
			Code:      "password_mismatch",
			Message:   "password does not match confirmation",
			InputType: "string",
		},
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("ProblemValidationViolationsFromFieldErrors() = %#v, want %#v", got, want)
	}
}
