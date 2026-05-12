package i18n_test

import (
	"errors"
	"reflect"
	"testing"
	"testing/fstest"

	"lazuli.dev/runtime/lazuli/i18n"
)

func TestMessageCatalogLookupInterpolatesAndFallsBack(t *testing.T) {
	t.Parallel()

	messages := map[string]map[string]string{
		"en": {
			"welcome": "Hello, {{ name }}.",
			"status":  "Ready",
		},
		"pt": {
			"welcome": "Ola, {{name}}.",
		},
		"pt-BR": {},
	}
	catalog := i18n.NewMessageCatalog(testContract(), messages)
	messages["pt"]["welcome"] = "changed"

	rendered, err := catalog.Lookup("pt-BR", "welcome", map[string]string{
		"name": "Ada",
	})
	if err != nil {
		t.Fatalf("Lookup: %v", err)
	}
	if rendered != "Ola, Ada." {
		t.Fatalf("Lookup = %q, want %q", rendered, "Ola, Ada.")
	}

	rendered, err = catalog.Lookup("pt-BR", "status", nil)
	if err != nil {
		t.Fatalf("Lookup default fallback: %v", err)
	}
	if rendered != "Ready" {
		t.Fatalf("Lookup default fallback = %q, want Ready", rendered)
	}
}

func TestMessageCatalogMissingKeyErrorIncludesFallbacks(t *testing.T) {
	t.Parallel()

	catalog := i18n.NewMessageCatalog(testContract(), map[string]map[string]string{
		"en":    {},
		"pt":    {},
		"pt-BR": {},
	})

	_, err := catalog.Lookup("pt-BR", "missing", nil)
	if !errors.Is(err, i18n.ErrMessageNotFound) {
		t.Fatalf("Lookup error = %v, want ErrMessageNotFound", err)
	}

	var missing *i18n.MissingMessageError
	if !errors.As(err, &missing) {
		t.Fatalf("Lookup error = %T, want MissingMessageError", err)
	}
	wantSearched := []string{"pt-BR", "pt", "en"}
	if !reflect.DeepEqual(missing.Searched, wantSearched) {
		t.Fatalf("searched locales = %#v, want %#v", missing.Searched, wantSearched)
	}
}

func TestMessageCatalogMissingVariableError(t *testing.T) {
	t.Parallel()

	catalog := i18n.NewMessageCatalog(testContract(), map[string]map[string]string{
		"en": {
			"welcome": "Hello, {{name}}.",
		},
	})

	_, err := catalog.Lookup("en", "welcome", nil)
	if !errors.Is(err, i18n.ErrMessageVariableMissing) {
		t.Fatalf("Lookup error = %v, want ErrMessageVariableMissing", err)
	}

	var missing *i18n.MissingMessageVariableError
	if !errors.As(err, &missing) {
		t.Fatalf("Lookup error = %T, want MissingMessageVariableError", err)
	}
	if missing.Key != "welcome" || missing.Name != "name" {
		t.Fatalf("missing variable = %+v", missing)
	}
}

func TestLoadMessageCatalogFSReadsJSONFiles(t *testing.T) {
	t.Parallel()

	fsys := fstest.MapFS{
		"messages/en.json": {
			Data: []byte(`{"welcome":"Hello, {{name}}.","status":"Ready"}`),
		},
		"messages/pt.json": {
			Data: []byte(`{"welcome":"Ola, {{name}}."}`),
		},
		"messages/ignored.txt": {
			Data: []byte(`ignored`),
		},
	}

	catalog, err := i18n.LoadMessageCatalogFS(fsys, "messages", testContract())
	if err != nil {
		t.Fatalf("LoadMessageCatalogFS: %v", err)
	}

	rendered, err := catalog.Lookup("pt-BR", "welcome", map[string]string{
		"name": "Ada",
	})
	if err != nil {
		t.Fatalf("Lookup: %v", err)
	}
	if rendered != "Ola, Ada." {
		t.Fatalf("Lookup = %q, want %q", rendered, "Ola, Ada.")
	}
}

func TestLoadMessageCatalogFSRejectsInvalidJSON(t *testing.T) {
	t.Parallel()

	fsys := fstest.MapFS{
		"messages/en.json": {
			Data: []byte(`{"welcome": 42}`),
		},
	}

	_, err := i18n.LoadMessageCatalogFS(fsys, "messages", testContract())
	if err == nil {
		t.Fatal("expected invalid JSON error")
	}
}

func testContract() i18n.LocaleContract {
	return i18n.LocaleContract{
		Default:   "en",
		Supported: []string{"en", "pt", "pt-BR"},
		Fallbacks: []i18n.Fallback{
			{From: "pt-BR", To: "pt"},
			{From: "pt", To: "en"},
		},
	}
}
