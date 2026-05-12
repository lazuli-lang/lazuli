package jobs

import (
	"context"
	"errors"
	"strconv"
	"sync"
	"testing"
	"time"
)

func TestLeaderCampaignRenewSnapshotAndResign(t *testing.T) {
	t.Parallel()

	start := time.Date(2026, 5, 12, 14, 0, 0, 0, time.UTC)
	clock := newLeaderTestClock(start)
	store := newFakeLeaderStore(clock)
	ctx := context.Background()

	lease, err := Campaign(ctx, store, "jobs/scheduler", "worker-a", time.Minute)
	if err != nil {
		t.Fatalf("Campaign: %v", err)
	}
	if lease.Name != "jobs/scheduler" || lease.LeaderID != "worker-a" {
		t.Fatalf("lease identity = %+v, want jobs/scheduler worker-a", lease)
	}
	if lease.Token == "" {
		t.Fatal("Campaign returned empty token")
	}
	if !lease.ExpiresAt.Equal(start.Add(time.Minute)) {
		t.Fatalf("lease ExpiresAt = %v, want %v", lease.ExpiresAt, start.Add(time.Minute))
	}

	if _, err := Campaign(ctx, store, "jobs/scheduler", "worker-b", time.Minute); !errors.Is(err, ErrLeaderHeld) {
		t.Fatalf("second Campaign error = %v, want ErrLeaderHeld", err)
	}

	snapshot, err := Snapshot(ctx, store, "jobs/scheduler")
	if err != nil {
		t.Fatalf("Snapshot: %v", err)
	}
	if !snapshot.Held || snapshot.LeaderID != "worker-a" || !snapshot.ExpiresAt.Equal(lease.ExpiresAt) {
		t.Fatalf("snapshot = %+v, want worker-a held until %v", snapshot, lease.ExpiresAt)
	}

	clock.Advance(20 * time.Second)
	renewed, err := Renew(ctx, store, lease, time.Minute)
	if err != nil {
		t.Fatalf("Renew: %v", err)
	}
	if renewed.Token != lease.Token {
		t.Fatalf("Renew token = %q, want %q", renewed.Token, lease.Token)
	}
	if !renewed.ExpiresAt.Equal(start.Add(80 * time.Second)) {
		t.Fatalf("renewed ExpiresAt = %v, want %v", renewed.ExpiresAt, start.Add(80*time.Second))
	}

	if err := Resign(ctx, store, renewed); err != nil {
		t.Fatalf("Resign: %v", err)
	}
	snapshot, err = Snapshot(ctx, store, "jobs/scheduler")
	if err != nil {
		t.Fatalf("Snapshot after Resign: %v", err)
	}
	if snapshot.Held || snapshot.LeaderID != "" || !snapshot.ExpiresAt.IsZero() {
		t.Fatalf("snapshot after Resign = %+v, want no leader", snapshot)
	}

	next, err := Campaign(ctx, store, "jobs/scheduler", "worker-b", time.Minute)
	if err != nil {
		t.Fatalf("Campaign after Resign: %v", err)
	}
	if next.LeaderID != "worker-b" {
		t.Fatalf("next leader = %q, want worker-b", next.LeaderID)
	}
}

func TestLeaderCampaignCanReplaceExpiredLeader(t *testing.T) {
	t.Parallel()

	start := time.Date(2026, 5, 12, 15, 0, 0, 0, time.UTC)
	clock := newLeaderTestClock(start)
	store := newFakeLeaderStore(clock)
	ctx := context.Background()

	oldLease, err := Campaign(ctx, store, "jobs/sweeper", "worker-a", time.Minute)
	if err != nil {
		t.Fatalf("Campaign old leader: %v", err)
	}

	clock.Advance(time.Minute)
	replacement, err := Campaign(ctx, store, "jobs/sweeper", "worker-b", time.Minute)
	if err != nil {
		t.Fatalf("Campaign replacement after expiry: %v", err)
	}
	if replacement.LeaderID != "worker-b" {
		t.Fatalf("replacement leader = %q, want worker-b", replacement.LeaderID)
	}
	if replacement.Token == oldLease.Token {
		t.Fatal("replacement reused the expired leader token")
	}

	if _, err := Renew(ctx, store, oldLease, time.Minute); !errors.Is(err, ErrLeadershipLost) {
		t.Fatalf("expired leader Renew error = %v, want ErrLeadershipLost", err)
	}
	if err := Resign(ctx, store, oldLease); !errors.Is(err, ErrLeadershipLost) {
		t.Fatalf("expired leader Resign error = %v, want ErrLeadershipLost", err)
	}
}

