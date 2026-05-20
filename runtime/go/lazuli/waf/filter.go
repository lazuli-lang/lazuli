// Package waf is the runtime-level Filter seam for @plugin/waf.
// Adapters wrap Cloudflare WAF / AWS WAF / Imperva / ModSecurity.
//
// Filter is invoked from the HTTP middleware chain BEFORE the
// recover middleware (so a WAF rejection doesn't traverse the
// runtime). Decision is fail-closed for sandbox-rule violations,
// fail-open for rate-limit-style violations (those bubble to the
// runtime rate limiter).
package waf

import (
	"context"
	"errors"
	"net/http"
)

// Filter inspects an HTTP request and decides whether to block it
// at the WAF layer. Implementations MUST be safe for concurrent use.
type Filter interface {
	// Inspect returns Decision == Allow (proceed) or Deny (block).
	// Reason is for the audit / observability layer; never sent to
	// the client (avoid attacker probing).
	Inspect(ctx context.Context, r *http.Request) (Decision, error)

	// Close releases any background workers / open connections.
	Close() error
}

type Decision int

const (
	Allow Decision = iota
	Deny
	// Tarpit - slow-roll the response (anti-scrape). Adapter may
	// implement; default falls back to Deny.
	Tarpit
)

// Reason carries the WAF's verdict for the audit layer.
type Reason struct {
	RuleID   string
	Category string // "sql_injection" | "xss" | "rate_anomaly" | etc.
	Score    int
}

var (
	ErrFilterUnavailable = errors.New("lazuli/waf: filter unavailable")
)

// NoopFilter is the default. ALL requests Allow. Pilots SHOULD
// bind @plugin/waf for prod.
type NoopFilter struct{}

func (NoopFilter) Inspect(ctx context.Context, r *http.Request) (Decision, error) {
	return Allow, nil
}
func (NoopFilter) Close() error { return nil }
