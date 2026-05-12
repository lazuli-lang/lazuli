package jobs

import (
	"context"
	"errors"
	"time"
)

var (
	errNilLeaderStore = errors.New("jobs: leader store is required")

	// ErrLeaderNameRequired is returned when a leader-election helper is called
	// without an election name.
	ErrLeaderNameRequired = errors.New("jobs: leader election name required")
	// ErrLeaderIDRequired is returned when a campaign, renewal, or resignation
	// is missing the candidate/leader identity.
	ErrLeaderIDRequired = errors.New("jobs: leader id required")
	// ErrLeaderTokenRequired is returned when a renewal or resignation is
	// missing the opaque ownership token from the current lease.
	ErrLeaderTokenRequired = errors.New("jobs: leader token required")
	// ErrLeaderTTLRequired is returned when a campaign or renewal uses a
	// non-positive TTL.
	ErrLeaderTTLRequired = errors.New("jobs: leader ttl must be positive")
	// ErrLeaderHeld is returned by non-blocking campaigns when another live
	// leader already holds the named election.
	ErrLeaderHeld = errors.New("jobs: leader held")
	// ErrLeadershipLost is returned when a renewal or resignation uses a stale,
	// expired, or non-owner lease.
	ErrLeadershipLost = errors.New("jobs: leadership lost")
)

// LeaderToken is an opaque ownership token for a leader election lease.
type LeaderToken string

// LeaderLease proves that LeaderID owns Name until ExpiresAt.
type LeaderLease struct {
	// Name is the provider-neutral election name.
	Name string
	// LeaderID is the candidate identity that currently owns the lease.
	LeaderID string
	// Token is the opaque ownership token required to renew or resign.
	Token LeaderToken
	// ExpiresAt is the provider's current lease expiration time.
	ExpiresAt time.Time
}

// LeadershipSnapshot is a point-in-time view of a named election.
type LeadershipSnapshot struct {
	// Name is the provider-neutral election name.
	Name string
	// LeaderID is the active leader identity. Empty when Held is false.
	LeaderID string
	// ExpiresAt is the active lease expiration time. It is zero when Held is
	// false.
	ExpiresAt time.Time
	// Held reports whether the election currently has a live leader.
	Held bool
}

// LeaderStore is the provider-neutral lock-style contract for job leader
// election.
//
// Acquire is non-blocking: implementations return ErrLeaderHeld when another
// live owner holds the named election. Extend and Release must verify the token
// from the lease before mutating the election and return ErrLeadershipLost for
// stale, expired, or non-owner leases. Snapshot returns the current live leader
// without exposing the ownership token.
type LeaderStore interface {
	Acquire(ctx context.Context, name string, leaderID string, ttl time.Duration) (LeaderLease, error)
	Extend(ctx context.Context, lease LeaderLease, ttl time.Duration) (LeaderLease, error)
	Release(ctx context.Context, lease LeaderLease) error
	Snapshot(ctx context.Context, name string) (LeadershipSnapshot, error)
}

// Campaign attempts to become leader for name as leaderID.
//
// The attempt is non-blocking: stores return ErrLeaderHeld when another live
// leader already owns the election.
func Campaign(
	ctx context.Context,
	store LeaderStore,
	name string,
	leaderID string,
	ttl time.Duration,
) (LeaderLease, error) {
	if store == nil {
		return LeaderLease{}, errNilLeaderStore
	}
	ctx = leaderContext(ctx)
	if err := validateLeaderCampaign(ctx, name, leaderID, ttl); err != nil {
		return LeaderLease{}, err
	}
	return store.Acquire(ctx, name, leaderID, ttl)
}

// Renew extends lease for ttl when the lease still owns the election.
func Renew(ctx context.Context, store LeaderStore, lease LeaderLease, ttl time.Duration) (LeaderLease, error) {
	if store == nil {
		return LeaderLease{}, errNilLeaderStore
	}
	ctx = leaderContext(ctx)
	if err := validateLeaderLease(ctx, lease); err != nil {
		return LeaderLease{}, err
	}
	if ttl <= 0 {
		return LeaderLease{}, ErrLeaderTTLRequired
	}
	return store.Extend(ctx, lease, ttl)
}

// Resign releases lease when the lease still owns the election.
func Resign(ctx context.Context, store LeaderStore, lease LeaderLease) error {
	if store == nil {
		return errNilLeaderStore
	}
	ctx = leaderContext(ctx)
	if err := validateLeaderLease(ctx, lease); err != nil {
		return err
	}
	return store.Release(ctx, lease)
}

// Snapshot returns the current live leader for name.
func Snapshot(ctx context.Context, store LeaderStore, name string) (LeadershipSnapshot, error) {
	if store == nil {
		return LeadershipSnapshot{}, errNilLeaderStore
	}
	ctx = leaderContext(ctx)
	if err := validateLeaderName(ctx, name); err != nil {
		return LeadershipSnapshot{}, err
	}
	return store.Snapshot(ctx, name)
}

func validateLeaderCampaign(ctx context.Context, name, leaderID string, ttl time.Duration) error {
	if err := validateLeaderName(ctx, name); err != nil {
		return err
	}
	if leaderID == "" {
		return ErrLeaderIDRequired
	}
	if ttl <= 0 {
		return ErrLeaderTTLRequired
	}
	return nil
}

func validateLeaderLease(ctx context.Context, lease LeaderLease) error {
	if err := validateLeaderName(ctx, lease.Name); err != nil {
		return err
	}
	if lease.LeaderID == "" {
		return ErrLeaderIDRequired
	}
	if lease.Token == "" {
		return ErrLeaderTokenRequired
	}
	return nil
}

func validateLeaderName(ctx context.Context, name string) error {
	if err := ctx.Err(); err != nil {
		return err
	}
	if name == "" {
		return ErrLeaderNameRequired
	}
	return nil
}

func leaderContext(ctx context.Context) context.Context {
	if ctx == nil {
		return context.Background()
	}
	return ctx
}
