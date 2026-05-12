package lazuli

import (
	"errors"
	"testing"
)

func TestWithSourceStoresTagInContext(t *testing.T) {
	want := SourceTag{
		Feature: "customer",
		Kind:    "command",
		Name:    "create_customer",
		File:    "features/customer.lzi",
		Line:    42,
		Column:  8,
	}

	ctx := WithSource(t.Context(), want)
	got, ok := SourceFromContext(ctx)
	if !ok {
		t.Fatal("SourceFromContext(ctx) ok = false, want true")
	}
	if got != want {
		t.Fatalf("SourceFromContext(ctx) = %#v, want %#v", got, want)
	}
}

func TestSourceFromContextReturnsFalseWhenAbsent(t *testing.T) {
	if got, ok := SourceFromContext(t.Context()); ok || got != (SourceTag{}) {
		t.Fatalf("SourceFromContext(empty) = %#v, %v; want zero, false", got, ok)
	}
	if got, ok := SourceFromContext(nil); ok || got != (SourceTag{}) {
		t.Fatalf("SourceFromContext(nil) = %#v, %v; want zero, false", got, ok)
	}
}

func TestFormatAndParseSourceTag(t *testing.T) {
	tag := SourceTag{
		Feature: "customer",
		Kind:    "command",
		Name:    "create_customer",
		File:    "features/customer.lzi",
		Line:    42,
		Column:  8,
	}

	formatted := FormatSourceTag(tag)
	if formatted != "customer/command/create_customer@features/customer.lzi:42:8" {
		t.Fatalf("FormatSourceTag(tag) = %q", formatted)
	}

	got, err := ParseSourceTag(formatted)
	if err != nil {
		t.Fatalf("ParseSourceTag(%q) error = %v", formatted, err)
	}
	if got != tag {
		t.Fatalf("ParseSourceTag(%q) = %#v, want %#v", formatted, got, tag)
	}
}

func TestParseSourceTagRejectsMalformedInput(t *testing.T) {
	for _, input := range []string{
		"",
		"customer/command/create_customer",
		"customer/command@features/customer.lzi:42:8",
		"customer//create_customer@features/customer.lzi:42:8",
		"customer/command/create_customer@features/customer.lzi:0:8",
		"customer/command/create_customer@features/customer.lzi:42:0",
	} {
		if _, err := ParseSourceTag(input); !errors.Is(err, ErrSourceTagMalformed) {
			t.Fatalf("ParseSourceTag(%q) error = %v, want ErrSourceTagMalformed", input, err)
		}
	}
}

func TestParseSourceTagPreservesLocationError(t *testing.T) {
	_, err := ParseSourceTag("customer/command/create_customer@features/customer.lzi:bad:8")
	if !errors.Is(err, ErrSourceTagMalformed) {
		t.Fatalf("ParseSourceTag error = %v, want ErrSourceTagMalformed", err)
	}
	if !errors.Is(err, ErrSourceLocationMalformed) {
		t.Fatalf("ParseSourceTag error = %v, want ErrSourceLocationMalformed", err)
	}
}

func TestFormatAndParseSourceLocation(t *testing.T) {
	formatted := FormatSourceLocation("features/customer.lzi", 42, 8)
	if formatted != "features/customer.lzi:42:8" {
		t.Fatalf("FormatSourceLocation(...) = %q", formatted)
	}

	file, line, column, err := ParseSourceLocation(formatted)
	if err != nil {
		t.Fatalf("ParseSourceLocation(%q) error = %v", formatted, err)
	}
	if file != "features/customer.lzi" || line != 42 || column != 8 {
		t.Fatalf("ParseSourceLocation(%q) = %q, %d, %d", formatted, file, line, column)
	}
}

func TestParseSourceLocationKeepsColonsInFile(t *testing.T) {
	const input = `C:\work\features\customer.lzi:42:8`

	file, line, column, err := ParseSourceLocation(input)
	if err != nil {
		t.Fatalf("ParseSourceLocation(%q) error = %v", input, err)
	}
	if file != `C:\work\features\customer.lzi` || line != 42 || column != 8 {
		t.Fatalf("ParseSourceLocation(%q) = %q, %d, %d", input, file, line, column)
	}
}

func TestParseSourceLocationRejectsMalformedInput(t *testing.T) {
	for _, input := range []string{
		"",
		"features/customer.lzi",
		"features/customer.lzi:42",
		"features/customer.lzi:line:8",
		"features/customer.lzi:42:column",
		"features/customer.lzi:-1:8",
	} {
		if _, _, _, err := ParseSourceLocation(input); !errors.Is(err, ErrSourceLocationMalformed) {
			t.Fatalf("ParseSourceLocation(%q) error = %v, want ErrSourceLocationMalformed", input, err)
		}
	}
}
