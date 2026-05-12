package debug

import (
	"encoding/json"
	"strings"
	"testing"
	"unicode/utf8"
)

func TestEstimateTokensUsesDeterministicByteWordHeuristic(t *testing.T) {
	tests := []struct {
		name string
		data string
		want int
	}{
		{name: "empty", data: "", want: 0},
		{name: "words", data: "alpha beta", want: 3},
		{name: "json punctuation", data: `{"ok":true}`, want: 4},
		{name: "long identifier", data: "customer_profile_lookup", want: 6},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := EstimateTokens([]byte(tt.data)); got != tt.want {
				t.Fatalf("EstimateTokens(%q) = %d, want %d", tt.data, got, tt.want)
			}
		})
	}
}

func TestTruncateToTokenBudgetReturnsCopyWhenWithinBudget(t *testing.T) {
	data := []byte(`{"message":"short"}`)

	out, summary := TruncateToTokenBudget(data, &TokenBudgetConfig{
		MaxTokens: EstimateTokens(data),
	})

	if string(out) != string(data) {
		t.Fatalf("output = %s, want original", out)
	}
	if len(out) > 0 {
		out[0] = '['
	}
	if string(data) != `{"message":"short"}` {
		t.Fatalf("input mutated to %s", data)
	}
	if summary.Truncated {
		t.Fatalf("summary.Truncated = true, want false")
	}
	if summary.OutputTokens != summary.OriginalTokens {
		t.Fatalf("tokens = %d after %d, want unchanged", summary.OutputTokens, summary.OriginalTokens)
	}
}

func TestTruncateToTokenBudgetPreservesJSONLRecords(t *testing.T) {
	first := `{"name":"kept","value":"short"}`
	second := `{"name":"omitted","value":"` + strings.Repeat("word ", 80) + `"}`
	data := []byte(first + "\n" + second + "\n")
	marker := tokenBudgetJSONLMarker(2, 1)
	maxTokens := EstimateTokens([]byte(first + "\n" + string(marker) + "\n"))

	out, summary := TruncateToTokenBudget(data, &TokenBudgetConfig{
		MaxTokens: maxTokens,
	})

	if !summary.Truncated {
		t.Fatalf("summary.Truncated = false, want true")
	}
	if summary.OutputTokens > maxTokens {
		t.Fatalf("summary.OutputTokens = %d, want <= %d\n%s", summary.OutputTokens, maxTokens, out)
	}

	lines := strings.Split(strings.TrimSuffix(string(out), "\n"), "\n")
	if len(lines) != 2 {
		t.Fatalf("line count = %d, want 2\n%s", len(lines), out)
	}
	if lines[0] != first {
		t.Fatalf("first line = %s, want %s", lines[0], first)
	}
	for i, line := range lines {
		if !json.Valid([]byte(line)) {
			t.Fatalf("line %d is not valid JSON: %s", i+1, line)
		}
	}

	var markerRecord map[string]any
	if err := json.Unmarshal([]byte(lines[1]), &markerRecord); err != nil {
		t.Fatalf("unmarshal marker: %v", err)
	}
	if markerRecord["type"] != string(EntryTypeProfile) {
		t.Fatalf("marker type = %v, want profile", markerRecord["type"])
	}
	if markerRecord["name"] != tokenBudgetMarkerName {
		t.Fatalf("marker name = %v, want %s", markerRecord["name"], tokenBudgetMarkerName)
	}
	metadata, ok := markerRecord["metadata"].(map[string]any)
	if !ok {
		t.Fatalf("marker metadata = %#v, want object", markerRecord["metadata"])
	}
	labels, ok := metadata["labels"].(map[string]any)
	if !ok {
		t.Fatalf("marker labels = %#v, want object", metadata["labels"])
	}
	if labels["truncated"] != "true" {
		t.Fatalf("marker truncated label = %v, want true", labels["truncated"])
	}
	if labels["omitted_lines"] != "1" {
		t.Fatalf("marker omitted_lines label = %v, want 1", labels["omitted_lines"])
	}
}

func TestTruncateToTokenBudgetPreservesSingleJSONValidity(t *testing.T) {
	input := []byte(`{"message":"` + strings.Repeat("alpha ", 40) + `","items":["` + strings.Repeat("beta ", 20) + `"]}`)

	out, summary := TruncateToTokenBudget(input, &TokenBudgetConfig{
		MaxTokens: 35,
	})

	if !summary.Truncated {
		t.Fatalf("summary.Truncated = false, want true")
	}
	if summary.OutputTokens > summary.MaxTokens {
		t.Fatalf("summary.OutputTokens = %d, want <= %d\n%s", summary.OutputTokens, summary.MaxTokens, out)
	}
	if !json.Valid(out) {
		t.Fatalf("output is not valid JSON: %s", out)
	}

	var decoded map[string]any
	if err := json.Unmarshal(out, &decoded); err != nil {
		t.Fatalf("unmarshal output: %v", err)
	}
	message, ok := decoded["message"].(string)
	if !ok {
		t.Fatalf("message = %#v, want string", decoded["message"])
	}
	if len(message) >= len(strings.Repeat("alpha ", 40)) {
		t.Fatalf("message length = %d, want shorter than original", len(message))
	}
}

func TestTruncateToTokenBudgetUsesRuneSafeTextFallback(t *testing.T) {
	input := []byte(strings.Repeat("alpha ", 50) + "cafe")

	out, summary := TruncateToTokenBudget(input, &TokenBudgetConfig{
		MaxTokens: 12,
	})

	if !summary.Truncated {
		t.Fatalf("summary.Truncated = false, want true")
	}
	if summary.OutputTokens > summary.MaxTokens {
		t.Fatalf("summary.OutputTokens = %d, want <= %d", summary.OutputTokens, summary.MaxTokens)
	}
	if !utf8.Valid(out) {
		t.Fatalf("output is not valid UTF-8: %q", out)
	}
	if !strings.HasSuffix(string(out), truncationSuffix) {
		t.Fatalf("output = %q, want truncation suffix", out)
	}
}
