package cache

import (
	"encoding/hex"
	"errors"
	"testing"
	"time"
)

func TestBuildLockKey(t *testing.T) {
	key, err := BuildLockKey(LockKeyParts{CacheKey: " customer.query.list|-|abc "})
	if err != nil {
		t.Fatalf("BuildLockKey(default) error = %v", err)
	}
	if want := "lazuli:cache:lock:customer.query.list|-|abc"; key != want {
		t.Fatalf("BuildLockKey(default) = %q, want %q", key, want)
	}

	key, err = BuildLockKey(LockKeyParts{
		CacheKey: "customer.query.list|-|abc",
		Prefix:   "locks:",
	})
	if err != nil {
		t.Fatalf("BuildLockKey(custom) error = %v", err)
	}
	if want := "locks:customer.query.list|-|abc"; key != want {
		t.Fatalf("BuildLockKey(custom) = %q, want %q", key, want)
	}

	if _, err := BuildLockKey(LockKeyParts{}); !errors.Is(err, ErrInvalidLockKey) {
		t.Fatalf("BuildLockKey(empty) error = %v, want ErrInvalidLockKey", err)
	}
	if _, err := BuildLockKey(LockKeyParts{CacheKey: "query\nkey"}); !errors.Is(err, ErrInvalidLockKey) {
		t.Fatalf("BuildLockKey(control) error = %v, want ErrInvalidLockKey", err)
	}
}

func TestLockOwnerTokenParsingAndMinting(t *testing.T) {
	token, err := NewLockOwnerToken()
	if err != nil {
		t.Fatalf("NewLockOwnerToken() error = %v", err)
	}
	if len(token.String()) != 32 {
		t.Fatalf("NewLockOwnerToken() length = %d, want 32", len(token.String()))
	}
	if _, err := hex.DecodeString(token.String()); err != nil {
		t.Fatalf("NewLockOwnerToken() is not hex: %v", err)
	}
	if !token.Valid() {
		t.Fatal("NewLockOwnerToken() returned invalid token")
	}

	parsed, err := ParseLockOwnerToken(" owner-1 ")
	if err != nil {
		t.Fatalf("ParseLockOwnerToken(valid) error = %v", err)
	}
	if parsed != LockOwnerToken("owner-1") {
		t.Fatalf("ParseLockOwnerToken(valid) = %q, want owner-1", parsed)
	}

	for _, value := range []string{"", " \t ", "owner\n1"} {
		if _, err := ParseLockOwnerToken(value); !errors.Is(err, ErrInvalidLockOwnerToken) {
			t.Fatalf("ParseLockOwnerToken(%q) error = %v, want ErrInvalidLockOwnerToken", value, err)
		}
	}
}

func TestResolveLockTTLAppliesBounds(t *testing.T) {
	policy := LockPolicy{
		DefaultTTL: 10 * time.Second,
		MinTTL:     5 * time.Second,
		MaxTTL:     time.Minute,
	}

	tests := []struct {
		name      string
		requested time.Duration
		want      time.Duration
	}{
		{name: "default", requested: 0, want: 10 * time.Second},
		{name: "minimum", requested: time.Second, want: 5 * time.Second},
		{name: "within bounds", requested: 20 * time.Second, want: 20 * time.Second},
		{name: "maximum", requested: 2 * time.Minute, want: time.Minute},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, err := policy.ResolveTTL(tt.requested)
			if err != nil {
				t.Fatalf("ResolveTTL() error = %v", err)
			}
			if got != tt.want {
				t.Fatalf("ResolveTTL() = %v, want %v", got, tt.want)
			}
		})
	}

	if _, err := policy.ResolveTTL(-time.Second); !errors.Is(err, ErrInvalidLockPlan) {
		t.Fatalf("ResolveTTL(negative) error = %v, want ErrInvalidLockPlan", err)
	}
	invalid := LockPolicy{
		DefaultTTL: 10 * time.Second,
		MinTTL:     time.Minute,
		MaxTTL:     5 * time.Second,
	}
	if _, err := invalid.ResolveTTL(0); !errors.Is(err, ErrInvalidLockPolicy) {
		t.Fatalf("ResolveTTL(invalid policy) error = %v, want ErrInvalidLockPolicy", err)
	}
}

func TestLockMetadataStaleDetection(t *testing.T) {
	now := time.Date(2026, 5, 12, 18, 30, 0, 0, time.UTC)
	policy := LockPolicy{StaleAfter: time.Minute}
	live := LockMetadata{
		OwnerToken: LockOwnerToken("owner-1"),
		AcquiredAt: now.Add(-30 * time.Second),
		ExpiresAt:  now.Add(30 * time.Second),
	}
	if live.Stale(policy, now) {
		t.Fatal("live metadata is stale, want false")
	}

	expired := live
	expired.ExpiresAt = now
	if !expired.Stale(policy, now) {
		t.Fatal("expired metadata is not stale")
	}

	old := live
	old.AcquiredAt = now.Add(-2 * time.Minute)
	old.ExpiresAt = now.Add(30 * time.Second)
	if !old.Stale(policy, now) {
		t.Fatal("old metadata is not stale")
	}

	malformed := live
	malformed.OwnerToken = ""
	if !malformed.Stale(policy, now) {
		t.Fatal("metadata with empty owner token is not stale")
	}
}

