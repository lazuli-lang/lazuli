package lazuli

import (
	"reflect"
	"testing"
)

// resolveFieldTarget mirrors the field shapes the Go codegen emits for input
// structs: acronym-bearing names (ID, AgencyID, HTMLBody, ...) carried by the
// acronym-aware caser in `lazuli_codegen_go::emitter::casing`, each with a
// `json` struct tag naming the snake_case wire key.
type resolveFieldTarget struct {
	ID          string `json:"id"`
	AgencyID    string `json:"agency_id"`
	HTMLBody    string `json:"html_body"`
	APIKey      string `json:"api_key"`
	JSONPayload string `json:"json_payload"`
	UUID        string `json:"uuid"`
	URL         string `json:"url"`
	TTL         int    `json:"ttl"`
	Name        string `json:"name"`
	OwnerID     string `json:"owner_id"`
}

// TestReadPathResolvesAcronymWireKeys is the regression guard for the
// runtime<->codegen casing drift: command bindings pass the snake_case wire
// key (FromInput("agency_id")) and the resolver must land on the acronym-cased
// Go field (AgencyID). Before the fix the naive caser produced "AgencyId" and
// reflection missed, returning bad_request "input field not found".
func TestReadPathResolvesAcronymWireKeys(t *testing.T) {
	in := resolveFieldTarget{
		ID:          "id-val",
		AgencyID:    "agency-val",
		HTMLBody:    "<p>hi</p>",
		APIKey:      "secret",
		JSONPayload: `{"a":1}`,
		UUID:        "uuid-val",
		URL:         "https://example.com",
		TTL:         42,
		Name:        "name-val",
		OwnerID:     "owner-val",
	}

	cases := []struct {
		wireKey string
		want    any
	}{
		{"id", "id-val"},
		{"agency_id", "agency-val"},
		{"html_body", "<p>hi</p>"},
		{"api_key", "secret"},
		{"json_payload", `{"a":1}`},
		{"uuid", "uuid-val"},
		{"url", "https://example.com"},
		{"ttl", 42},
		{"name", "name-val"},
		{"owner_id", "owner-val"},
	}
	for _, c := range cases {
		got, err := readPath(reflect.ValueOf(in), c.wireKey)
		if err != nil {
			t.Errorf("readPath(%q) returned error: %v", c.wireKey, err)
			continue
		}
		if got != c.want {
			t.Errorf("readPath(%q) = %#v, want %#v", c.wireKey, got, c.want)
		}
	}
}

// TestReadPathResolvesExactFieldName covers the query-binding face, which
// passes the Go field name verbatim (FromInput("AgencyID")). Both faces must
// resolve to the same field.
func TestReadPathResolvesExactFieldName(t *testing.T) {
	in := resolveFieldTarget{AgencyID: "agency-val", ID: "id-val"}
	for _, name := range []string{"AgencyID", "ID"} {
		got, err := readPath(reflect.ValueOf(in), name)
		if err != nil {
			t.Errorf("readPath(%q) returned error: %v", name, err)
			continue
		}
		want := "agency-val"
		if name == "ID" {
			want = "id-val"
		}
		if got != want {
			t.Errorf("readPath(%q) = %#v, want %#v", name, got, want)
		}
	}
}

// TestReadPathTagOnlyResolution proves the drift-proof leg: a field whose
// acronym caser would NOT produce the wire key still resolves via its json
// tag (single source of truth shared with the emitter).
func TestReadPathTagOnlyResolution(t *testing.T) {
	type tagged struct {
		Weird string `json:"some_wire_key"`
	}
	got, err := readPath(reflect.ValueOf(tagged{Weird: "v"}), "some_wire_key")
	if err != nil {
		t.Fatalf("readPath(some_wire_key) returned error: %v", err)
	}
	if got != "v" {
		t.Fatalf("readPath(some_wire_key) = %#v, want %q", got, "v")
	}
}

// TestReadPathUnknownKeyErrors is the negative: an unknown key must fail with
// a clear bad_request, not silently resolve.
func TestReadPathUnknownKeyErrors(t *testing.T) {
	_, err := readPath(reflect.ValueOf(resolveFieldTarget{}), "does_not_exist")
	if err == nil {
		t.Fatal("readPath(does_not_exist) = nil error, want bad_request")
	}
	le, ok := err.(*Error)
	if !ok {
		t.Fatalf("readPath error type = %T, want *Error", err)
	}
	if le.Code != CodeBadRequest {
		t.Errorf("readPath error code = %v, want %v", le.Code, CodeBadRequest)
	}
}

// TestPascalCaseMatchesEmitter pins the acronym-aware caser against the exact
// outputs the codegen emits, so the two cannot silently drift.
func TestPascalCaseMatchesEmitter(t *testing.T) {
	cases := map[string]string{
		"id":           "ID",
		"agency_id":    "AgencyID",
		"html_body":    "HTMLBody",
		"api_key":      "APIKey",
		"json_payload": "JSONPayload",
		"uuid":         "UUID",
		"url":          "URL",
		"ttl":          "TTL",
		"first_name":   "FirstName",
		"owner_id":     "OwnerID",
	}
	for in, want := range cases {
		if got := pascalCase(in); got != want {
			t.Errorf("pascalCase(%q) = %q, want %q", in, got, want)
		}
	}
}
