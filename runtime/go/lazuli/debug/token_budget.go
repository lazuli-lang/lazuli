package debug

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"unicode"
	"unicode/utf8"
)

const (
	// DefaultTokenBudget is the proposal budget for one debug bundle.
	DefaultTokenBudget = 4000

	tokenBudgetMarkerName = "token_budget_truncated"
)

// TokenBudgetConfig controls debug bundle token budget truncation.
type TokenBudgetConfig struct {
	// MaxTokens caps the estimated output token count. Values less than or equal
	// to zero use DefaultTokenBudget.
	MaxTokens int
}

// TokenBudgetSummary reports estimated token and byte accounting before and
// after budget truncation.
type TokenBudgetSummary struct {
	MaxTokens      int
	OriginalTokens int
	OutputTokens   int
	OriginalBytes  int
	OutputBytes    int
	Truncated      bool
}

// EstimateTokens returns a deterministic byte/word token estimate.
//
// This deliberately does not import a tokenizer. It uses the larger of a
// byte-floor estimate and a word/punctuation estimate, which is stable enough
// for local truncation before CI checks the canonical fixture with the pinned
// tokenizer.
func EstimateTokens(data []byte) int {
	if len(data) == 0 {
		return 0
	}

	tokens := 0
	wordBytes := 0
	otherBytes := 0
	for i := 0; i < len(data); {
		r, size := utf8.DecodeRune(data[i:])
		if r == utf8.RuneError && size == 1 {
			r = rune(data[i])
		}

		if tokenBudgetWordRune(r) {
			wordBytes += size
			i += size
			continue
		}

		tokens += tokenBudgetCeilDiv(wordBytes, 4)
		wordBytes = 0
		if !unicode.IsSpace(r) {
			otherBytes += size
		}
		i += size
	}
	tokens += tokenBudgetCeilDiv(wordBytes, 4)
	tokens += tokenBudgetCeilDiv(otherBytes, 4)

	if floor := tokenBudgetCeilDiv(len(data), 4); tokens < floor {
		return floor
	}
	return tokens
}

// TruncateToTokenBudget returns data bounded by a deterministic token estimate.
//
// Valid JSONL is truncated at record boundaries and may include a JSON marker
// line. Valid single JSON is re-encoded as valid JSON with long strings
// shortened. Other content is truncated at UTF-8 rune boundaries.
func TruncateToTokenBudget(data []byte, config *TokenBudgetConfig) ([]byte, TokenBudgetSummary) {
	maxTokens := tokenBudgetMaxTokens(config)
	originalTokens := EstimateTokens(data)
	summary := TokenBudgetSummary{
		MaxTokens:      maxTokens,
		OriginalTokens: originalTokens,
		OutputTokens:   originalTokens,
		OriginalBytes:  len(data),
		OutputBytes:    len(data),
	}
	if originalTokens <= maxTokens {
		out := append([]byte(nil), data...)
		return out, summary
	}

	out, ok := tokenBudgetTruncateJSONL(data, maxTokens)
	if !ok {
		out, ok = tokenBudgetTruncateJSON(data, maxTokens)
	}
	if !ok {
		out = tokenBudgetTruncateText(data, maxTokens)
	}

	summary.OutputTokens = EstimateTokens(out)
	summary.OutputBytes = len(out)
	summary.Truncated = true
	return out, summary
}

func tokenBudgetMaxTokens(config *TokenBudgetConfig) int {
	if config == nil || config.MaxTokens <= 0 {
		return DefaultTokenBudget
	}
	return config.MaxTokens
}

func tokenBudgetTruncateJSONL(data []byte, maxTokens int) ([]byte, bool) {
	lines, ok := tokenBudgetJSONLLines(data)
	if !ok {
		return nil, false
	}

	for keep := len(lines) - 1; keep >= 0; keep-- {
		var out bytes.Buffer
		for i := 0; i < keep; i++ {
			out.Write(lines[i])
			out.WriteByte('\n')
		}
		if omitted := len(lines) - keep; omitted > 0 {
			out.Write(tokenBudgetJSONLMarker(keep+1, omitted))
			out.WriteByte('\n')
		}
		if EstimateTokens(out.Bytes()) <= maxTokens {
			return out.Bytes(), true
		}
	}
	return []byte{}, true
}

func tokenBudgetJSONLLines(data []byte) ([][]byte, bool) {
	trimmed := bytes.TrimRight(data, "\r\n")
	if len(trimmed) == 0 || !bytes.Contains(trimmed, []byte("\n")) {
		return nil, false
	}

	parts := bytes.Split(trimmed, []byte("\n"))
	lines := make([][]byte, 0, len(parts))
	for _, part := range parts {
		line := bytes.TrimSuffix(part, []byte("\r"))
		if len(bytes.TrimSpace(line)) == 0 || !json.Valid(line) {
			return nil, false
		}
		lines = append(lines, append([]byte(nil), line...))
	}
	return lines, true
}

