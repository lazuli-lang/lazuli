package openapi

import (
	"errors"
	"strings"
	"testing"
)

func TestValidateDocumentJSONAcceptsMinimalOpenAPI3(t *testing.T) {
	data := []byte(`{
		"openapi": "3.1.0",
		"info": {"title": "Pets", "version": "1.0.0"},
		"paths": {
			" pets/:id/ ": {
				"GET": {"operationId": "getPet"},
				"parameters": []
			}
		}
	}`)

	if err := ValidateDocumentJSON(data); err != nil {
		t.Fatalf("ValidateDocumentJSON() error = %v", err)
	}
}

func TestValidateDocumentRejectsMissingRequiredRootFields(t *testing.T) {
	tests := []struct {
		name string
		doc  map[string]any
		want string
	}{
		{
			name: "openapi",
			doc: map[string]any{
				"info":  map[string]any{},
				"paths": map[string]any{},
			},
			want: "openapi",
		},
		{
			name: "info",
			doc: map[string]any{
				"openapi": "3.1.0",
				"paths":   map[string]any{},
			},
			want: "info",
		},
		{
			name: "paths",
			doc: map[string]any{
				"openapi": "3.1.0",
				"info":    map[string]any{},
			},
			want: "paths",
		},
		{
			name: "openapi 2",
			doc: map[string]any{
				"openapi": "2.0",
				"info":    map[string]any{},
				"paths":   map[string]any{},
			},
			want: "OpenAPI 3",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := ValidateDocument(tt.doc)
			if !errors.Is(err, ErrInvalidDocument) {
				t.Fatalf("ValidateDocument() error = %v, want ErrInvalidDocument", err)
			}
			if !strings.Contains(err.Error(), tt.want) {
				t.Fatalf("ValidateDocument() error = %q, want %q", err, tt.want)
			}
		})
	}
}

func TestValidateDocumentRejectsDuplicateOperationIDs(t *testing.T) {
	doc := map[string]any{
		"openapi": "3.1.0",
		"info":    map[string]any{},
		"paths": map[string]any{
			"/pets": map[string]any{
				"get": map[string]any{"operationId": "find"},
			},
			"/owners": map[string]any{
				"post": map[string]any{"operationId": "find"},
			},
		},
	}

	err := ValidateDocument(doc)
	if !errors.Is(err, ErrInvalidDocument) {
		t.Fatalf("ValidateDocument() error = %v, want ErrInvalidDocument", err)
	}
	if !strings.Contains(err.Error(), "operationId") {
		t.Fatalf("ValidateDocument() error = %q, want operationId", err)
	}
}

func TestValidateDocumentRejectsNormalizedPathAndMethodCollisions(t *testing.T) {
	t.Run("path", func(t *testing.T) {
		doc := map[string]any{
			"openapi": "3.1.0",
			"info":    map[string]any{},
			"paths": map[string]any{
				"pets/:id": map[string]any{
					"get": map[string]any{},
				},
				"/pets/{id}/": map[string]any{
					"post": map[string]any{},
				},
			},
		}

		err := ValidateDocument(doc)
		if !errors.Is(err, ErrInvalidDocument) {
			t.Fatalf("ValidateDocument() error = %v, want ErrInvalidDocument", err)
		}
		if !strings.Contains(err.Error(), "normalizes") {
			t.Fatalf("ValidateDocument() error = %q, want normalized collision", err)
		}
	})

	t.Run("method", func(t *testing.T) {
		doc := map[string]any{
			"openapi": "3.1.0",
			"info":    map[string]any{},
			"paths": map[string]any{
				"/pets": map[string]any{
					"GET":   map[string]any{},
					" get ": map[string]any{},
				},
			},
		}

		err := ValidateDocument(doc)
		if !errors.Is(err, ErrInvalidDocument) {
			t.Fatalf("ValidateDocument() error = %v, want ErrInvalidDocument", err)
		}
		if !strings.Contains(err.Error(), "normalizes") {
			t.Fatalf("ValidateDocument() error = %q, want normalized collision", err)
		}
	})
}

func TestNormalizePathAndMethod(t *testing.T) {
	path, err := NormalizePath(" api//pets/:id/ ")
	if err != nil {
		t.Fatalf("NormalizePath() error = %v", err)
	}
	if path != "/api/pets/{id}" {
		t.Fatalf("NormalizePath() = %q, want /api/pets/{id}", path)
	}
	path, err = NormalizePath("/reports/{year}-{month}.json")
	if err != nil {
		t.Fatalf("NormalizePath(embedded template) error = %v", err)
	}
	if path != "/reports/{year}-{month}.json" {
		t.Fatalf("NormalizePath(embedded template) = %q", path)
	}
	path, err = NormalizePath("/accounts:search")
	if err != nil {
		t.Fatalf("NormalizePath(colon literal) error = %v", err)
	}
	if path != "/accounts:search" {
		t.Fatalf("NormalizePath(colon literal) = %q", path)
	}

	method, err := NormalizeMethod(" POST ")
	if err != nil {
		t.Fatalf("NormalizeMethod() error = %v", err)
	}
	if method != "post" {
		t.Fatalf("NormalizeMethod() = %q, want post", method)
	}

	if _, err := NormalizeMethod("CONNECT"); !errors.Is(err, ErrInvalidDocument) {
		t.Fatalf("NormalizeMethod(CONNECT) error = %v, want ErrInvalidDocument", err)
	}
}

func TestValidateDocumentJSONRejectsInvalidJSON(t *testing.T) {
	for _, data := range [][]byte{
		[]byte(``),
		[]byte(`[]`),
		[]byte(`{"openapi":"3.1.0","info":{},"paths":{}} null`),
	} {
		err := ValidateDocumentJSON(data)
		if !errors.Is(err, ErrInvalidDocument) {
			t.Fatalf("ValidateDocumentJSON(%q) error = %v, want ErrInvalidDocument", data, err)
		}
	}
}
