package diagnostics_test

import (
	"errors"
	"reflect"
	"strings"
	"testing"

	"lazuli.dev/runtime/lazuli/diagnostics"
)

func TestErrorCodeRegistryLookupAndSortedDefinitions(t *testing.T) {
	t.Parallel()

	input := []diagnostics.ErrorCodeDefinition{
		{
			Code:             diagnostics.Code("LAZULI-RUNTIME-002"),
			Namespace:        diagnostics.Family("LAZULI-RUNTIME"),
			Severity:         diagnostics.SeverityWarning,
			Owner:            "runtime",
			DocumentationURL: "https://docs.lazuli.dev/errors/LAZULI-RUNTIME-002",
		},
		{
			Code:             diagnostics.Code("LAZULI-RUNTIME-001"),
			Namespace:        diagnostics.Family("LAZULI-RUNTIME"),
			Severity:         diagnostics.SeverityError,
			Owner:            "runtime",
			DocumentationURL: "/docs/errors/LAZULI-RUNTIME-001",
		},
	}

	registry, err := diagnostics.NewErrorCodeRegistry(input...)
	if err != nil {
		t.Fatalf("NewErrorCodeRegistry() error = %v", err)
	}

	if codes := registry.Codes(); !reflect.DeepEqual(codes, []diagnostics.Code{
		diagnostics.Code("LAZULI-RUNTIME-001"),
		diagnostics.Code("LAZULI-RUNTIME-002"),
	}) {
		t.Fatalf("Codes() = %v", codes)
	}

	definition, ok := registry.Lookup(diagnostics.Code("LAZULI-RUNTIME-002"))
	if !ok {
		t.Fatal("Lookup() missing registered code")
	}
	if definition.Severity != diagnostics.SeverityWarning || definition.Owner != "runtime" {
		t.Fatalf("Lookup() definition = %#v", definition)
	}

	definitions := registry.Definitions()
	definitions[0].Owner = "changed"
	got, err := registry.LookupRequired(diagnostics.Code("LAZULI-RUNTIME-001"))
	if err != nil {
		t.Fatalf("LookupRequired() error = %v", err)
	}
	if got.Owner != "runtime" {
		t.Fatalf("Definitions() returned mutable backing storage: owner = %q", got.Owner)
	}

	if _, err := registry.LookupRequired(diagnostics.Code("LAZULI-RUNTIME-999")); !errors.Is(err, diagnostics.ErrUnknownErrorCode) {
		t.Fatalf("LookupRequired(missing) error = %v, want ErrUnknownErrorCode", err)
	}
}

func TestValidateErrorCodeDefinitionsRejectsInvalidMetadata(t *testing.T) {
	t.Parallel()

	definitions := []diagnostics.ErrorCodeDefinition{
		{
			Code:             diagnostics.Code("LAZULI-RUNTIME-001"),
			Namespace:        diagnostics.Family("OTHER"),
			Severity:         diagnostics.Severity(99),
			Owner:            "",
			DocumentationURL: "ftp://docs.lazuli.dev/errors/LAZULI-RUNTIME-001",
		},
		{
			Code:             diagnostics.Code("lazuli-runtime-002"),
			Namespace:        diagnostics.Family("LAZULI-RUNTIME"),
			Severity:         diagnostics.SeverityError,
			Owner:            "runtime",
			DocumentationURL: "docs/errors/LAZULI-RUNTIME-002",
		},
	}

	err := diagnostics.ValidateErrorCodeDefinitions(definitions)
	if !errors.Is(err, diagnostics.ErrInvalidErrorCode) {
		t.Fatalf("ValidateErrorCodeDefinitions() error = %v, want ErrInvalidErrorCode", err)
	}
	for _, fragment := range []string{
		"code namespace is",
		"severity must be",
		"owner is required",
		"must use http or https",
		"namespace must contain only A-Z",
		"root-relative path",
	} {
		if !strings.Contains(err.Error(), fragment) {
			t.Fatalf("ValidateErrorCodeDefinitions() error = %q, want fragment %q", err.Error(), fragment)
		}
	}
}

