package debug

import (
	"encoding/json"
	"errors"
	"fmt"
	"strings"
	"testing"
)

type errorReportFieldError struct {
	Code    string
	Status  int
	message string
}

func (e *errorReportFieldError) Error() string {
	return e.message
}

type errorReportMethodError struct {
	message string
}

func (e errorReportMethodError) Error() string {
	return e.message
}

func (e errorReportMethodError) Code() string {
	return "rate_limited"
}

func (e errorReportMethodError) Status() int {
	return 429
}

type errorReportBase struct {
	Code   string
	Status int
}

type errorReportBaseError struct {
	Base    errorReportBase
	message string
}

func (e *errorReportBaseError) Error() string {
	return e.message
}

func TestBuildErrorReportNilReturnsZeroReport(t *testing.T) {
	report := BuildErrorReport(nil)

	if report.Message != "" {
		t.Fatalf("Message = %q, want empty", report.Message)
	}
	if len(report.Chain) != 0 {
		t.Fatalf("Chain length = %d, want 0", len(report.Chain))
	}

	encoded, err := json.Marshal(report)
	if err != nil {
		t.Fatalf("Marshal error: %v", err)
	}
	if string(encoded) != "{}" {
		t.Fatalf("JSON = %s, want {}", encoded)
	}
}

func TestBuildErrorReportIncludesWrappedChainAndFieldMetadata(t *testing.T) {
	cause := &errorReportFieldError{
		Code:    "validation_failed",
		Status:  422,
		message: "email is invalid",
	}
	err := fmt.Errorf("create customer: %w", cause)

	report := BuildErrorReport(err)

	if report.Message != "create customer: email is invalid" {
		t.Fatalf("Message = %q, want wrapped message", report.Message)
	}
	if report.Code != "validation_failed" {
		t.Fatalf("Code = %q, want validation_failed", report.Code)
	}
	if report.Status != 422 {
		t.Fatalf("Status = %d, want 422", report.Status)
	}
	if len(report.Chain) != 2 {
		t.Fatalf("Chain length = %d, want 2", len(report.Chain))
	}
	if report.Chain[0].Type != "*fmt.wrapError" {
		t.Fatalf("root Type = %q, want *fmt.wrapError", report.Chain[0].Type)
	}
	if !strings.HasSuffix(report.Chain[1].Type, ".errorReportFieldError") {
		t.Fatalf("cause Type = %q, want errorReportFieldError suffix", report.Chain[1].Type)
	}
	if report.Chain[1].Code != "validation_failed" {
		t.Fatalf("cause Code = %q, want validation_failed", report.Chain[1].Code)
	}
	if report.Chain[1].Status != 422 {
		t.Fatalf("cause Status = %d, want 422", report.Chain[1].Status)
	}
}

func TestBuildErrorReportUsesBaseCodeAndStatusFields(t *testing.T) {
	report := BuildErrorReport(&errorReportBaseError{
		Base: errorReportBase{
			Code:   "not_found",
			Status: 404,
		},
		message: "customer missing",
	})

	if report.Code != "not_found" {
		t.Fatalf("Code = %q, want not_found", report.Code)
	}
	if report.Status != 404 {
		t.Fatalf("Status = %d, want 404", report.Status)
	}
	if report.Chain[0].Code != "not_found" {
		t.Fatalf("chain Code = %q, want not_found", report.Chain[0].Code)
	}
	if report.Chain[0].Status != 404 {
		t.Fatalf("chain Status = %d, want 404", report.Chain[0].Status)
	}
}

func TestBuildErrorReportUsesCodeAndStatusMethods(t *testing.T) {
	report := BuildErrorReport(errorReportMethodError{message: "quota exhausted"})

	if report.Code != "rate_limited" {
		t.Fatalf("Code = %q, want rate_limited", report.Code)
	}
	if report.Status != 429 {
		t.Fatalf("Status = %d, want 429", report.Status)
	}
	if len(report.Chain) != 1 {
		t.Fatalf("Chain length = %d, want 1", len(report.Chain))
	}
	if report.Chain[0].Code != "rate_limited" {
		t.Fatalf("chain Code = %q, want rate_limited", report.Chain[0].Code)
	}
	if report.Chain[0].Status != 429 {
		t.Fatalf("chain Status = %d, want 429", report.Chain[0].Status)
	}
}

func TestBuildErrorReportFlattensJoinedErrors(t *testing.T) {
	err := fmt.Errorf("outer: %w", errors.Join(
		errors.New("first"),
		errorReportMethodError{message: "second"},
	))

	report := BuildErrorReport(err)

	if len(report.Chain) != 4 {
		t.Fatalf("Chain length = %d, want 4", len(report.Chain))
	}
	messages := make([]string, 0, len(report.Chain))
	for _, frame := range report.Chain {
		messages = append(messages, frame.Message)
	}
	if !errorReportContainsMessage(messages, "first") {
		t.Fatalf("messages = %#v, want first", messages)
	}
	if !errorReportContainsMessage(messages, "second") {
		t.Fatalf("messages = %#v, want second", messages)
	}
	if report.Code != "rate_limited" {
		t.Fatalf("Code = %q, want rate_limited", report.Code)
	}
}

func TestBuildErrorReportMarshalsAsJSON(t *testing.T) {
	report := BuildErrorReport(&errorReportFieldError{
		Code:    "bad_request",
		Status:  400,
		message: "invalid input",
	})

	encoded, err := json.Marshal(report)
	if err != nil {
		t.Fatalf("Marshal error: %v", err)
	}
	var decoded map[string]any
	if err := json.Unmarshal(encoded, &decoded); err != nil {
		t.Fatalf("Unmarshal error: %v; JSON = %s", err, encoded)
	}
	if decoded["message"] != "invalid input" {
		t.Fatalf("message = %v, want invalid input", decoded["message"])
	}
	if decoded["code"] != "bad_request" {
		t.Fatalf("code = %v, want bad_request", decoded["code"])
	}
	if decoded["status"] != float64(400) {
		t.Fatalf("status = %v, want 400", decoded["status"])
	}
	if _, ok := decoded["chain"].([]any); !ok {
		t.Fatalf("chain = %#v, want JSON array", decoded["chain"])
	}
}

func errorReportContainsMessage(messages []string, want string) bool {
	for _, message := range messages {
		if message == want {
			return true
		}
	}
	return false
}