func TestLeaderHelpersValidateInputsAndContext(t *testing.T) {
	t.Parallel()

	clock := newLeaderTestClock(time.Date(2026, 5, 12, 16, 0, 0, 0, time.UTC))
	store := newFakeLeaderStore(clock)
	ctx := context.Background()
	lease := LeaderLease{Name: "jobs/scheduler", LeaderID: "worker-a", Token: "token"}

	if _, err := Campaign(ctx, nil, "jobs/scheduler", "worker-a", time.Minute); !errors.Is(err, errNilLeaderStore) {
		t.Fatalf("nil store Campaign error = %v, want errNilLeaderStore", err)
	}
	if _, err := Campaign(ctx, store, "", "worker-a", time.Minute); !errors.Is(err, ErrLeaderNameRequired) {
		t.Fatalf("empty name Campaign error = %v, want ErrLeaderNameRequired", err)
	}
	if _, err := Campaign(ctx, store, "jobs/scheduler", "", time.Minute); !errors.Is(err, ErrLeaderIDRequired) {
		t.Fatalf("empty leader Campaign error = %v, want ErrLeaderIDRequired", err)
	}
	if _, err := Campaign(ctx, store, "jobs/scheduler", "worker-a", 0); !errors.Is(err, ErrLeaderTTLRequired) {
		t.Fatalf("empty ttl Campaign error = %v, want ErrLeaderTTLRequired", err)
	}
	if _, err := Renew(ctx, store, LeaderLease{Name: "jobs/scheduler", LeaderID: "worker-a"}, time.Minute); !errors.Is(err, ErrLeaderTokenRequired) {
		t.Fatalf("missing token Renew error = %v, want ErrLeaderTokenRequired", err)
	}
	if _, err := Renew(ctx, store, lease, 0); !errors.Is(err, ErrLeaderTTLRequired) {
		t.Fatalf("empty ttl Renew error = %v, want ErrLeaderTTLRequired", err)
	}
	if err := Resign(ctx, store, LeaderLease{Name: "jobs/scheduler", Token: "token"}); !errors.Is(err, ErrLeaderIDRequired) {
		t.Fatalf("missing leader Resign error = %v, want ErrLeaderIDRequired", err)
	}
	if _, err := Snapshot(ctx, store, ""); !errors.Is(err, ErrLeaderNameRequired) {
		t.Fatalf("empty name Snapshot error = %v, want ErrLeaderNameRequired", err)
	}

	canceled, cancel := context.WithCancel(context.Background())
	cancel()
	if _, err := Campaign(canceled, store, "jobs/scheduler", "worker-a", time.Minute); !errors.Is(err, context.Canceled) {
		t.Fatalf("canceled Campaign error = %v, want context.Canceled", err)
	}
}

type leaderTestClock struct {
	mu  sync.Mutex
	now time.Time
}

func newLeaderTestClock(now time.Time) *leaderTestClock {
	return &leaderTestClock{now: now}
}

func (c *leaderTestClock) Now() time.Time {
	c.mu.Lock()
	defer c.mu.Unlock()
	return c.now
}

func (c *leaderTestClock) Advance(d time.Duration) time.Time {
	c.mu.Lock()
	defer c.mu.Unlock()
	c.now = c.now.Add(d)
	return c.now
}

