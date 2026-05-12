package lazuli

import (
	"bytes"
	"encoding/json"
	"errors"
	"io"
	"net/http"
	"strconv"
	"strings"
)

const (
	defaultBodyPreviewMaxBytes = int64(4 << 10)
	redactedBodyValue          = "[REDACTED]"
)

var defaultBodyRedactFields = []string{"password", "token", "secret"}

// BodyRedactionConfig controls request body previews produced for logs.
type BodyRedactionConfig struct {
	// MaxBytes caps the returned preview size. Values less than or equal to
	// zero use the default 4 KiB cap.
	MaxBytes int64
	// RedactFields are JSON object field names whose values are replaced in the
	// preview. Nil uses the default fields: password, token, and secret. A
	// non-nil empty slice disables field redaction.
	RedactFields []string
}

// RedactedBodyPreview returns a bounded, log-safe preview of r.Body and then
// restores r.Body so downstream handlers can read the original body.
//
// The helper reads at most MaxBytes plus one sentinel byte from the request
// body. It redacts configured JSON object fields when the preview is valid
// JSON, and applies a conservative best-effort redaction pass to invalid or
// truncated JSON. The truncated result reports whether the original preview or
// redacted output exceeded the configured cap. Invalid JSON does not produce an
// error; read failures do.
func RedactedBodyPreview(r *http.Request, config *BodyRedactionConfig) ([]byte, bool, error) {
	if r == nil {
		return nil, false, errors.New("lazuli: nil request")
	}
	if r.Body == nil {
		return nil, false, nil
	}

	maxBytes := bodyPreviewMaxBytes(config)
	readLimit := maxBytes
	if maxBytes < 1<<63-1 {
		readLimit++
	}

	original := r.Body
	consumed, err := io.ReadAll(io.LimitReader(original, readLimit))
	r.Body = &restoredRequestBody{
		Reader: io.MultiReader(bytes.NewReader(consumed), original),
		Closer: original,
	}
	if err != nil {
		return nil, false, err
	}

	preview := consumed
	truncated := int64(len(consumed)) > maxBytes
	if truncated {
		preview = consumed[:len(consumed)-1]
	}

	preview = redactJSONBodyPreview(preview, bodyRedactFieldSet(config))
	var redactionTruncated bool
	preview, redactionTruncated = truncateBodyPreview(preview, maxBytes)
	return preview, truncated || redactionTruncated, nil
}

type restoredRequestBody struct {
	io.Reader
	io.Closer
}

func bodyPreviewMaxBytes(config *BodyRedactionConfig) int64 {
	if config == nil || config.MaxBytes <= 0 {
		return defaultBodyPreviewMaxBytes
	}
	return config.MaxBytes
}

func bodyRedactFieldSet(config *BodyRedactionConfig) map[string]struct{} {
	fields := defaultBodyRedactFields
	if config != nil && config.RedactFields != nil {
		fields = config.RedactFields
	}

	redact := make(map[string]struct{}, len(fields))
	for _, field := range fields {
		field = strings.ToLower(strings.TrimSpace(field))
		if field == "" {
			continue
		}
		redact[field] = struct{}{}
	}
	return redact
}

func redactJSONBodyPreview(preview []byte, redact map[string]struct{}) []byte {
	if len(preview) == 0 || len(redact) == 0 {
		return preview
	}

	var value any
	decoder := json.NewDecoder(bytes.NewReader(preview))
	decoder.UseNumber()
	if err := decoder.Decode(&value); err == nil {
		var extra any
		if err := decoder.Decode(&extra); err == io.EOF {
			value = redactJSONValue(value, redact)
			if redacted, err := json.Marshal(value); err == nil {
				return redacted
			}
		}
	}

	return redactInvalidJSONPreview(preview, redact)
}

func redactJSONValue(value any, redact map[string]struct{}) any {
	switch typed := value.(type) {
	case map[string]any:
		for key, child := range typed {
			if _, ok := redact[strings.ToLower(key)]; ok {
				typed[key] = redactedBodyValue
				continue
			}
			typed[key] = redactJSONValue(child, redact)
		}
	case []any:
		for i, child := range typed {
			typed[i] = redactJSONValue(child, redact)
		}
	}
	return value
}

func redactInvalidJSONPreview(preview []byte, redact map[string]struct{}) []byte {
	redactedString := []byte(strconv.Quote(redactedBodyValue))
	out := make([]byte, 0, len(preview))

	for i := 0; i < len(preview); {
		if preview[i] != '"' {
			out = append(out, preview[i])
			i++
			continue
		}

		stringEnd, ok := scanJSONString(preview, i)
		if !ok {
			out = append(out, preview[i:]...)
			break
		}

		token := preview[i:stringEnd]
		key, err := strconv.Unquote(string(token))
		if err != nil {
			out = append(out, token...)
			i = stringEnd
			continue
		}

		colon := skipJSONWhitespace(preview, stringEnd)
		if colon >= len(preview) || preview[colon] != ':' {
			out = append(out, token...)
			i = stringEnd
			continue
		}

		valueStart := skipJSONWhitespace(preview, colon+1)
		out = append(out, token...)
		out = append(out, preview[stringEnd:valueStart]...)

		if _, ok := redact[strings.ToLower(key)]; !ok {
			i = valueStart
			continue
		}

		out = append(out, redactedString...)
		i = scanJSONValueEnd(preview, valueStart)
	}

	return out
}

func scanJSONString(buf []byte, start int) (int, bool) {
	escaped := false
	for i := start + 1; i < len(buf); i++ {
		switch {
		case escaped:
			escaped = false
		case buf[i] == '\\':
			escaped = true
		case buf[i] == '"':
			return i + 1, true
		}
	}
	return len(buf), false
}

func skipJSONWhitespace(buf []byte, start int) int {
	for start < len(buf) {
		switch buf[start] {
		case ' ', '\n', '\r', '\t':
			start++
		default:
			return start
		}
	}
	return start
}

func scanJSONValueEnd(buf []byte, start int) int {
	if start >= len(buf) {
		return start
	}

	switch buf[start] {
	case '"':
		if end, ok := scanJSONString(buf, start); ok {
			return end
		}
		return len(buf)
	case '{', '[':
		return scanJSONCompositeEnd(buf, start)
	default:
		for i := start; i < len(buf); i++ {
			switch buf[i] {
			case ' ', '\n', '\r', '\t', ',', '}', ']':
				return i
			}
		}
		return len(buf)
	}
}

func scanJSONCompositeEnd(buf []byte, start int) int {
	stack := make([]byte, 0, 4)
	for i := start; i < len(buf); i++ {
		switch buf[i] {
		case '"':
			end, ok := scanJSONString(buf, i)
			if !ok {
				return len(buf)
			}
			i = end - 1
		case '{':
			stack = append(stack, '}')
		case '[':
			stack = append(stack, ']')
		case '}', ']':
			if len(stack) == 0 || buf[i] != stack[len(stack)-1] {
				return i
			}
			stack = stack[:len(stack)-1]
			if len(stack) == 0 {
				return i + 1
			}
		}
	}
	return len(buf)
}

func truncateBodyPreview(preview []byte, maxBytes int64) ([]byte, bool) {
	if int64(len(preview)) <= maxBytes {
		return preview, false
	}
	return preview[:int(maxBytes)], true
}
