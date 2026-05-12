package email

import (
	"context"
	"errors"
	"net"
	"net/http"
	"net/textproto"
	"time"
)

const (
	// DefaultDeliveryRetryMaxAttempts is the default total attempt budget,
	// including the first delivery attempt.
	DefaultDeliveryRetryMaxAttempts = 3
	// DefaultDeliveryRetryBaseDelay is the first retry delay.
	DefaultDeliveryRetryBaseDelay = 5 * time.Second
	// DefaultDeliveryRetryMaxDelay caps exponential retry backoff.
	DefaultDeliveryRetryMaxDelay = 5 * time.Minute
)

// DeliveryFailureClass describes whether a failed delivery result can be
// retried. DeliveryFailureNone represents success or no classified failure.
type DeliveryFailureClass string

const (
	DeliveryFailureNone      DeliveryFailureClass = "none"
	DeliveryFailureTransient DeliveryFailureClass = "transient"
	DeliveryFailurePermanent DeliveryFailureClass = "permanent"
)

// Retryable reports whether failures in this class should consume retry
// budget.
func (c DeliveryFailureClass) Retryable() bool {
	return c == DeliveryFailureTransient
}

// RetrySchedule configures deterministic email delivery retry backoff.
//
// MaxAttempts is the total number of attempts including the first send. Zero
// values use Lazuli's email retry defaults. Delay calculation is intentionally
// deterministic; dispatchers may add jitter at their boundary if needed.
type RetrySchedule struct {
	MaxAttempts int
	BaseDelay   time.Duration
	MaxDelay    time.Duration
}

// Normalize returns schedule with defaults and a valid delay cap applied.
func (s RetrySchedule) Normalize() RetrySchedule {
	if s.MaxAttempts < 1 {
		s.MaxAttempts = DefaultDeliveryRetryMaxAttempts
	}
	if s.BaseDelay <= 0 {
		s.BaseDelay = DefaultDeliveryRetryBaseDelay
	}
	if s.MaxDelay <= 0 {
		s.MaxDelay = DefaultDeliveryRetryMaxDelay
	}
	if s.MaxDelay < s.BaseDelay {
		s.MaxDelay = s.BaseDelay
	}
	return s
}

// Delay returns the wait before the one-based attempt number.
//
// Attempt 1 is the initial delivery and has no delay. Attempt 2 waits
// BaseDelay, then later attempts double until MaxDelay.
func (s RetrySchedule) Delay(attempt int) time.Duration {
	return s.DelayBeforeAttempt(attempt)
}

// DelayBeforeAttempt returns the wait before the one-based attempt number.
func (s RetrySchedule) DelayBeforeAttempt(attempt int) time.Duration {
	if attempt <= 1 {
		return 0
	}

	s = s.Normalize()
	delay := s.BaseDelay
	for i := 2; i < attempt; i++ {
		if delay >= s.MaxDelay/2 {
			return s.MaxDelay
		}
		delay *= 2
		if delay > s.MaxDelay {
			return s.MaxDelay
		}
	}
	return delay
}

// NextDelay returns the wait after a completed one-based attempt.
func (s RetrySchedule) NextDelay(afterAttempt int) time.Duration {
	return s.DelayBeforeAttempt(afterAttempt + 1)
}

// DeliveryAttemptEnvelope captures the classified result of one delivery
// attempt plus the retry budget decision for the next attempt.
type DeliveryAttemptEnvelope struct {
	// Attempt is one-based: the first send attempt is 1.
	Attempt int
	// MaxAttempts is the total budget, including Attempt 1.
	MaxAttempts int
	// StatusCode is an optional provider status code. HTTP status codes are
	// classified directly; SMTP status codes are classified when carried by
	// *textproto.Error in Err.
	StatusCode int
	Err        error

	FailureClass DeliveryFailureClass
	NextDelay    time.Duration
}

// NewDeliveryAttemptEnvelope classifies one delivery attempt result and fills
// the next retry delay when retry budget remains.
func NewDeliveryAttemptEnvelope(attempt, statusCode int, err error, schedule RetrySchedule) DeliveryAttemptEnvelope {
	if attempt < 1 {
		attempt = 1
	}
	schedule = schedule.Normalize()

	env := DeliveryAttemptEnvelope{
		Attempt:      attempt,
		MaxAttempts:  schedule.MaxAttempts,
		StatusCode:   statusCode,
		Err:          err,
		FailureClass: ClassifyDeliveryResult(statusCode, err),
	}
	if env.ShouldRetry() {
		env.NextDelay = schedule.NextDelay(attempt)
	}
	return env
}

