package pattern

import (
	"errors"
	"reflect"
	"testing"
)

func TestParseAnnotation(t *testing.T) {
	tests := []struct {
		name string
		line string
		want Annotation
	}{
		{
			name: "canonical",
			line: "//lazuli:pattern command_pgx_insert v1",
			want: Annotation{ID: PatternCommandPgxInsert, Version: VersionV1},
		},
		{
			name: "trims surrounding whitespace",
			line: "  //lazuli:pattern webhook_hmac_receiver v3\t",
			want: Annotation{ID: PatternWebhookHmacReceiver, Version: VersionV3},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, err := ParseAnnotation(tt.line)
			if err != nil {
				t.Fatalf("ParseAnnotation(%q) error = %v", tt.line, err)
			}
			if got != tt.want {
				t.Fatalf("ParseAnnotation(%q) = %#v, want %#v", tt.line, got, tt.want)
			}
		})
	}
}

func TestParseAnnotationRejectsInvalidShape(t *testing.T) {
	tests := []string{
		"",
		"// lazuli:pattern command_pgx_insert v1",
		"//lazuli:pattern",
		"//lazuli:pattern command_pgx_insert",
		"//lazuli:pattern command_pgx_insert v1 extra",
		"//lazuli:pattern command_pgx_insert v1 // trailing",
	}

	for _, line := range tests {
		t.Run(line, func(t *testing.T) {
			_, err := ParseAnnotation(line)
			if !errors.Is(err, ErrInvalidAnnotation) {
				t.Fatalf("ParseAnnotation(%q) error = %v, want ErrInvalidAnnotation", line, err)
			}
		})
	}
}

func TestParseAnnotationRejectsUnknownCatalogValues(t *testing.T) {
	tests := []struct {
		name    string
		line    string
		wantErr error
	}{
		{
			name:    "unknown id",
			line:    "//lazuli:pattern custom_pgx_insert v1",
			wantErr: ErrUnknownPatternID,
		},
		{
			name:    "unknown version",
			line:    "//lazuli:pattern command_pgx_insert v4",
			wantErr: ErrUnknownPatternVersion,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			_, err := ParseAnnotation(tt.line)
			if !errors.Is(err, tt.wantErr) {
				t.Fatalf("ParseAnnotation(%q) error = %v, want %v", tt.line, err, tt.wantErr)
			}
		})
	}
}

func TestFormatAnnotation(t *testing.T) {
	got, err := FormatAnnotation(PatternQueryPgxLookup, VersionV2)
	if err != nil {
		t.Fatalf("FormatAnnotation() error = %v", err)
	}

	const want = "//lazuli:pattern query_pgx_lookup v2"
	if got != want {
		t.Fatalf("FormatAnnotation() = %q, want %q", got, want)
	}
}

func TestFormatAnnotationValidatesCatalogValues(t *testing.T) {
	if _, err := FormatAnnotation("custom", VersionV1); !errors.Is(err, ErrUnknownPatternID) {
		t.Fatalf("FormatAnnotation(custom) error = %v, want ErrUnknownPatternID", err)
	}
	if _, err := FormatAnnotation(PatternCommandPgxInsert, "v4"); !errors.Is(err, ErrUnknownPatternVersion) {
		t.Fatalf("FormatAnnotation(v4) error = %v, want ErrUnknownPatternVersion", err)
	}
}

func TestAnnotationValidateAndString(t *testing.T) {
	annotation := Annotation{ID: PatternJobRiverWorker, Version: VersionV1}
	if err := annotation.Validate(); err != nil {
		t.Fatalf("Validate() error = %v", err)
	}

	const want = "//lazuli:pattern job_river_worker v1"
	if got := annotation.String(); got != want {
		t.Fatalf("String() = %q, want %q", got, want)
	}
}

func TestCatalogHelpers(t *testing.T) {
	if !IsKnownAnnotation(PatternResourceListPgxScan, VersionV3) {
		t.Fatal("IsKnownAnnotation(resource_list_pgx_scan, v3) = false, want true")
	}
	if IsKnownAnnotation("custom", VersionV1) {
		t.Fatal("IsKnownAnnotation(custom, v1) = true, want false")
	}
	if IsKnownPatternID("") {
		t.Fatal("IsKnownPatternID(empty) = true, want false")
	}
	if IsKnownPatternVersion("") {
		t.Fatal("IsKnownPatternVersion(empty) = true, want false")
	}
}

func TestCatalogSlicesAreStableCopies(t *testing.T) {
	wantIDs := []PatternID{
		PatternCommandPgxInsert,
		PatternCommandPgxUpdate,
		PatternQueryPgxList,
		PatternQueryPgxLookup,
		PatternResourceListPgxScan,
		PatternJobRiverWorker,
		PatternWebhookHmacReceiver,
	}
	gotIDs := PatternIDs()
	if !reflect.DeepEqual(gotIDs, wantIDs) {
		t.Fatalf("PatternIDs() = %#v, want %#v", gotIDs, wantIDs)
	}
	gotIDs[0] = "mutated"
	if reflect.DeepEqual(PatternIDs(), gotIDs) {
		t.Fatal("PatternIDs() returned a mutable backing slice")
	}

	wantVersions := []PatternVersion{VersionV1, VersionV2, VersionV3}
	gotVersions := PatternVersions()
	if !reflect.DeepEqual(gotVersions, wantVersions) {
		t.Fatalf("PatternVersions() = %#v, want %#v", gotVersions, wantVersions)
	}
	gotVersions[0] = "v9"
	if reflect.DeepEqual(PatternVersions(), gotVersions) {
		t.Fatal("PatternVersions() returned a mutable backing slice")
	}
}
