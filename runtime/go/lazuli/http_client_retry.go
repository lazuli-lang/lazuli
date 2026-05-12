package lazuli

import (
	"context"
	"fmt"
	"io"
	"net/http"
	"strconv"
	"strings"
	"time"
)

const (
	defaultHTTPRetryMaxAttempts = 3
	defaultHTTPRetryBaseDelay   = 100 * time.Millisecond
	defaultHTTPRetryMaxDelay    = 2 * time.Second
	maxHTTPRetryDelay           = time.Duration(1<<63 - 1)
)

// HTTPRetryPolicy configures outbound adapter HTTP retries.
//
// A zero-value policy retries up to three total attempts with exponential
// backoff. Retry-After is honored for retryable responses and capped by
// MaxDelay.
type HTTPRetryPolicy struct {
	// MaxAttempts is the total number of tries, including the first request.
	// Values less than one use the default. Set to one to disable retries.
	MaxAttempts int

	// BaseDelay is the first retry delay. Values less than or equal to zero use
	// the default.
	BaseDelay time.Duration

	// MaxDelay caps computed backoff and Retry-After delays. Values less than or
	// equal to zero use the default.
	MaxDelay time.Duration

	// Sleep waits between attempts. Nil uses a context-aware timer. Tests may
	// replace it to avoid real sleeps.
	Sleep func(context.Context, time.Duration) error
}

// HTTPRetryTransport retries transient outbound adapter calls.
//
// It retries only idempotent HTTP methods, transport errors, and selected
// retryable response statuses: 429, 500, 502, 503, and 504. Requests with a body
// are retried only when GetBody can rewind the payload.
type HTTPRetryTransport struct {
	Base   http.RoundTripper
	Policy HTTPRetryPolicy
}

var _ http.RoundTripper = (*HTTPRetryTransport)(nil)

// HTTPClientWithRetry returns a shallow copy of client with retry behavior on
// its Transport. A nil client is treated like http.DefaultClient.
func HTTPClientWithRetry(client *http.Client, policy HTTPRetryPolicy) *http.Client {
	var out http.Client
	if client != nil {
		out = *client
	}

	base := out.Transport
	if base == nil {
		base = http.DefaultTransport
	}
	out.Transport = &HTTPRetryTransport{Base: base, Policy: policy}
	return &out
}

// RoundTrip implements http.RoundTripper.
func (t *HTTPRetryTransport) RoundTrip(req *http.Request) (*http.Response, error) {
	if req == nil {
		return nil, fmt.Errorf("lazuli: nil http retry request")
	}
	if err := req.Context().Err(); err != nil {
		return nil, err
	}

	base := t.Base
	if base == nil {
		base = http.DefaultTransport
	}
	policy := normalizeHTTPRetryPolicy(t.Policy)
	if !httpRetryMethod(req.Method) {
		return base.RoundTrip(req)
	}
	bodyReplayable := httpRetryBodyReplayable(req)

	for attempt := 1; ; attempt++ {
		if attempt > 1 {
			if err := rewindHTTPRetryBody(req); err != nil {
				return nil, err
			}
		}

		resp, err := base.RoundTrip(req)
		if !httpRetryableResult(resp, err) || attempt >= policy.MaxAttempts || !bodyReplayable {
			return resp, err
		}

		if ctxErr := req.Context().Err(); ctxErr != nil {
			closeHTTPRetryResponse(resp)
			return nil, ctxErr
		}

		delay := httpRetryDelay(policy, attempt, resp)
		closeHTTPRetryResponse(resp)
		if err := policy.Sleep(req.Context(), delay); err != nil {
			return nil, err
		}
		if err := req.Context().Err(); err != nil {
			return nil, err
		}
	}
}

func normalizeHTTPRetryPolicy(policy HTTPRetryPolicy) HTTPRetryPolicy {
	if policy.MaxAttempts < 1 {
		policy.MaxAttempts = defaultHTTPRetryMaxAttempts
	}
	if policy.BaseDelay <= 0 {
		policy.BaseDelay = defaultHTTPRetryBaseDelay
	}
	if policy.MaxDelay <= 0 {
		policy.MaxDelay = defaultHTTPRetryMaxDelay
	}
	if policy.MaxDelay < policy.BaseDelay {
		policy.MaxDelay = policy.BaseDelay
	}
	if policy.Sleep == nil {
		policy.Sleep = sleepHTTPRetry
	}
	return policy
}

func httpRetryMethod(method string) bool {
	switch strings.ToUpper(method) {
	case http.MethodGet, http.MethodHead, http.MethodPut, http.MethodDelete, http.MethodOptions, http.MethodTrace:
		return true
	default:
		return false
	}
}

func httpRetryableResult(resp *http.Response, err error) bool {
	if err != nil {
		return true
	}
	if resp == nil {
		return false
	}
	switch resp.StatusCode {
	case http.StatusTooManyRequests,
		http.StatusInternalServerError,
		http.StatusBadGateway,
		http.StatusServiceUnavailable,
		http.StatusGatewayTimeout:
		return true
	default:
		return false
	}
}

func httpRetryBodyReplayable(req *http.Request) bool {
	return req.Body == nil || req.Body == http.NoBody || req.GetBody != nil
}

func rewindHTTPRetryBody(req *http.Request) error {
	if req.Body == nil || req.Body == http.NoBody {
		return nil
	}
	body, err := req.GetBody()
	if err != nil {
		return fmt.Errorf("lazuli: rewind http retry body: %w", err)
	}
	req.Body = body
	return nil
}

func httpRetryDelay(policy HTTPRetryPolicy, attempt int, resp *http.Response) time.Duration {
	if retryAfter, ok := httpRetryAfterDelay(resp); ok {
		return capHTTPRetryDelay(retryAfter, policy.MaxDelay)
	}

	delay := policy.BaseDelay
	for i := 1; i < attempt; i++ {
		if delay >= policy.MaxDelay/2 {
			return policy.MaxDelay
		}
		delay *= 2
	}
	return capHTTPRetryDelay(delay, policy.MaxDelay)
}

func httpRetryAfterDelay(resp *http.Response) (time.Duration, bool) {
	if resp == nil {
		return 0, false
	}
	raw := strings.TrimSpace(resp.Header.Get("Retry-After"))
	if raw == "" {
		return 0, false
	}
	if seconds, err := strconv.ParseInt(raw, 10, 64); err == nil {
		if seconds < 0 {
			return 0, false
		}
		if seconds > int64(maxHTTPRetryDelay/time.Second) {
			return maxHTTPRetryDelay, true
		}
		return time.Duration(seconds) * time.Second, true
	}
	when, err := http.ParseTime(raw)
	if err != nil {
		return 0, false
	}
	delay := time.Until(when)
	if delay < 0 {
		delay = 0
	}
	return delay, true
}

func capHTTPRetryDelay(delay, max time.Duration) time.Duration {
	if delay > max {
		return max
	}
	return delay
}

func sleepHTTPRetry(ctx context.Context, delay time.Duration) error {
	if delay <= 0 {
		return ctx.Err()
	}

	timer := time.NewTimer(delay)
	defer timer.Stop()

	select {
	case <-ctx.Done():
		return ctx.Err()
	case <-timer.C:
		return nil
	}
}

func closeHTTPRetryResponse(resp *http.Response) {
	if resp == nil || resp.Body == nil {
		return
	}
	_, _ = io.Copy(io.Discard, io.LimitReader(resp.Body, 512<<10))
	_ = resp.Body.Close()
}