func TestPlanLockAcquireActions(t *testing.T) {
	now := time.Date(2026, 5, 12, 18, 30, 0, 0, time.UTC)
	policy := LockPolicy{
		KeyPrefix:  "locks:",
		DefaultTTL: 10 * time.Second,
		MinTTL:     time.Second,
		MaxTTL:     time.Minute,
	}
	key, err := policy.BuildKey("customer.query.list|-|abc")
	if err != nil {
		t.Fatalf("BuildKey() error = %v", err)
	}

	plan, err := PlanLockAcquire(policy, LockAcquireRequest{
		CacheKey:   "customer.query.list|-|abc",
		OwnerToken: LockOwnerToken("owner-1"),
		Now:        now,
	})
	if err != nil {
		t.Fatalf("PlanLockAcquire(new) error = %v", err)
	}
	if !plan.CanAcquire || !plan.WriteRequired || plan.Action != LockAcquireSet {
		t.Fatalf("PlanLockAcquire(new) action = %s can=%v write=%v", plan.Action, plan.CanAcquire, plan.WriteRequired)
	}
	if plan.Key != key || plan.Metadata.Key != key || plan.ExpiresAt != now.Add(10*time.Second) {
		t.Fatalf("PlanLockAcquire(new) metadata = %#v", plan)
	}

	current := LockMetadata{
		Key:        key,
		OwnerToken: LockOwnerToken("other-owner"),
		AcquiredAt: now.Add(-time.Second),
		ExpiresAt:  now.Add(time.Minute),
	}
	plan, err = PlanLockAcquire(policy, LockAcquireRequest{
		CacheKey:       "customer.query.list|-|abc",
		OwnerToken:     LockOwnerToken("owner-1"),
		Now:            now,
		Current:        current,
		CurrentPresent: true,
	})
	if err != nil {
		t.Fatalf("PlanLockAcquire(held) error = %v", err)
	}
	if plan.CanAcquire || plan.WriteRequired || plan.Action != LockAcquireWait {
		t.Fatalf("PlanLockAcquire(held) action = %s can=%v write=%v", plan.Action, plan.CanAcquire, plan.WriteRequired)
	}

	current.OwnerToken = LockOwnerToken("owner-1")
	plan, err = PlanLockAcquire(policy, LockAcquireRequest{
		CacheKey:       "customer.query.list|-|abc",
		OwnerToken:     LockOwnerToken(" owner-1 "),
		Now:            now,
		Current:        current,
		CurrentPresent: true,
	})
	if err != nil {
		t.Fatalf("PlanLockAcquire(owned) error = %v", err)
	}
	if !plan.CanAcquire || plan.WriteRequired || plan.Action != LockAcquireAlreadyOwned {
		t.Fatalf("PlanLockAcquire(owned) action = %s can=%v write=%v", plan.Action, plan.CanAcquire, plan.WriteRequired)
	}
	if plan.Metadata != current {
		t.Fatalf("PlanLockAcquire(owned) metadata = %#v, want current %#v", plan.Metadata, current)
	}

	current.OwnerToken = LockOwnerToken("other-owner")
	current.ExpiresAt = now.Add(-time.Second)
	plan, err = PlanLockAcquire(policy, LockAcquireRequest{
		CacheKey:       "customer.query.list|-|abc",
		OwnerToken:     LockOwnerToken("owner-1"),
		TTL:            2 * time.Minute,
		Now:            now,
		Current:        current,
		CurrentPresent: true,
	})
	if err != nil {
		t.Fatalf("PlanLockAcquire(stale) error = %v", err)
	}
	if !plan.CanAcquire || !plan.WriteRequired || !plan.ReplaceStale || plan.Action != LockAcquireReplaceStale {
		t.Fatalf("PlanLockAcquire(stale) action = %s can=%v write=%v replace=%v", plan.Action, plan.CanAcquire, plan.WriteRequired, plan.ReplaceStale)
	}
	if plan.TTL != time.Minute || plan.ExpiresAt != now.Add(time.Minute) {
		t.Fatalf("PlanLockAcquire(stale) ttl/expires = %v/%v", plan.TTL, plan.ExpiresAt)
	}
}

