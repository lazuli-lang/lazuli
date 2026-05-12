package email

import (
	"context"
	"errors"
	"fmt"
	"net/http"
	"net/textproto"
	"testing"
	"time"
)

func TestClassifyDeliveryStatus(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name   string
		status int
		want   DeliveryFailureClass
	}{
		{name: "none", status: 0, want: DeliveryFailureNone},
		{name: "accepted", status: http.StatusAccepted, want: DeliveryFailureNone},
		{name: "bad request", status: http.StatusBadRequest, want: DeliveryFailurePermanent},
		{name: "unauthorized", status: http.StatusUnauthorized, want: DeliveryFailurePermanent},
		{name: "request timeout", status: http.StatusRequestTimeout, want: DeliveryFailureTransient},
		{name: "rate limit", status: http.StatusTooManyRequests, want: DeliveryFailureTransient},
		{name: "server error", status: http.StatusInternalServerError, want: DeliveryFailureTransient},
		{name: "gateway timeout", status: http.StatusGatewayTimeout, want: DeliveryFailureTransient},
	}

	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			if got := ClassifyDeliveryStatus(tt.status); got != tt.want {
				t.Fatalf("ClassifyDeliveryStatus(%d) = %q, want %q", tt.status, got, tt.want)
			}
		})
	}
}

func TestClassifyDeliveryError(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name string
		err  error
		want DeliveryFailureClass
	}{
		{name: "nil", want: DeliveryFailureNone},
		{name: "validation", err: fmt.Errorf("wrap: %w", ErrInvalidMessage), want: DeliveryFailurePermanent},
		{name: "message size", err: fmt.Errorf("wrap: %w", ErrMessageSizeExceeded), want: DeliveryFailurePermanent},
		{name: "context canceled", err: context.Canceled, want: DeliveryFailurePermanent},
		{name: "context deadline", err: context.DeadlineExceeded, want: DeliveryFailureTransient},
		{name: "http status carrier", err: statusCodeError{code: http.StatusTooManyRequests}, want: DeliveryFailureTransient},
		{name: "smtp transient", err: &textproto.Error{Code: 450, Msg: "mailbox unavailable"}, want: DeliveryFailureTransient},
		{name: "smtp permanent", err: &textproto.Error{Code: 550, Msg: "mailbox unavailable"}, want: DeliveryFailurePermanent},
		{name: "temporary network", err: temporaryNetError{}, want: DeliveryFailureTransient},
		{name: "unknown dispatcher", err: errors.New("provider unavailable"), want: DeliveryFailureTransient},
	}

	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			if got := ClassifyDeliveryError(tt.err); got != tt.want {
				t.Fatalf("ClassifyDeliveryError(%v) = %q, want %q", tt.err, got, tt.want)
			}
		})
	}
}

func TestRetryScheduleBoundsExponentialBackoff(t *testing.T) {
	t.Parallel()

	schedule := RetrySchedule{
		MaxAttempts: 5,
		BaseDelay:   time.Second,
		MaxDelay:    3 * time.Second,
	}

	tests := []struct {
		attempt int
		want    time.Duration
	}{
		{attempt: 0, want: 0},
		{attempt: 1, want: 0},
		{attempt: 2, want: time.Second},
		{attempt: 3, want: 2 * time.Second},
		{attempt: 4, want: 3 * time.Second},
		{attempt: 5, want: 3 * time.Second},
	}

	for _, tt := range tests {
		if got := schedule.DelayBeforeAttempt(tt.attempt); got != tt.want {
			t.Fatalf("DelayBeforeAttempt(%d) = %s, want %s", tt.attempt, got, tt.want)
		}
	}

	normalized := (RetrySchedule{MaxAttempts: 1, BaseDelay: 10 * time.Second, MaxDelay: time.Second}).Normalize()
	if normalized.MaxDelay != normalized.BaseDelay {
		t.Fatalf("Normalize MaxDelay = %s, want BaseDelay %s", normalized.MaxDelay, normalized.BaseDelay)
	}
}

func TestRetryScheduleDefaults(t *testing.T) {
	t.Parallel()

	schedule := (RetrySchedule{}).Normalize()
	if schedule.MaxAttempts != DefaultDeliveryRetryMaxAttempts {
		t.Fatalf("MaxAttempts = %d, want %d", schedule.MaxAttempts, DefaultDeliveryRetryMaxAttempts)
	}
	if schedule.BaseDelay != DefaultDeliveryRetryBaseDelay {
		t.Fatalf("BaseDelay = %s, want %s", schedule.BaseDelay, DefaultDeliveryRetryBaseDelay)
	}
	if schedule.MaxDelay != DefaultDeliveryRetryMaxDelay {
		t.Fatalf("MaxDelay = %s, want %s", schedule.MaxDelay, DefaultDeliveryRetryMaxDelay)
	}
}

func TestDeliveryAttemptEnvelope(t *testing.T) {
	t.Parallel()

	schedule := RetrySchedule{
		MaxAttempts: 3,
		BaseDelay:   time.Second,
		MaxDelay:    5 * time.Second,
	}

	first := NewDeliveryAttemptEnvelope(1, http.StatusServiceUnavailable, nil, schedule)
	if first.FailureClass != DeliveryFailureTransient {
		t.Fatalf("FailureClass = %q, want transient", first.FailureClass)
	}
	if !first.ShouldRetry() {
		t.Fatal("ShouldRetry = false, want true")
	}
	if first.NextDelay != time.Second {
		t.Fatalf("NextDelay = %s, want 1s", first.NextDelay)
	}

	last := NewDeliveryAttemptEnvelope(3, http.StatusServiceUnavailable, nil, schedule)
	if last.ShouldRetry() {
		t.Fatal("last ShouldRetry = true, want false")
	}
	if last.NextDelay != 0 {
		t.Fatalf("last NextDelay = %s, want 0", last.NextDelay)
	}

	permanent := NewDeliveryAttemptEnvelope(1, http.StatusBadRequest, errors.New("bad payload"), schedule)
	if permanent.FailureClass != DeliveryFailurePermanent {
		t.Fatalf("permanent FailureClass = %q, want permanent", permanent.FailureClass)
	}
	if permanent.ShouldRetry() {
		t.Fatal("permanent ShouldRetry = true, want false")
	}

	success := NewDeliveryAttemptEnvelope(1, http.StatusAccepted, nil, schedule)
	if !success.Successful() {
		t.Fatal("Successful = false, want true")
	}
	if success.ShouldRetry() {
		t.Fatal("success ShouldRetry = true, want false")
	}
}

type statusCodeError struct {
	code int
}

func (e statusCodeError) Error() string {
	return http.StatusText(e.code)
}

func (e statusCodeError) StatusCode() int {
	return e.code
}

type temporaryNetError struct{}

func (temporaryNetError) Error() string {
	return "temporary network error"
}

func (temporaryNetError) Timeout() bool {
	return false
}

func (temporaryNetError) Temporary() bool {
	return true
}