// Successful reports whether the attempt has no classified delivery failure.
func (e DeliveryAttemptEnvelope) Successful() bool {
	return e.FailureClass == DeliveryFailureNone
}

// ShouldRetry reports whether a later attempt should be scheduled.
func (e DeliveryAttemptEnvelope) ShouldRetry() bool {
	return e.FailureClass.Retryable() && e.Attempt < e.MaxAttempts
}

// ClassifyDeliveryResult classifies a provider result. A failing status code
// takes precedence over a generic error because it carries provider intent.
func ClassifyDeliveryResult(statusCode int, err error) DeliveryFailureClass {
	if statusCode != 0 {
		class := ClassifyDeliveryStatus(statusCode)
		if class != DeliveryFailureNone {
			return class
		}
	}
	return ClassifyDeliveryError(err)
}

// ClassifyDeliveryStatus classifies HTTP-style dispatcher status codes.
func ClassifyDeliveryStatus(statusCode int) DeliveryFailureClass {
	switch {
	case statusCode == 0:
		return DeliveryFailureNone
	case statusCode >= http.StatusOK && statusCode < http.StatusBadRequest:
		return DeliveryFailureNone
	case statusCode == http.StatusRequestTimeout || statusCode == http.StatusTooManyRequests:
		return DeliveryFailureTransient
	case statusCode >= http.StatusInternalServerError && statusCode < 600:
		return DeliveryFailureTransient
	case statusCode >= http.StatusBadRequest && statusCode < http.StatusInternalServerError:
		return DeliveryFailurePermanent
	default:
		return DeliveryFailurePermanent
	}
}

// ClassifySMTPStatus classifies SMTP reply codes.
func ClassifySMTPStatus(statusCode int) DeliveryFailureClass {
	switch {
	case statusCode == 0:
		return DeliveryFailureNone
	case statusCode >= 200 && statusCode < 400:
		return DeliveryFailureNone
	case statusCode >= 400 && statusCode < 500:
		return DeliveryFailureTransient
	case statusCode >= 500 && statusCode < 600:
		return DeliveryFailurePermanent
	default:
		return DeliveryFailurePermanent
	}
}

// ClassifyDeliveryError classifies dispatcher errors independent of a response
// status. Unknown dispatcher errors are treated as transient so a bounded retry
// schedule can absorb short provider/network interruptions.
func ClassifyDeliveryError(err error) DeliveryFailureClass {
	if err == nil {
		return DeliveryFailureNone
	}
	if errors.Is(err, context.Canceled) {
		return DeliveryFailurePermanent
	}
	if errors.Is(err, ErrInvalidMessage) || errors.Is(err, ErrMessageSizeExceeded) {
		return DeliveryFailurePermanent
	}

	var statusErr deliveryStatusCoder
	if errors.As(err, &statusErr) {
		return ClassifyDeliveryStatus(statusErr.StatusCode())
	}

	var smtpErr *textproto.Error
	if errors.As(err, &smtpErr) {
		return ClassifySMTPStatus(smtpErr.Code)
	}

	var netErr net.Error
	if errors.As(err, &netErr) && (netErr.Timeout() || netErr.Temporary()) {
		return DeliveryFailureTransient
	}

	return DeliveryFailureTransient
}

// IsTransientDeliveryStatus reports whether statusCode should be retried.
func IsTransientDeliveryStatus(statusCode int) bool {
	return ClassifyDeliveryStatus(statusCode) == DeliveryFailureTransient
}

// IsPermanentDeliveryStatus reports whether statusCode should not be retried.
func IsPermanentDeliveryStatus(statusCode int) bool {
	return ClassifyDeliveryStatus(statusCode) == DeliveryFailurePermanent
}

// IsTransientDeliveryError reports whether err should be retried.
func IsTransientDeliveryError(err error) bool {
	return ClassifyDeliveryError(err) == DeliveryFailureTransient
}

// IsPermanentDeliveryError reports whether err should not be retried.
func IsPermanentDeliveryError(err error) bool {
	return ClassifyDeliveryError(err) == DeliveryFailurePermanent
}

type deliveryStatusCoder interface {
	StatusCode() int
}
