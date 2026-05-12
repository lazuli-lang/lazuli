package debug

import (
	"encoding/json"
	"testing"
)

func TestRedactRedactsDefaultKeysDeepCopy(t *testing.T) {
	envelope := map[string]any{
		"apiKey": "api-secret",
		"context": map[string]any{
			"client-secret": "client-secret-value",
			"token_count":   12,
		},
		"events": []any{
			map[string]any{
				"refresh_token": "refresh-secret",
				"public":        "visible",
			},
		},
	}

	got := Redact(envelope, nil).(map[string]any)

	if got["apiKey"] != RedactedValue {
		t.Fatalf("apiKey = %v, want redacted", got["apiKey"])
	}
	context := got["context"].(map[string]any)
	if context["client-secret"] != RedactedValue {
		t.Fatalf("client-secret = %v, want redacted", context["client-secret"])
	}
	if context["token_count"] != 12 {
		t.Fatalf("token_count = %v, want 12", context["token_count"])
	}
	events := got["events"].([]any)
	event := events[0].(map[string]any)
	if event["refresh_token"] != RedactedValue {
		t.Fatalf("refresh_token = %v, want redacted", event["refresh_token"])
	}
	if event["public"] != "visible" {
		t.Fatalf("public = %v, want visible", event["public"])
	}

	if envelope["apiKey"] != "api-secret" {
		t.Fatalf("input apiKey mutated = %v", envelope["apiKey"])
	}
	originalContext := envelope["context"].(map[string]any)
	if originalContext["client-secret"] != "client-secret-value" {
		t.Fatalf("input client-secret mutated = %v", originalContext["client-secret"])
	}
}

func TestRedactUsesConfiguredKeysAndMask(t *testing.T) {
	envelope := map[string]any{
		"password": "kept",
		"trace_id": "hidden",
	}

	got := Redact(envelope, &RedactionConfig{
		RedactKeys: []string{"trace_id"},
		Mask:       "***",
	}).(map[string]any)

	if got["trace_id"] != "***" {
		t.Fatalf("trace_id = %v, want custom mask", got["trace_id"])
	}
	if got["password"] != "kept" {
		t.Fatalf("password = %v, want unredacted when defaults are replaced", got["password"])
	}

	disabled := Redact(envelope, &RedactionConfig{
		RedactKeys: []string{},
	}).(map[string]any)
	if disabled["password"] != "kept" {
		t.Fatalf("password = %v, want unredacted with empty key set", disabled["password"])
	}
}

func TestRedactTruncatesStringsAndSlices(t *testing.T) {
	envelope := map[string]any{
		"message": "abcdefghijklmnopqrstuvwxyz",
		"items":   []any{"one", "two", "three", "four"},
		"typed":   []string{"red", "green", "blue", "yellow"},
	}

	got := Redact(envelope, &RedactionConfig{
		MaxStringLen: 8,
		MaxSliceLen:  3,
		RedactKeys:   []string{},
	}).(map[string]any)

	if got["message"] != "abcde..." {
		t.Fatalf("message = %q, want truncated string", got["message"])
	}
	items := got["items"].([]any)
	redactTestAssertTruncatedSlice(t, items, "one", "two", 2)

	typed := got["typed"].([]any)
	redactTestAssertTruncatedSlice(t, typed, "red", "green", 2)
}

func TestRedactRawMessageAndStructJSONShape(t *testing.T) {
	raw := json.RawMessage(`{"password":"secret","count":2}`)
	gotRaw := Redact(raw, nil).(map[string]any)

	if gotRaw["password"] != RedactedValue {
		t.Fatalf("raw password = %v, want redacted", gotRaw["password"])
	}
	if gotRaw["count"] != json.Number("2") {
		t.Fatalf("raw count = %v, want 2", gotRaw["count"])
	}

	envelope := redactTestEnvelope{
		Source:   "features/customer.lzi:42:1",
		Password: "secret",
		Count:    3,
	}
	gotStruct := Redact(envelope, nil).(map[string]any)
	if gotStruct["password"] != RedactedValue {
		t.Fatalf("struct password = %v, want redacted", gotStruct["password"])
	}
	if gotStruct["source"] != envelope.Source {
		t.Fatalf("struct source = %v, want %s", gotStruct["source"], envelope.Source)
	}
	if gotStruct["count"] != json.Number("3") {
		t.Fatalf("struct count = %v, want 3", gotStruct["count"])
	}
}

func TestMarshalRedactedJSONStableMap(t *testing.T) {
	envelope := map[string]any{
		"z": 1,
		"a": map[string]any{
			"password": "secret",
			"message":  "ok",
		},
	}

	first, err := MarshalRedactedJSON(envelope, nil)
	if err != nil {
		t.Fatalf("MarshalRedactedJSON first: %v", err)
	}
	second, err := MarshalRedactedJSON(envelope, nil)
	if err != nil {
		t.Fatalf("MarshalRedactedJSON second: %v", err)
	}

	const want = `{"a":{"message":"ok","password":"[REDACTED]"},"z":1}`
	if string(first) != want {
		t.Fatalf("first JSON = %s, want %s", first, want)
	}
	if string(second) != want {
		t.Fatalf("second JSON = %s, want %s", second, want)
	}
}

type redactTestEnvelope struct {
	Source   string `json:"source"`
	Password string `json:"password"`
	Count    int    `json:"count"`
}

func redactTestAssertTruncatedSlice(t *testing.T, got []any, first, second string, omitted int) {
	t.Helper()

	if len(got) != 3 {
		t.Fatalf("slice length = %d, want 3", len(got))
	}
	if got[0] != first || got[1] != second {
		t.Fatalf("slice prefix = %#v, want %q, %q", got[:2], first, second)
	}
	marker := got[2].(map[string]any)
	if marker[truncatedSliceKey] != true {
		t.Fatalf("truncated marker = %v, want true", marker[truncatedSliceKey])
	}
	if marker[omittedItemsKey] != omitted {
		t.Fatalf("omitted = %v, want %d", marker[omittedItemsKey], omitted)
	}
}
