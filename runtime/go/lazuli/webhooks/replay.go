package webhooks

import (
	"errors"
	"fmt"
	"net/http"
	"strconv"
	"strings"
	"time"
)

var (
	// ErrWebhookReplayDenied means the webhook contract does not allow replayed deliveries.
	ErrWebhookReplayDenied = errors.New("webhooks: replay denied for this contract")

	// ErrWebhookReplayWindowExpired means the delivered timestamp is older than the replay window.
	ErrWebhookReplayWindowExpired = errors.New("webhooks: replay window expired")

	// ErrWebhookReplayTimestampInvalid means the webhook timestamp was missing, malformed, or from the future.
	ErrWebhookReplayTimestampInvalid = errors.New("webhooks: replay timestamp invalid")

	// ErrWebhookReplayWindowInvalid means the replay window literal could not be parsed.
	ErrWebhookReplayWindowInvalid = errors.New("webhooks: replay window invalid")

	// ErrWebhookReplayModeInvalid means the replay mode is outside the closed allow/deny catalog.
	ErrWebhookReplayModeInvalid = errors.New("webhooks: replay mode invalid")
)

var errWebhookTimestampUnsupported = errors.New("unsupported timestamp format")

// ReplayError carries structured context for replay guard failures.
//
// Use errors.Is with ErrWebhookReplayDenied, ErrWebhookReplayWindowExpired,
// ErrWebhookReplayTimestampInvalid, ErrWebhookReplayWindowInvalid, or
// ErrWebhookReplayModeInvalid to classify the failure.
type ReplayError struct {
	Kind        error
	Now         time.Time
	DeliveredAt time.Time
	Window      time.Duration
	Value       string
	Err         error
}

// Error returns a stable, human-readable replay guard failure message.
func (e *ReplayError) Error() string {
	if e == nil {
		return "<nil>"
	}

	message := "webhooks: replay check failed"
	if e.Kind != nil {
		message = e.Kind.Error()
	}
	if e.Value != "" {
		message = fmt.Sprintf("%s: %q", message, e.Value)
	}
	if !e.DeliveredAt.IsZero() {
		message = fmt.Sprintf("%s: delivered_at=%s", message, e.DeliveredAt.UTC().Format(time.RFC3339Nano))
	}
	if e.Window > 0 {
		message = fmt.Sprintf("%s window=%s", message, e.Window)
	}
	if e.Err != nil {
		message = fmt.Sprintf("%s: %v", message, e.Err)
	}
	return message
}

// Is reports whether target matches this replay error's classified kind.
func (e *ReplayError) Is(target error) bool {
	return e != nil && e.Kind != nil && target == e.Kind
}

// Unwrap exposes the lower-level parsing error, when one exists.
func (e *ReplayError) Unwrap() error {
	if e == nil {
		return nil
	}
	return e.Err
}

// CheckReplay validates a delivery timestamp against the lowered replay spec.
//
// A nil spec means the webhook contract declared no replay guard. ReplayDeny
// always returns ErrWebhookReplayDenied. ReplayAllow requires a positive
// duration literal in spec.Window and rejects missing, future, or stale
// deliveredAt timestamps.
func CheckReplay(now time.Time, spec *ReplaySpec, deliveredAt time.Time) error {
	if spec == nil {
		return nil
	}

	switch spec.Mode {
	case ReplayDeny:
		return replayError(ErrWebhookReplayDenied, now, deliveredAt, 0, "", nil)
	case ReplayAllow:
	default:
		return replayError(ErrWebhookReplayModeInvalid, now, deliveredAt, 0, "", nil)
	}

	window, err := parseReplayWindow(spec.Window)
	if err != nil {
		return replayError(ErrWebhookReplayWindowInvalid, now, deliveredAt, 0, spec.Window, err)
	}
	if deliveredAt.IsZero() {
		return replayError(ErrWebhookReplayTimestampInvalid, now, deliveredAt, window, "", nil)
	}
	if deliveredAt.After(now) {
		return replayError(ErrWebhookReplayTimestampInvalid, now, deliveredAt, window, "", nil)
	}
	if now.Sub(deliveredAt) > window {
		return replayError(ErrWebhookReplayWindowExpired, now, deliveredAt, window, "", nil)
	}
	return nil
}

// ParseWebhookTimestamp parses common webhook timestamp header values.
//
// It accepts RFC3339/RFC3339Nano timestamps, Unix seconds, Unix milliseconds,
// and HTTP-date values. Parsed timestamps are returned in UTC.
func ParseWebhookTimestamp(value string) (time.Time, error) {
	trimmed := strings.TrimSpace(value)
	if trimmed == "" {
		return time.Time{}, replayError(ErrWebhookReplayTimestampInvalid, time.Time{}, time.Time{}, 0, value, nil)
	}

	if timestamp, err := time.Parse(time.RFC3339Nano, trimmed); err == nil {
		return timestamp.UTC(), nil
	}
	if timestamp, ok, err := parseUnixWebhookTimestamp(trimmed); ok {
		if err != nil {
			return time.Time{}, replayError(ErrWebhookReplayTimestampInvalid, time.Time{}, time.Time{}, 0, value, err)
		}
		return timestamp.UTC(), nil
	}
	if timestamp, err := http.ParseTime(trimmed); err == nil {
		return timestamp.UTC(), nil
	}

	return time.Time{}, replayError(ErrWebhookReplayTimestampInvalid, time.Time{}, time.Time{}, 0, value, errWebhookTimestampUnsupported)
}

func replayError(kind error, now, deliveredAt time.Time, window time.Duration, value string, err error) *ReplayError {
	return &ReplayError{
		Kind:        kind,
		Now:         now,
		DeliveredAt: deliveredAt,
		Window:      window,
		Value:       value,
		Err:         err,
	}
}

func parseReplayWindow(raw string) (time.Duration, error) {
	window, err := time.ParseDuration(strings.TrimSpace(raw))
	if err != nil {
		return 0, err
	}
	if window <= 0 {
		return 0, errors.New("duration must be positive")
	}
	return window, nil
}

func parseUnixWebhookTimestamp(value string) (time.Time, bool, error) {
	if !isWebhookTimestampInteger(value) {
		return time.Time{}, false, nil
	}

	unix, err := strconv.ParseInt(value, 10, 64)
	if err != nil {
		return time.Time{}, true, err
	}
	const millisecondsThreshold int64 = 1_000_000_000_000
	if unix >= millisecondsThreshold || unix <= -millisecondsThreshold {
		return time.UnixMilli(unix), true, nil
	}
	return time.Unix(unix, 0), true, nil
}

func isWebhookTimestampInteger(value string) bool {
	if value == "" {
		return false
	}
	for i, c := range value {
		if i == 0 && (c == '+' || c == '-') {
			continue
		}
		if c < '0' || c > '9' {
			return false
		}
	}
	return value != "+" && value != "-"
}
