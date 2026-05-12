package snapshot

import (
	"encoding/json"
	"errors"
	"math"
	"strings"
	"testing"
	"time"
)

func TestMarshalReturnsStableIndentedJSONWithSortedMapKeys(t *testing.T) {
	got, err := String(map[string]any{
		"z": 3,
		"a": map[string]any{
			"z": 2,
			"a": 1,
		},
		"m": []any{
			map[string]any{"b": true, "a": false},
		},
	})
	if err != nil {
		t.Fatalf("String error = %v", err)
	}

	const want = "{\n" +
		"  \"a\": {\n" +
		"    \"a\": 1,\n" +
		"    \"z\": 2\n" +
		"  },\n" +
		"  \"m\": [\n" +
		"    {\n" +
		"      \"a\": false,\n" +
		"      \"b\": true\n" +
		"    }\n" +
		"  ],\n" +
		"  \"z\": 3\n" +
		"}\n"
	if got != want {
		t.Fatalf("snapshot =\n%s\nwant:\n%s", got, want)
	}
}

func TestMarshalNormalizesTimesToUTC(t *testing.T) {
	local := time.FixedZone("test", -3*60*60)
	value := map[string]any{
		"createdAt": time.Date(2026, 5, 12, 10, 30, 45, 123000000, local),
		"nested": map[string]any{
			"updatedAt": "2026-05-12T08:30:45+02:00",
		},
		"plain": "2026-05-12",
	}

	got, err := String(value, WithNormalizedTimes())
	if err != nil {
		t.Fatalf("String error = %v", err)
	}

	const want = "{\n" +
		"  \"createdAt\": \"2026-05-12T13:30:45.123Z\",\n" +
		"  \"nested\": {\n" +
		"    \"updatedAt\": \"2026-05-12T06:30:45Z\"\n" +
		"  },\n" +
		"  \"plain\": \"2026-05-12\"\n" +
		"}\n"
	if got != want {
		t.Fatalf("snapshot =\n%s\nwant:\n%s", got, want)
	}
}

func TestMarshalUsesCustomTimeNormalizer(t *testing.T) {
	got, err := String(map[string]any{
		"createdAt": time.Date(2026, 5, 12, 10, 30, 45, 0, time.UTC),
		"name":      "kept",
	}, WithTimeNormalizer(func(time.Time) any {
		return "<time>"
	}))
	if err != nil {
		t.Fatalf("String error = %v", err)
	}

	const want = "{\n" +
		"  \"createdAt\": \"<time>\",\n" +
		"  \"name\": \"kept\"\n" +
		"}\n"
	if got != want {
		t.Fatalf("snapshot =\n%s\nwant:\n%s", got, want)
	}
}

func TestMarshalRedactsConfiguredJSONFields(t *testing.T) {
	type account struct {
		ID      string         `json:"id"`
		Token   string         `json:"token"`
		Profile map[string]any `json:"profile"`
	}

	got, err := String(account{
		ID:    "acct_123",
		Token: "secret-token",
		Profile: map[string]any{
			"Secret": "nested-secret",
			"email":  "ada@example.com",
		},
	}, WithRedactedFields("TOKEN", " secret "), WithRedaction("***"))
	if err != nil {
		t.Fatalf("String error = %v", err)
	}

	const want = "{\n" +
		"  \"id\": \"acct_123\",\n" +
		"  \"profile\": {\n" +
		"    \"Secret\": \"***\",\n" +
		"    \"email\": \"ada@example.com\"\n" +
		"  },\n" +
		"  \"token\": \"***\"\n" +
		"}\n"
	if got != want {
		t.Fatalf("snapshot =\n%s\nwant:\n%s", got, want)
	}
	if strings.Contains(got, "secret-token") || strings.Contains(got, "nested-secret") {
		t.Fatalf("snapshot leaked redacted value: %s", got)
	}
}

func TestCompareReturnsEmptyDiffForMatchingSnapshot(t *testing.T) {
	const want = "{\r\n  \"id\": 1\r\n}"

	diff, err := Compare(want, map[string]int{"id": 1})
	if err != nil {
		t.Fatalf("Compare error = %v", err)
	}
	if diff != "" {
		t.Fatalf("diff = %q, want empty", diff)
	}
}

func TestCompareReturnsDiffForMismatch(t *testing.T) {
	diff, err := Compare("{\n  \"id\": 1\n}\n", map[string]int{"id": 2})
	if err != nil {
		t.Fatalf("Compare error = %v", err)
	}

	if !strings.Contains(diff, "--- want\n+++ got\n") {
		t.Fatalf("diff missing header: %q", diff)
	}
	if !strings.Contains(diff, "-   \"id\": 1\n") {
		t.Fatalf("diff missing wanted line: %q", diff)
	}
	if !strings.Contains(diff, "+   \"id\": 2\n") {
		t.Fatalf("diff missing got line: %q", diff)
	}
}

func TestMarshalReturnsJSONErrors(t *testing.T) {
	_, err := String(math.NaN())
	if err == nil {
		t.Fatal("String error = nil, want error")
	}
	var unsupported *json.UnsupportedValueError
	if !errors.As(err, &unsupported) {
		t.Fatalf("String error = %T, want unsupported value error", err)
	}
}
