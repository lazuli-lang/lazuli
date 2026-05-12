package apitest

import (
	"encoding/json"
	"net/http"
	"strings"
	"testing"
)

func TestRunCasesSendsRequestAndChecksJSONSubsetAndHeaders(t *testing.T) {
	handler := http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodPost {
			t.Fatalf("method = %s, want POST", r.Method)
		}
		if got := r.URL.RequestURI(); got != "/widgets?active=true" {
			t.Fatalf("request URI = %q, want /widgets?active=true", got)
		}
		if got := r.Header.Get("X-Tenant"); got != "acme" {
			t.Fatalf("X-Tenant = %q, want acme", got)
		}
		if got := r.Header.Get("Content-Type"); got != "application/json" {
			t.Fatalf("Content-Type = %q, want application/json", got)
		}

		var body map[string]string
		if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
			t.Fatalf("request JSON decode error: %v", err)
		}
		if body["name"] != "Bolt" {
			t.Fatalf("name = %q, want Bolt", body["name"])
		}

		w.Header().Set("X-Request-ID", "req_123")
		w.Header().Add("X-Trace", "edge")
		w.Header().Add("X-Trace", "app")
		w.WriteHeader(http.StatusCreated)
		_, _ = w.Write([]byte(`{"ok":true,"widget":{"id":"w_1","name":"Bolt"},"ignored":"extra"}`))
	})

	RunCases(t, handler, []RequestCase{
		{
			Name:   "create widget",
			Method: http.MethodPost,
			Path:   "/widgets?active=true",
			Body: map[string]string{
				"name": "Bolt",
			},
			Headers: http.Header{
				"X-Tenant": {"acme"},
			},
			Expected: ExpectedResponse{
				Status: http.StatusCreated,
				Headers: http.Header{
					"X-Request-ID": {"req_123"},
					"X-Trace":      {"edge", "app"},
				},
				JSONSubset: map[string]any{
					"ok": true,
					"widget": map[string]any{
						"name": "Bolt",
					},
				},
			},
		},
	})
}

func TestRunCasesChecksJSONEquality(t *testing.T) {
	handler := http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		_, _ = w.Write([]byte(`{"items":[{"id":1,"name":"Bolt"}],"ok":true}`))
	})

	RunCases(t, handler, []RequestCase{
		{
			Name: "list widgets",
			Path: "/widgets",
			Expected: ExpectedResponse{
				JSON: json.RawMessage(`{"ok":true,"items":[{"name":"Bolt","id":1}]}`),
			},
		},
	})
}

func TestRunCasesDefaultsMethodPathAndStatus(t *testing.T) {
	handler := http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodGet {
			t.Fatalf("method = %s, want GET", r.Method)
		}
		if r.URL.Path != "/" {
			t.Fatalf("path = %q, want /", r.URL.Path)
		}
		_, _ = w.Write([]byte(`{"status":"ok"}`))
	})

	RunCases(t, handler, []RequestCase{
		{
			Expected: ExpectedResponse{
				JSON: map[string]string{"status": "ok"},
			},
		},
	})
}

func TestCompareJSONSubsetReportsNestedPath(t *testing.T) {
	want, err := normalizeExpectedJSON(map[string]any{
		"user": map[string]any{
			"email": "ada@example.test",
		},
	})
	if err != nil {
		t.Fatalf("normalize expected JSON: %v", err)
	}
	got, err := decodeJSON([]byte(`{"user":{"name":"Ada"}}`))
	if err != nil {
		t.Fatalf("decode JSON: %v", err)
	}

	err = compareJSONSubset(want, got, "$")
	if err == nil {
		t.Fatal("compareJSONSubset error = nil, want nested missing-key error")
	}
	if !strings.Contains(err.Error(), `$["user"]`) || !strings.Contains(err.Error(), `missing key "email"`) {
		t.Fatalf("compareJSONSubset error = %q, want nested key path", err)
	}
}
