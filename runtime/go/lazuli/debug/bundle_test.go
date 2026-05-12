package debug

import (
	"bytes"
	"encoding/json"
	"errors"
	"reflect"
	"strings"
	"testing"
)

func TestBuildJSONLSortsEntriesAndAccountsForSizes(t *testing.T) {
	entries := []Entry{
		{
			Type:           EntryTypeProfile,
			Name:           "hot_ops",
			Feature:        "invoice",
			ProfileSnippet: "invoice.query.list 8.1%",
			PatternID:      "query_pgx_list",
			PatternVersion: "v2",
			Labels: map[string]string{
				" source ": "pprof",
				"":         "ignored",
			},
		},
		ErrorEntry(
			"validation_failed",
			json.RawMessage(`{ "code": "validation_failed", "base": { "origin": "user_dsl", "op": "create_customer" } }`),
			"command create_customer\n",
			`{"kind":"command","op":"create_customer"}`,
		),
		{
			Type:      EntryTypeExample,
			Name:      "command_with_safety",
			Intent:    "create command guarded by a safety validator",
			LZISource: "command create_customer\n  safety check_email\n",
			IRSnippet: `{"kind":"command","op":"create_customer"}`,
			CommonErrors: []string{
				"validator_pii_class_mismatch",
				"safety_unbound",
				"safety_unbound",
				" ",
			},
			Labels: map[string]string{"pilot": "two"},
		},
		ExampleEntry(
			"auth_login",
			"password login",
			"auth password\n",
			`{"kind":"auth"}`,
		),
	}

	data, summary, err := BuildJSONL(entries)
	if err != nil {
		t.Fatalf("BuildJSONL error = %v", err)
	}
	if summary.EntryCount != 4 {
		t.Fatalf("summary.EntryCount = %d, want 4", summary.EntryCount)
	}
	if summary.TotalBytes != len(data) {
		t.Fatalf("summary.TotalBytes = %d, want %d", summary.TotalBytes, len(data))
	}

	records := debugBundleRecords(t, data)
	gotOrder := make([]string, 0, len(records))
	for _, record := range records {
		gotOrder = append(gotOrder, string(record.Type)+":"+record.Name)
	}
	wantOrder := []string{
		"example:auth_login",
		"example:command_with_safety",
		"error:validation_failed",
		"profile:hot_ops",
	}
	if !reflect.DeepEqual(gotOrder, wantOrder) {
		t.Fatalf("record order = %#v, want %#v", gotOrder, wantOrder)
	}

	commonErrors := records[1].CommonErrors
	wantCommonErrors := []string{"safety_unbound", "validator_pii_class_mismatch"}
	if !reflect.DeepEqual(commonErrors, wantCommonErrors) {
		t.Fatalf("common_errors = %#v, want %#v", commonErrors, wantCommonErrors)
	}

	normalizedEnvelope := string(records[2].ErrorEnvelope)
	const wantEnvelope = `{"base":{"op":"create_customer","origin":"user_dsl"},"code":"validation_failed"}`
	if normalizedEnvelope != wantEnvelope {
		t.Fatalf("error_envelope = %s, want %s", normalizedEnvelope, wantEnvelope)
	}

	lines := strings.Split(strings.TrimSuffix(string(data), "\n"), "\n")
	for i, record := range records {
		wantLineBytes := len(lines[i]) + 1
		if record.Metadata.Ordinal != i+1 {
			t.Fatalf("record %d ordinal = %d, want %d", i, record.Metadata.Ordinal, i+1)
		}
		if record.Metadata.LineBytes != wantLineBytes {
			t.Fatalf("record %d line_bytes = %d, want %d", i, record.Metadata.LineBytes, wantLineBytes)
		}
		if summary.Entries[i].Metadata.LineBytes != wantLineBytes {
			t.Fatalf("summary entry %d line bytes = %d, want %d", i, summary.Entries[i].Metadata.LineBytes, wantLineBytes)
		}
	}

	commandContentBytes := len(records[1].Intent) +
		len(records[1].LZISource) +
		len(records[1].IRSnippet) +
		len("safety_unbound") +
		len("validator_pii_class_mismatch")
	if records[1].Metadata.ContentBytes != commandContentBytes {
		t.Fatalf("command content bytes = %d, want %d", records[1].Metadata.ContentBytes, commandContentBytes)
	}

	profileLabels := records[3].Metadata.Labels
	if !reflect.DeepEqual(profileLabels, map[string]string{"source": "pprof"}) {
		t.Fatalf("profile labels = %#v, want source label", profileLabels)
	}

	reversed := debugBundleReverseEntries(entries)
	second, secondSummary, err := BuildJSONL(reversed)
	if err != nil {
		t.Fatalf("BuildJSONL(reversed) error = %v", err)
	}
	if string(second) != string(data) {
		t.Fatalf("BuildJSONL output changed after input reorder\nfirst:  %s\nsecond: %s", data, second)
	}
	if !reflect.DeepEqual(secondSummary, summary) {
		t.Fatalf("BuildJSONL summary changed after input reorder\nfirst:  %#v\nsecond: %#v", summary, secondSummary)
	}
}

