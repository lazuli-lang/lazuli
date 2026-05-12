package lazuli

import (
	"errors"
	"fmt"
	"io"
	"net/http"
	"strings"
	"time"
)

// ErrSSEInvalidRetry is returned when an SSE retry delay cannot be encoded as
// the protocol's non-negative integer millisecond value.
var ErrSSEInvalidRetry = errors.New("lazuli: invalid sse retry")

// SSEEvent is a single Server-Sent Events frame.
//
// Event sets the optional event type, ID sets the optional event id, and Data
// is split into one data line per newline. Retry is the optional client
// reconnect delay; a zero value omits the retry field.
type SSEEvent struct {
	Event string
	Data  string
	ID    string
	Retry time.Duration
}

// WriteSSE writes one Server-Sent Events frame to w, sets stream-friendly
// response headers, and flushes the response when the writer supports it.
func WriteSSE(w http.ResponseWriter, event SSEEvent) error {
	if err := validateSSELine("event", event.Event); err != nil {
		return err
	}
	if err := validateSSELine("id", event.ID); err != nil {
		return err
	}
	if err := validateSSERetry(event.Retry); err != nil {
		return err
	}

	var frame strings.Builder
	writeSSELine(&frame, "event", event.Event)
	writeSSELine(&frame, "id", event.ID)
	if event.Retry > 0 {
		fmt.Fprintf(&frame, "retry: %d\n", event.Retry.Milliseconds())
	}
	writeSSEData(&frame, event.Data)
	frame.WriteByte('\n')

	header := w.Header()
	header.Set("Content-Type", "text/event-stream")
	header.Set("Cache-Control", "no-cache")
	header.Set("Connection", "keep-alive")
	header.Set("X-Accel-Buffering", "no")

	payload := frame.String()
	n, err := io.WriteString(w, payload)
	if err != nil {
		return fmt.Errorf("lazuli: write sse event: %w", err)
	}
	if n != len(payload) {
		return fmt.Errorf("lazuli: write sse event: %w", io.ErrShortWrite)
	}

	if err := http.NewResponseController(w).Flush(); err != nil && !errors.Is(err, http.ErrNotSupported) {
		return fmt.Errorf("lazuli: flush sse event: %w", err)
	}
	return nil
}

func validateSSELine(field, value string) error {
	if strings.ContainsAny(value, "\r\n") {
		return fmt.Errorf("lazuli: invalid sse %s field: must be single-line", field)
	}
	return nil
}

func validateSSERetry(retry time.Duration) error {
	if retry < 0 {
		return fmt.Errorf("%w: must be non-negative, got %s", ErrSSEInvalidRetry, retry)
	}
	if retry > 0 && retry%time.Millisecond != 0 {
		return fmt.Errorf("%w: must be a whole millisecond duration, got %s", ErrSSEInvalidRetry, retry)
	}
	return nil
}

func writeSSELine(frame *strings.Builder, field, value string) {
	if value == "" {
		return
	}
	fmt.Fprintf(frame, "%s: %s\n", field, value)
}

func writeSSEData(frame *strings.Builder, data string) {
	if data == "" {
		return
	}
	for _, line := range sseDataLines(data) {
		fmt.Fprintf(frame, "data: %s\n", line)
	}
}

func sseDataLines(data string) []string {
	normalized := strings.ReplaceAll(data, "\r\n", "\n")
	normalized = strings.ReplaceAll(normalized, "\r", "\n")
	return strings.Split(normalized, "\n")
}