func TestErrorCodeRegistryRejectsDuplicates(t *testing.T) {
	t.Parallel()

	definition := diagnostics.ErrorCodeDefinition{
		Code:             diagnostics.Code("LAZULI-RUNTIME-001"),
		Namespace:        diagnostics.Family("LAZULI-RUNTIME"),
		Severity:         diagnostics.SeverityError,
		Owner:            "runtime",
		DocumentationURL: "/docs/errors/LAZULI-RUNTIME-001",
	}

	_, err := diagnostics.NewErrorCodeRegistry(definition, definition)
	if !errors.Is(err, diagnostics.ErrDuplicateErrorCode) {
		t.Fatalf("NewErrorCodeRegistry(duplicate) error = %v, want ErrDuplicateErrorCode", err)
	}

	var registry diagnostics.ErrorCodeRegistry
	if err := registry.Register(definition); err != nil {
		t.Fatalf("Register(first) error = %v", err)
	}
	if err := registry.Register(definition); !errors.Is(err, diagnostics.ErrDuplicateErrorCode) {
		t.Fatalf("Register(duplicate) error = %v, want ErrDuplicateErrorCode", err)
	}
}

func TestSortedErrorCodeDefinitionsNormalizesWithoutMutatingInput(t *testing.T) {
	t.Parallel()

	input := []diagnostics.ErrorCodeDefinition{
		{
			Code:             diagnostics.Code(" LAZULI-RUNTIME-002 "),
			Namespace:        diagnostics.Family(" LAZULI-RUNTIME "),
			Severity:         diagnostics.SeverityInfo,
			Owner:            " runtime ",
			DocumentationURL: " https://docs.lazuli.dev/errors/LAZULI-RUNTIME-002 ",
		},
		{
			Code:             diagnostics.Code("LAZULI-RUNTIME-001"),
			Namespace:        diagnostics.Family("LAZULI-RUNTIME"),
			Severity:         diagnostics.SeverityHint,
			Owner:            "runtime",
			DocumentationURL: "/docs/errors/LAZULI-RUNTIME-001",
		},
	}

	got, err := diagnostics.SortedErrorCodeDefinitions(input)
	if err != nil {
		t.Fatalf("SortedErrorCodeDefinitions() error = %v", err)
	}
	if input[0].Owner != " runtime " {
		t.Fatal("SortedErrorCodeDefinitions() mutated input")
	}
	if codes := []diagnostics.Code{got[0].Code, got[1].Code}; !reflect.DeepEqual(codes, []diagnostics.Code{
		diagnostics.Code("LAZULI-RUNTIME-001"),
		diagnostics.Code("LAZULI-RUNTIME-002"),
	}) {
		t.Fatalf("sorted codes = %v", codes)
	}
	if got[1].Owner != "runtime" || got[1].DocumentationURL != "https://docs.lazuli.dev/errors/LAZULI-RUNTIME-002" {
		t.Fatalf("normalized definition = %#v", got[1])
	}
}

func TestValidateErrorCodeNamespace(t *testing.T) {
	t.Parallel()

	if err := diagnostics.ValidateErrorCodeNamespace(diagnostics.Code("LAZULI-RUNTIME-001"), diagnostics.Family("LAZULI-RUNTIME")); err != nil {
		t.Fatalf("ValidateErrorCodeNamespace(valid) error = %v", err)
	}
	if err := diagnostics.ValidateErrorCodeNamespace(diagnostics.Code("LAZULI-RUNTIME-01"), diagnostics.Family("LAZULI-RUNTIME")); !errors.Is(err, diagnostics.ErrInvalidErrorCode) {
		t.Fatalf("ValidateErrorCodeNamespace(short sequence) error = %v, want ErrInvalidErrorCode", err)
	}
}