func TestWriteJSONLMatchesBuildJSONL(t *testing.T) {
	entries := []Entry{
		ProfileEntry("cpu", "customer.command.create_customer 12.4%"),
	}

	wantData, wantSummary, err := BuildJSONL(entries)
	if err != nil {
		t.Fatalf("BuildJSONL error = %v", err)
	}

	var out bytes.Buffer
	gotSummary, err := WriteJSONL(&out, entries)
	if err != nil {
		t.Fatalf("WriteJSONL error = %v", err)
	}
	if out.String() != string(wantData) {
		t.Fatalf("WriteJSONL data = %q, want %q", out.String(), string(wantData))
	}
	if !reflect.DeepEqual(gotSummary, wantSummary) {
		t.Fatalf("WriteJSONL summary = %#v, want %#v", gotSummary, wantSummary)
	}
}

func TestBuilderAddsEntries(t *testing.T) {
	builder := NewBuilder()
	builder.Add(ProfileEntry("alloc", "command_pgx_insert v1 34 MB"))
	builder.Add(ExampleEntry("command", "create command", "command create_customer\n", "{}"))

	data, summary, err := builder.Build()
	if err != nil {
		t.Fatalf("Build error = %v", err)
	}
	if summary.EntryCount != 2 {
		t.Fatalf("summary.EntryCount = %d, want 2", summary.EntryCount)
	}

	records := debugBundleRecords(t, data)
	if records[0].Type != EntryTypeExample || records[1].Type != EntryTypeProfile {
		t.Fatalf("records sorted by type = %s then %s, want example then profile", records[0].Type, records[1].Type)
	}
}

func TestBuildJSONLValidatesEntries(t *testing.T) {
	tests := []struct {
		name    string
		entries []Entry
		wantErr error
	}{
		{
			name:    "missing type",
			entries: []Entry{{Name: "missing_type"}},
			wantErr: ErrMissingEntryType,
		},
		{
			name:    "invalid type",
			entries: []Entry{{Type: EntryType("other"), Name: "bad_type"}},
			wantErr: ErrInvalidEntryType,
		},
		{
			name:    "missing name",
			entries: []Entry{{Type: EntryTypeExample}},
			wantErr: ErrMissingEntryName,
		},
		{
			name: "invalid envelope",
			entries: []Entry{{
				Type:          EntryTypeError,
				Name:          "bad_json",
				ErrorEnvelope: json.RawMessage(`{"code":`),
			}},
			wantErr: ErrInvalidErrorEnvelope,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			_, _, err := BuildJSONL(tt.entries)
			if !errors.Is(err, tt.wantErr) {
				t.Fatalf("BuildJSONL error = %v, want %v", err, tt.wantErr)
			}
		})
	}
}

func TestWriteJSONLRejectsNilWriter(t *testing.T) {
	_, err := WriteJSONL(nil, []Entry{ProfileEntry("cpu", "top")})
	if !errors.Is(err, ErrNilWriter) {
		t.Fatalf("WriteJSONL(nil) error = %v, want ErrNilWriter", err)
	}
}

func debugBundleRecords(t *testing.T, data []byte) []bundleRecord {
	t.Helper()

	text := strings.TrimSuffix(string(data), "\n")
	if text == "" {
		return nil
	}

	lines := strings.Split(text, "\n")
	records := make([]bundleRecord, 0, len(lines))
	for i, line := range lines {
		var record bundleRecord
		if err := json.Unmarshal([]byte(line), &record); err != nil {
			t.Fatalf("unmarshal line %d: %v\n%s", i+1, err, line)
		}
		records = append(records, record)
	}
	return records
}

func debugBundleReverseEntries(entries []Entry) []Entry {
	reversed := make([]Entry, len(entries))
	for i := range entries {
		reversed[len(entries)-1-i] = entries[i]
	}
	return reversed
}