func tokenBudgetJSONLMarker(ordinal, omitted int) []byte {
	snippet := fmt.Sprintf("omitted %d debug bundle records to fit token budget", omitted)
	line, _, err := marshalRecord(bundleRecord{
		Type:           EntryTypeProfile,
		Name:           tokenBudgetMarkerName,
		ProfileSnippet: snippet,
		Metadata: EntryMetadata{
			Ordinal:      ordinal,
			ContentBytes: len(snippet),
			Labels: map[string]string{
				"omitted_lines": fmt.Sprint(omitted),
				"truncated":     "true",
			},
		},
	})
	if err != nil {
		return []byte(fmt.Sprintf(`{"name":"%s","omitted_lines":%d,"truncated":true,"type":"%s"}`, tokenBudgetMarkerName, omitted, EntryTypeProfile))
	}
	return line
}

func tokenBudgetTruncateJSON(data []byte, maxTokens int) ([]byte, bool) {
	value, ok := tokenBudgetDecodeJSON(data)
	if !ok {
		return nil, false
	}

	best, ok := tokenBudgetMarshalIfWithin(tokenBudgetMinimalJSONValue(value), maxTokens)
	if !ok {
		return []byte("null"), true
	}

	high := tokenBudgetMaxStringRunes(value)
	low := 0
	for low <= high {
		mid := low + (high-low)/2
		candidate, err := json.Marshal(tokenBudgetLimitStrings(value, mid))
		if err != nil {
			break
		}
		if EstimateTokens(candidate) <= maxTokens {
			best = candidate
			low = mid + 1
			continue
		}
		high = mid - 1
	}
	return best, true
}

func tokenBudgetDecodeJSON(data []byte) (any, bool) {
	decoder := json.NewDecoder(bytes.NewReader(data))
	decoder.UseNumber()

	var value any
	if err := decoder.Decode(&value); err != nil {
		return nil, false
	}
	var extra any
	if err := decoder.Decode(&extra); err == nil || err != io.EOF {
		return nil, false
	}
	return value, true
}

func tokenBudgetMarshalIfWithin(value any, maxTokens int) ([]byte, bool) {
	encoded, err := json.Marshal(value)
	if err != nil {
		return nil, false
	}
	return encoded, EstimateTokens(encoded) <= maxTokens
}

func tokenBudgetMaxStringRunes(value any) int {
	switch typed := value.(type) {
	case string:
		return utf8.RuneCountInString(typed)
	case []any:
		max := 0
		for _, item := range typed {
			if child := tokenBudgetMaxStringRunes(item); child > max {
				max = child
			}
		}
		return max
	case map[string]any:
		max := 0
		for _, child := range typed {
			if childMax := tokenBudgetMaxStringRunes(child); childMax > max {
				max = childMax
			}
		}
		return max
	default:
		return 0
	}
}

func tokenBudgetLimitStrings(value any, maxRunes int) any {
	switch typed := value.(type) {
	case string:
		return truncateString(typed, maxRunes)
	case []any:
		out := make([]any, len(typed))
		for i, item := range typed {
			out[i] = tokenBudgetLimitStrings(item, maxRunes)
		}
		return out
	case map[string]any:
		out := make(map[string]any, len(typed))
		for key, child := range typed {
			out[key] = tokenBudgetLimitStrings(child, maxRunes)
		}
		return out
	default:
		return value
	}
}

func tokenBudgetMinimalJSONValue(value any) any {
	switch value.(type) {
	case []any:
		return []any{}
	case map[string]any:
		return map[string]any{}
	case string:
		return ""
	default:
		return value
	}
}

func tokenBudgetTruncateText(data []byte, maxTokens int) []byte {
	suffix := []byte(truncationSuffix)
	low := 0
	high := len(data)
	best := []byte{}
	for low <= high {
		mid := low + (high-low)/2
		end := tokenBudgetRuneBoundary(data, mid)
		candidate := append([]byte(nil), data[:end]...)
		if end < len(data) {
			candidate = append(candidate, suffix...)
		}
		if EstimateTokens(candidate) <= maxTokens {
			best = candidate
			low = mid + 1
			continue
		}
		high = mid - 1
	}
	return best
}

func tokenBudgetRuneBoundary(data []byte, end int) int {
	if end >= len(data) {
		return len(data)
	}
	for end > 0 && !utf8.RuneStart(data[end]) {
		end--
	}
	return end
}

func tokenBudgetWordRune(r rune) bool {
	return r == '_' || unicode.IsLetter(r) || unicode.IsDigit(r)
}

func tokenBudgetCeilDiv(value, divisor int) int {
	if value <= 0 {
		return 0
	}
	return (value + divisor - 1) / divisor
}
