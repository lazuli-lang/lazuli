package i18n_test

import (
	"errors"
	"reflect"
	"testing"

	"lazuli.dev/runtime/lazuli/i18n"
)

func TestFormatMessageReplacesNamedPlaceholders(t *testing.T) {
	t.Parallel()

	got, err := i18n.FormatMessage("Hello, { name }. You have {count} alerts.", map[string]string{
		"name":  "Ada",
		"count": "3",
	})
	if err != nil {
		t.Fatalf("FormatMessage: %v", err)
	}
	want := "Hello, Ada. You have 3 alerts."
	if got != want {
		t.Fatalf("FormatMessage = %q, want %q", got, want)
	}
}

func TestFormatMessageEscapesBraces(t *testing.T) {
	t.Parallel()

	got, err := i18n.FormatMessage("Use {{name}} literally, then {name}; close with }}.", map[string]string{
		"name": "Ada",
	})
	if err != nil {
		t.Fatalf("FormatMessage: %v", err)
	}
	want := "Use {name} literally, then Ada; close with }."
	if got != want {
		t.Fatalf("FormatMessage = %q, want %q", got, want)
	}
}

func TestFormatMessageMissingVariablesError(t *testing.T) {
	t.Parallel()

	_, err := i18n.FormatMessage("Hello, {first} {last}. Count: {count}. Again: {last}.", map[string]string{
		"first": "Ada",
	})
	if !errors.Is(err, i18n.ErrMessageVariableMissing) {
		t.Fatalf("FormatMessage error = %v, want ErrMessageVariableMissing", err)
	}

	var missing *i18n.MissingMessageVariablesError
	if !errors.As(err, &missing) {
		t.Fatalf("FormatMessage error = %T, want MissingMessageVariablesError", err)
	}
	wantNames := []string{"last", "count"}
	if !reflect.DeepEqual(missing.Names, wantNames) {
		t.Fatalf("missing names = %#v, want %#v", missing.Names, wantNames)
	}
}

func TestFormatMessageInvalidBraceSyntax(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name   string
		source string
		offset int
	}{
		{
			name:   "unclosed",
			source: "Hello, {name",
			offset: 7,
		},
		{
			name:   "empty",
			source: "Hello, {}",
			offset: 7,
		},
		{
			name:   "closing",
			source: "Hello, }",
			offset: 7,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			_, err := i18n.FormatMessage(tt.source, nil)
			if !errors.Is(err, i18n.ErrMessageFormatInvalid) {
				t.Fatalf("FormatMessage error = %v, want ErrMessageFormatInvalid", err)
			}

			var invalid *i18n.InvalidMessageFormatError
			if !errors.As(err, &invalid) {
				t.Fatalf("FormatMessage error = %T, want InvalidMessageFormatError", err)
			}
			if invalid.Offset != tt.offset {
				t.Fatalf("invalid offset = %d, want %d", invalid.Offset, tt.offset)
			}
		})
	}
}

func TestMessageCatalogFormatMessageFallsBackAndFormats(t *testing.T) {
	t.Parallel()

	catalog := i18n.NewMessageCatalog(testContract(), map[string]map[string]string{
		"en": {
			"status": "Ready for {name}; literal {{status}}.",
		},
		"pt": {
			"welcome": "Ola, {name}.",
		},
		"pt-BR": {},
	})

	rendered, err := catalog.FormatMessage("pt-BR", "welcome", map[string]string{
		"name": "Ada",
	})
	if err != nil {
		t.Fatalf("FormatMessage pt fallback: %v", err)
	}
	if rendered != "Ola, Ada." {
		t.Fatalf("FormatMessage pt fallback = %q, want %q", rendered, "Ola, Ada.")
	}

	rendered, err = catalog.FormatMessage("pt-BR", "status", map[string]string{
		"name": "Ada",
	})
	if err != nil {
		t.Fatalf("FormatMessage default fallback: %v", err)
	}
	want := "Ready for Ada; literal {status}."
	if rendered != want {
		t.Fatalf("FormatMessage default fallback = %q, want %q", rendered, want)
	}
}

func TestMessageCatalogFormatMessageMissingVariableIncludesKey(t *testing.T) {
	t.Parallel()

	catalog := i18n.NewMessageCatalog(testContract(), map[string]map[string]string{
		"en": {
			"welcome": "Hello, {name}.",
		},
	})

	_, err := catalog.FormatMessage("en", "welcome", nil)
	if !errors.Is(err, i18n.ErrMessageVariableMissing) {
		t.Fatalf("FormatMessage error = %v, want ErrMessageVariableMissing", err)
	}

	var missing *i18n.MissingMessageVariablesError
	if !errors.As(err, &missing) {
		t.Fatalf("FormatMessage error = %T, want MissingMessageVariablesError", err)
	}
	if missing.Key != "welcome" || !reflect.DeepEqual(missing.Names, []string{"name"}) {
		t.Fatalf("missing variables = %+v", missing)
	}
}