type fakeLeaderStore struct {
	mu     sync.Mutex
	clock  *leaderTestClock
	next   uint64
	leases map[string]fakeLeaderLease
}

type fakeLeaderLease struct {
	leaderID  string
	token     LeaderToken
	expiresAt time.Time
}

var _ LeaderStore = (*fakeLeaderStore)(nil)

func newFakeLeaderStore(clock *leaderTestClock) *fakeLeaderStore {
	return &fakeLeaderStore{clock: clock, leases: make(map[string]fakeLeaderLease)}
}

func (s *fakeLeaderStore) Acquire(
	ctx context.Context,
	name string,
	leaderID string,
	ttl time.Duration,
) (LeaderLease, error) {
	if err := fakeLeaderContextErr(ctx); err != nil {
		return LeaderLease{}, err
	}

	s.mu.Lock()
	defer s.mu.Unlock()
	s.deleteExpiredLocked(name)
	if _, held := s.leases[name]; held {
		return LeaderLease{}, ErrLeaderHeld
	}

	s.next++
	lease := LeaderLease{
		Name:      name,
		LeaderID:  leaderID,
		Token:     LeaderToken("leader-" + strconv.FormatUint(s.next, 10)),
		ExpiresAt: s.nowLocked().Add(ttl),
	}
	s.leases[name] = fakeLeaderLease{
		leaderID:  lease.LeaderID,
		token:     lease.Token,
		expiresAt: lease.ExpiresAt,
	}
	return lease, nil
}

func (s *fakeLeaderStore) Extend(ctx context.Context, lease LeaderLease, ttl time.Duration) (LeaderLease, error) {
	if err := fakeLeaderContextErr(ctx); err != nil {
		return LeaderLease{}, err
	}

	s.mu.Lock()
	defer s.mu.Unlock()
	if !s.ownsLocked(lease) {
		return LeaderLease{}, ErrLeadershipLost
	}

	lease.ExpiresAt = s.nowLocked().Add(ttl)
	s.leases[lease.Name] = fakeLeaderLease{
		leaderID:  lease.LeaderID,
		token:     lease.Token,
		expiresAt: lease.ExpiresAt,
	}
	return lease, nil
}

func (s *fakeLeaderStore) Release(ctx context.Context, lease LeaderLease) error {
	if err := fakeLeaderContextErr(ctx); err != nil {
		return err
	}

	s.mu.Lock()
	defer s.mu.Unlock()
	if !s.ownsLocked(lease) {
		return ErrLeadershipLost
	}
	delete(s.leases, lease.Name)
	return nil
}

func (s *fakeLeaderStore) Snapshot(ctx context.Context, name string) (LeadershipSnapshot, error) {
	if err := fakeLeaderContextErr(ctx); err != nil {
		return LeadershipSnapshot{}, err
	}

	s.mu.Lock()
	defer s.mu.Unlock()
	s.deleteExpiredLocked(name)
	current, held := s.leases[name]
	if !held {
		return LeadershipSnapshot{Name: name}, nil
	}
	return LeadershipSnapshot{
		Name:      name,
		LeaderID:  current.leaderID,
		ExpiresAt: current.expiresAt,
		Held:      true,
	}, nil
}

func (s *fakeLeaderStore) ownsLocked(lease LeaderLease) bool {
	s.deleteExpiredLocked(lease.Name)
	current, held := s.leases[lease.Name]
	return held && current.leaderID == lease.LeaderID && current.token == lease.Token
}

func (s *fakeLeaderStore) deleteExpiredLocked(name string) {
	current, held := s.leases[name]
	if !held {
		return
	}
	if !s.nowLocked().Before(current.expiresAt) {
		delete(s.leases, name)
	}
}

func (s *fakeLeaderStore) nowLocked() time.Time {
	if s.clock == nil {
		return time.Now()
	}
	return s.clock.Now()
}

func fakeLeaderContextErr(ctx context.Context) error {
	if ctx == nil {
		return nil
	}
	return ctx.Err()
}