func TestPlanLockAcquireRejectsInvalidInputs(t *testing.T) {
	policy := LockPolicy{KeyPrefix: "locks:"}
	if _, err := PlanLockAcquire(policy, LockAcquireRequest{
		CacheKey: "customer.query.list|-|abc",
	}); !errors.Is(err, ErrInvalidLockOwnerToken) {
		t.Fatalf("PlanLockAcquire(empty owner) error = %v, want ErrInvalidLockOwnerToken", err)
	}

	current := LockMetadata{
		Key:        "locks:other",
		OwnerToken: LockOwnerToken("owner-1"),
		AcquiredAt: time.Now(),
		ExpiresAt:  time.Now().Add(time.Minute),
	}
	if _, err := PlanLockAcquire(policy, LockAcquireRequest{
		CacheKey:       "customer.query.list|-|abc",
		OwnerToken:     LockOwnerToken("owner-1"),
		Current:        current,
		CurrentPresent: true,
	}); !errors.Is(err, ErrInvalidLockPlan) {
		t.Fatalf("PlanLockAcquire(mismatched key) error = %v, want ErrInvalidLockPlan", err)
	}
}

func TestPlanLockReleaseActions(t *testing.T) {
	now := time.Date(2026, 5, 12, 18, 30, 0, 0, time.UTC)
	policy := LockPolicy{KeyPrefix: "locks:"}
	key, err := policy.BuildKey("customer.query.list|-|abc")
	if err != nil {
		t.Fatalf("BuildKey() error = %v", err)
	}
	current := LockMetadata{
		Key:        key,
		OwnerToken: LockOwnerToken("owner-1"),
		AcquiredAt: now.Add(-time.Second),
		ExpiresAt:  now.Add(time.Minute),
	}

	plan, err := PlanLockRelease(policy, LockReleaseRequest{
		CacheKey:       "customer.query.list|-|abc",
		OwnerToken:     LockOwnerToken("owner-1"),
		Now:            now,
		Current:        current,
		CurrentPresent: true,
	})
	if err != nil {
		t.Fatalf("PlanLockRelease(owned) error = %v", err)
	}
	if !plan.CanRelease || !plan.Delete || plan.Stale || plan.Action != LockReleaseDelete {
		t.Fatalf("PlanLockRelease(owned) action = %s release=%v delete=%v stale=%v", plan.Action, plan.CanRelease, plan.Delete, plan.Stale)
	}

	plan, err = PlanLockRelease(policy, LockReleaseRequest{
		CacheKey:   "customer.query.list|-|abc",
		OwnerToken: LockOwnerToken("owner-1"),
		Now:        now,
	})
	if err != nil {
		t.Fatalf("PlanLockRelease(missing) error = %v", err)
	}
	if plan.CanRelease || plan.Delete || plan.Action != LockReleaseSkipMissing {
		t.Fatalf("PlanLockRelease(missing) action = %s release=%v delete=%v", plan.Action, plan.CanRelease, plan.Delete)
	}

	plan, err = PlanLockRelease(policy, LockReleaseRequest{
		CacheKey:       "customer.query.list|-|abc",
		OwnerToken:     LockOwnerToken("other-owner"),
		Now:            now,
		Current:        current,
		CurrentPresent: true,
	})
	if err != nil {
		t.Fatalf("PlanLockRelease(wrong owner) error = %v", err)
	}
	if plan.CanRelease || plan.Delete || plan.Action != LockReleaseRejectOwner {
		t.Fatalf("PlanLockRelease(wrong owner) action = %s release=%v delete=%v", plan.Action, plan.CanRelease, plan.Delete)
	}

	current.ExpiresAt = now.Add(-time.Second)
	plan, err = PlanLockRelease(policy, LockReleaseRequest{
		CacheKey:       "customer.query.list|-|abc",
		OwnerToken:     LockOwnerToken(" owner-1 "),
		Now:            now,
		Current:        current,
		CurrentPresent: true,
	})
	if err != nil {
		t.Fatalf("PlanLockRelease(stale owned) error = %v", err)
	}
	if !plan.CanRelease || !plan.Delete || !plan.Stale || plan.Action != LockReleaseDelete {
		t.Fatalf("PlanLockRelease(stale owned) action = %s release=%v delete=%v stale=%v", plan.Action, plan.CanRelease, plan.Delete, plan.Stale)
	}
}

func TestPlanLockReleaseRejectsInvalidInputs(t *testing.T) {
	policy := LockPolicy{KeyPrefix: "locks:"}
	if _, err := PlanLockRelease(policy, LockReleaseRequest{
		CacheKey: "customer.query.list|-|abc",
	}); !errors.Is(err, ErrInvalidLockOwnerToken) {
		t.Fatalf("PlanLockRelease(empty owner) error = %v, want ErrInvalidLockOwnerToken", err)
	}

	current := LockMetadata{
		Key:        "locks:other",
		OwnerToken: LockOwnerToken("owner-1"),
		AcquiredAt: time.Now(),
		ExpiresAt:  time.Now().Add(time.Minute),
	}
	if _, err := PlanLockRelease(policy, LockReleaseRequest{
		CacheKey:       "customer.query.list|-|abc",
		OwnerToken:     LockOwnerToken("owner-1"),
		Current:        current,
		CurrentPresent: true,
	}); !errors.Is(err, ErrInvalidLockPlan) {
		t.Fatalf("PlanLockRelease(mismatched key) error = %v, want ErrInvalidLockPlan", err)
	}
}
