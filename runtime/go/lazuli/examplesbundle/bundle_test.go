package examplesbundle

import (
	"errors"
	"strings"
	"testing"
)

func TestMarshalJSONLCanonicalizesEntries(t *testing.T) {
	alphaSource := "command alpha {}\n"
	betaSource := "command beta {}\n"
	examples := []Example{
		{
			Name:       "Beta",
			Intent:     "show beta command",
			SourcePath: "examples/beta.lzi",
			Tags:       []string{"runtime", "command"},
			LZISource:  betaSource,
		},
		{
			Name:         " Alpha ",
			Intent:       " create command guarded by safety ",
			SourcePath:   "./examples/alpha.lzi",
			Tags:         []string{"safety", " command "},
			LZISource:    alphaSource,
			IRSnippet:    "command alpha ir",
			CommonErrors: []string{"validator_pii_class_mismatch", " safety_unbound "},
		},
	}

	got, err := MarshalJSONL(examples)
	if err != nil {
		t.Fatalf("MarshalJSONL() error = %v", err)
	}

	want := `{"name":"Alpha","intent":"create command guarded by safety","source_path":"examples/alpha.lzi","tags":["command","safety"],"content_hash":"` + ContentHash(alphaSource) + `","lzi_source":"command alpha {}\n","ir_snippet":"command alpha ir","common_errors":["safety_unbound","validator_pii_class_mismatch"]}` + "\n" +
		`{"name":"Beta","intent":"show beta command","source_path":"examples/beta.lzi","tags":["command","runtime"],"content_hash":"` + ContentHash(betaSource) + `","lzi_source":"command beta {}\n"}` + "\n"
	if string(got) != want {
		t.Fatalf("MarshalJSONL() = %q, want %q", got, want)
	}
}

func TestBuildCopiesAndSortsEntries(t *testing.T) {
	examples := []Example{
		examplesBundleTestExample("zeta", "examples/zeta.lzi"),
		examplesBundleTestExample("alpha", "examples/alpha.lzi"),
	}

	entries, err := Build(examples)
	if err != nil {
		t.Fatalf("Build() error = %v", err)
	}
	if got := entries[0].SourcePath; got != "examples/alpha.lzi" {
		t.Fatalf("first SourcePath = %q, want examples/alpha.lzi", got)
	}
	if got := entries[1].SourcePath; got != "examples/zeta.lzi" {
		t.Fatalf("second SourcePath = %q, want examples/zeta.lzi", got)
	}

	examples[1].Tags[0] = "changed"
	if entries[0].Tags[0] != "command" {
		t.Fatalf("entry tags changed after input mutation: %#v", entries[0].Tags)
	}
}

func TestValidateEntryChecksContentHash(t *testing.T) {
	entry, err := NewEntry(examplesBundleTestExample("alpha", "examples/alpha.lzi"))
	if err != nil {
		t.Fatalf("NewEntry() error = %v", err)
	}
	if err := ValidateEntry(entry); err != nil {
		t.Fatalf("ValidateEntry() error = %v", err)
	}

	entry.ContentHash = ContentHash("different")
	err = ValidateEntry(entry)
	if !errors.Is(err, ErrInvalidEntry) {
		t.Fatalf("ValidateEntry() error = %v, want ErrInvalidEntry", err)
	}
	if !strings.Contains(err.Error(), "content_hash") {
		t.Fatalf("ValidateEntry() error = %v, want content_hash field", err)
	}
}

func TestNewEntryValidatesRequiredFields(t *testing.T) {
	tests := []struct {
		name    string
		example Example
		field   string
	}{
		{
			name:    "missing name",
			example: examplesBundleTestExample("", "examples/alpha.lzi"),
			field:   "name",
		},
		{
			name: "missing intent",
			example: func() Example {
				example := examplesBundleTestExample("alpha", "examples/alpha.lzi")
				example.Intent = " "
				return example
			}(),
			field: "intent",
		},
		{
			name:    "missing source path",
			example: examplesBundleTestExample("alpha", ""),
			field:   "source_path",
		},
		{
			name: "missing tags",
			example: func() Example {
				example := examplesBundleTestExample("alpha", "examples/alpha.lzi")
				example.Tags = nil
				return example
			}(),
			field: "tags",
		},
		{
			name: "missing lzi source",
			example: func() Example {
				example := examplesBundleTestExample("alpha", "examples/alpha.lzi")
				example.LZISource = " "
				return example
			}(),
			field: "lzi_source",
		},
		{
			name:    "unsafe source path",
			example: examplesBundleTestExample("alpha", "../examples/alpha.lzi"),
			field:   "source_path",
		},
		{
			name: "non lzi source path",
			example: func() Example {
				example := examplesBundleTestExample("alpha", "examples/alpha.txt")
				return example
			}(),
			field: "source_path",
		},
		{
			name: "empty tag",
			example: func() Example {
				example := examplesBundleTestExample("alpha", "examples/alpha.lzi")
				example.Tags = []string{"command", " "}
				return example
			}(),
			field: "tags[1]",
		},
		{
			name: "duplicate tag",
			example: func() Example {
				example := examplesBundleTestExample("alpha", "examples/alpha.lzi")
				example.Tags = []string{"command", " command "}
				return example
			}(),
			field: "tags[1]",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			_, err := NewEntry(tt.example)
			if !errors.Is(err, ErrInvalidEntry) {
				t.Fatalf("NewEntry() error = %v, want ErrInvalidEntry", err)
			}
			if !strings.Contains(err.Error(), tt.field) {
				t.Fatalf("NewEntry() error = %v, want field %q", err, tt.field)
			}
		})
	}
}

func TestBuildRejectsDuplicateSourcePaths(t *testing.T) {
	_, err := Build([]Example{
		examplesBundleTestExample("alpha", "examples/alpha.lzi"),
		examplesBundleTestExample("alpha-copy", "./examples/alpha.lzi"),
	})
	if !errors.Is(err, ErrInvalidEntry) {
		t.Fatalf("Build() error = %v, want ErrInvalidEntry", err)
	}
	if !strings.Contains(err.Error(), "duplicate source path") {
		t.Fatalf("Build() error = %v, want duplicate source path", err)
	}
}

func TestWriteJSONLRequiresWriter(t *testing.T) {
	if err := WriteJSONL(nil, nil); !errors.Is(err, ErrWriterRequired) {
		t.Fatalf("WriteJSONL(nil) error = %v, want ErrWriterRequired", err)
	}
}

func examplesBundleTestExample(name string, sourcePath string) Example {
	return Example{
		Name:       name,
		Intent:     "show " + name + " command",
		SourcePath: sourcePath,
		Tags:       []string{"runtime", "command"},
		LZISource:  "command " + name + " {}\n",
	}
}
