// Package reputation is the runtime-level IPScorer seam for
// @plugin/ip-reputation. Adapters call abuse-signal feeds
// (AbuseIPDB, Cloudflare Radar, Project Honey Pot) to annotate
// inbound requests with risk scores.
//
// The framework calls Score from the rate-limit middleware so a
// "trusted" IP (low score) gets generous bucket sizes while a
// "risky" IP (high score) trips lower thresholds.
package reputation

import (
	"context"
	"errors"
	"time"
)

// Scorer returns a risk score for an IP address. Implementations
// MUST be safe for concurrent use AND cache aggressively -- the
// scoring path is in the request hot loop.
type Scorer interface {
	Score(ctx context.Context, ip string) (Score, error)
	Close() error
}

// Score carries the vendor's verdict. Higher = riskier. Vendors
// normalize to [0.0, 1.0]. ScoredAt lets callers cache + expire
// stale scores per their own TTL.
type Score struct {
	Risk     float32  // 0.0 = trusted, 1.0 = abuse-active
	Country  string   // ISO 3166-1 alpha-2; empty if unknown
	Reasons  []string // free-form vendor tags ("tor", "vpn", "datacenter")
	ScoredAt time.Time
}

var (
	ErrScorerUnavailable = errors.New("lazuli/reputation: scorer unavailable")
	ErrRateLimited       = errors.New("lazuli/reputation: vendor rate-limited")
)

// NeutralScorer returns Risk=0.0 for everything. Default when no
// adapter binds. Pilots SHOULD bind @plugin/ip-reputation for
// production hardening.
type NeutralScorer struct{}

func (NeutralScorer) Score(ctx context.Context, ip string) (Score, error) {
	return Score{Risk: 0.0, ScoredAt: time.Now()}, nil
}
func (NeutralScorer) Close() error { return nil }
