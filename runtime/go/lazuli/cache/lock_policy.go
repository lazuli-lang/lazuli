package cache

import (
	"crypto/rand"
	"encoding/hex"
	"errors"
	"fmt"
	"strings"
	"time"
)

const (
	defaultLockKeyPrefix  = "lazuli:cache:lock:"
	defaultLockTTL        = 30 * time.Second
	defaultLockMinTTL     = time.Second
	defaultLockMaxTTL     = 5 * time.Minute
	defaultLockStaleAfter = 10 * time.Minute
	lockOwnerTokenBytes   = 16
)

var (
	// ErrInvalidLockPolicy reports an invalid adapter-neutral cache lock
	// policy before it reaches a concrete backend.
	ErrInvalidLockPolicy = errors.New("lazuli/cache: invalid lock policy")
	// ErrInvalidLockKey reports invalid cache-key input for lock-key
	// derivation.
	ErrInvalidLockKey = errors.New("lazuli/cache: invalid lock key")
	// ErrInvalidLockOwnerToken reports an empty or malformed lock owner token.
	ErrInvalidLockOwnerToken = errors.New("lazuli/cache: invalid lock owner token")
	// ErrInvalidLockPlan reports invalid inputs for acquisition or release
	// planning.
	ErrInvalidLockPlan = errors.New("lazuli/cache: invalid lock plan")
)

// LockOwnerToken is an opaque ownership token used to verify lock release.
type LockOwnerToken string

// NewLockOwnerToken returns a random hex-encoded ownership token.
func NewLockOwnerToken() (LockOwnerToken, error) {
	var raw [lockOwnerTokenBytes]byte
	if _, err := rand.Read(raw[:]); err != nil {
		return "", err
	}
	return LockOwnerToken(hex.EncodeToString(raw[:])), nil
}

// ParseLockOwnerToken trims and validates an externally supplied token.
func ParseLockOwnerToken(token string) (LockOwnerToken, error) {
	parsed := LockOwnerToken(strings.TrimSpace(token))
	if !parsed.Valid() {
		return "", ErrInvalidLockOwnerToken
	}
	return parsed, nil
}

// String returns the opaque token value.
func (t LockOwnerToken) String() string {
	return string(t)
}

// Valid reports whether the token can safely identify a lock owner.
func (t LockOwnerToken) Valid() bool {
	token := strings.TrimSpace(string(t))
	return token != "" && !hasLockControlRune(token)
}

// LockKeyParts describes the canonical input for deriving a lock key.
type LockKeyParts struct {
	// CacheKey is the already-built cache entry key being protected.
	CacheKey string
	// Prefix overrides the default lock-key prefix when set.
	Prefix string
}

// BuildLockKey returns the provider-neutral lock key for a cache entry key.
func BuildLockKey(parts LockKeyParts) (string, error) {
	prefix := strings.TrimSpace(parts.Prefix)
	if prefix == "" {
		prefix = defaultLockKeyPrefix
	}
	return buildLockKey(prefix, parts.CacheKey)
}

// LockPolicy describes adapter-neutral cache lock behavior.
//
// Zero values use conservative defaults. Concrete adapters should still use
// atomic provider primitives for acquisition and release; this policy only
// computes metadata and dry-run decisions.
type LockPolicy struct {
	// KeyPrefix is prepended to a cache entry key to derive its lock key.
	KeyPrefix string
	// DefaultTTL is used when an acquisition request omits TTL.
	DefaultTTL time.Duration
	// MinTTL is the lower bound applied to requested lock TTLs.
	MinTTL time.Duration
	// MaxTTL is the upper bound applied to requested lock TTLs.
	MaxTTL time.Duration
	// StaleAfter marks metadata stale when its acquisition timestamp is older
	// than this window. Expired metadata is stale regardless of this value.
	StaleAfter time.Duration
}

// Normalize fills zero-value policy fields with runtime defaults.
func (p LockPolicy) Normalize() LockPolicy {
	p.KeyPrefix = strings.TrimSpace(p.KeyPrefix)
	if p.KeyPrefix == "" {
		p.KeyPrefix = defaultLockKeyPrefix
	}
	if p.DefaultTTL == 0 {
		p.DefaultTTL = defaultLockTTL
	}
	if p.MinTTL == 0 {
		p.MinTTL = defaultLockMinTTL
	}
	if p.MaxTTL == 0 {
		p.MaxTTL = defaultLockMaxTTL
	}
	if p.StaleAfter == 0 {
		p.StaleAfter = defaultLockStaleAfter
	}
	return p
}

// Validate reports invalid policy bounds or key-prefix values.
func (p LockPolicy) Validate() error {
	p = p.Normalize()

	var errs []error
	if p.KeyPrefix == "" {
		errs = append(errs, fmt.Errorf("%w: KeyPrefix is required", ErrInvalidLockPolicy))
	}
	if hasLockControlRune(p.KeyPrefix) {
		errs = append(errs, fmt.Errorf("%w: KeyPrefix contains control characters", ErrInvalidLockPolicy))
	}
	if p.DefaultTTL <= 0 {
		errs = append(errs, fmt.Errorf("%w: DefaultTTL must be positive", ErrInvalidLockPolicy))
	}
	if p.MinTTL <= 0 {
		errs = append(errs, fmt.Errorf("%w: MinTTL must be positive", ErrInvalidLockPolicy))
	}
	if p.MaxTTL <= 0 {
		errs = append(errs, fmt.Errorf("%w: MaxTTL must be positive", ErrInvalidLockPolicy))
	}
	if p.MinTTL > 0 && p.MaxTTL > 0 && p.MinTTL > p.MaxTTL {
		errs = append(errs, fmt.Errorf("%w: MinTTL must not exceed MaxTTL", ErrInvalidLockPolicy))
	}
	if p.DefaultTTL > 0 && p.MinTTL > 0 && p.DefaultTTL < p.MinTTL {
		errs = append(errs, fmt.Errorf("%w: DefaultTTL must not be below MinTTL", ErrInvalidLockPolicy))
	}
	if p.DefaultTTL > 0 && p.MaxTTL > 0 && p.DefaultTTL > p.MaxTTL {
		errs = append(errs, fmt.Errorf("%w: DefaultTTL must not exceed MaxTTL", ErrInvalidLockPolicy))
	}
	if p.StaleAfter < 0 {
		errs = append(errs, fmt.Errorf("%w: StaleAfter must not be negative", ErrInvalidLockPolicy))
	}
	return errors.Join(errs...)
}

// BuildKey derives a lock key for cacheKey using the policy prefix.
func (p LockPolicy) BuildKey(cacheKey string) (string, error) {
	p = p.Normalize()
	if err := p.Validate(); err != nil {
		return "", err
	}
	return buildLockKey(p.KeyPrefix, cacheKey)
}

// ResolveTTL returns the effective lock TTL after applying policy defaults and
// bounds. A zero requested TTL uses DefaultTTL.
func (p LockPolicy) ResolveTTL(requested time.Duration) (time.Duration, error) {
	return ResolveLockTTL(p, requested)
}

// ResolveLockTTL returns the effective lock TTL after applying policy defaults
// and bounds. A zero requested TTL uses DefaultTTL.
func ResolveLockTTL(policy LockPolicy, requested time.Duration) (time.Duration, error) {
	policy = policy.Normalize()
	if err := policy.Validate(); err != nil {
		return 0, err
	}
	if requested < 0 {
		return 0, fmt.Errorf("%w: TTL must not be negative", ErrInvalidLockPlan)
	}

	ttl := requested
	if ttl == 0 {
		ttl = policy.DefaultTTL
	}
	if ttl < policy.MinTTL {
		return policy.MinTTL, nil
	}
	if ttl > policy.MaxTTL {
		return policy.MaxTTL, nil
	}
	return ttl, nil
}

// LockMetadata is the provider-neutral value stored with a cache lock.
type LockMetadata struct {
	// Key is the derived lock key.
	Key string
	// OwnerToken is the opaque token that must match on release.
	OwnerToken LockOwnerToken
	// AcquiredAt is when the owner took the lock.
	AcquiredAt time.Time
	// ExpiresAt is the planned lock expiration time.
	ExpiresAt time.Time
}

// OwnedBy reports whether token owns the metadata.
func (m LockMetadata) OwnedBy(token LockOwnerToken) bool {
	owner, ok := normalizeLockOwnerToken(token)
	current, currentOK := normalizeLockOwnerToken(m.OwnerToken)
	return ok && currentOK && owner == current
}

// Expired reports whether the metadata is past its planned expiration.
func (m LockMetadata) Expired(now time.Time) bool {
	now = lockPlanNow(now)
	return m.ExpiresAt.IsZero() || !now.Before(m.ExpiresAt)
}

// Stale reports whether metadata should be treated as abandoned.
func (m LockMetadata) Stale(policy LockPolicy, now time.Time) bool {
	return IsLockStale(policy, m, now)
}

// IsLockStale reports whether metadata is expired, malformed, or older than
// the policy stale window.
func IsLockStale(policy LockPolicy, metadata LockMetadata, now time.Time) bool {
	policy = policy.Normalize()
	now = lockPlanNow(now)
	if !metadata.OwnerToken.Valid() || metadata.Expired(now) {
		return true
	}
	if policy.StaleAfter > 0 && !metadata.AcquiredAt.IsZero() && !now.Before(metadata.AcquiredAt.Add(policy.StaleAfter)) {
		return true
	}
	return false
}

// LockAcquireAction is the provider operation selected by acquisition
// planning.
type LockAcquireAction string

const (
	// LockAcquireSet stores a new lock when none is currently present.
	LockAcquireSet LockAcquireAction = "set"
	// LockAcquireReplaceStale replaces stale metadata for the same lock key.
	LockAcquireReplaceStale LockAcquireAction = "replace_stale"
	// LockAcquireAlreadyOwned means the requested owner already holds the lock.
	LockAcquireAlreadyOwned LockAcquireAction = "already_owned"
	// LockAcquireWait means another live owner holds the lock.
	LockAcquireWait LockAcquireAction = "wait"
)

// LockAcquireRequest describes one adapter-neutral acquisition decision.
type LockAcquireRequest struct {
	CacheKey       string
	OwnerToken     LockOwnerToken
	TTL            time.Duration
	Now            time.Time
	Current        LockMetadata
	CurrentPresent bool
}

// LockAcquirePlan is the dry-run result for acquiring a cache lock.
type LockAcquirePlan struct {
	Key        string
	OwnerToken LockOwnerToken
	TTL        time.Duration
	AcquiredAt time.Time
	ExpiresAt  time.Time
	Metadata   LockMetadata

	Action        LockAcquireAction
	CanAcquire    bool
	WriteRequired bool
	ReplaceStale  bool

	Current        LockMetadata
	CurrentPresent bool
	Reason         string
}

// PlanLockAcquire builds the metadata and adapter action for one acquisition
// attempt without mutating any backend state.
func PlanLockAcquire(policy LockPolicy, request LockAcquireRequest) (LockAcquirePlan, error) {
	policy = policy.Normalize()
	if err := policy.Validate(); err != nil {
		return LockAcquirePlan{}, err
	}

	key, err := policy.BuildKey(request.CacheKey)
	if err != nil {
		return LockAcquirePlan{}, err
	}
	token, ok := normalizeLockOwnerToken(request.OwnerToken)
	if !ok {
		return LockAcquirePlan{}, ErrInvalidLockOwnerToken
	}
	ttl, err := policy.ResolveTTL(request.TTL)
	if err != nil {
		return LockAcquirePlan{}, err
	}

	now := lockPlanNow(request.Now)
	metadata := LockMetadata{
		Key:        key,
		OwnerToken: token,
		AcquiredAt: now,
		ExpiresAt:  now.Add(ttl),
	}
	plan := LockAcquirePlan{
		Key:            key,
		OwnerToken:     token,
		TTL:            ttl,
		AcquiredAt:     metadata.AcquiredAt,
		ExpiresAt:      metadata.ExpiresAt,
		Metadata:       metadata,
		Current:        request.Current,
		CurrentPresent: request.CurrentPresent,
	}

	if !request.CurrentPresent {
		plan.Action = LockAcquireSet
		plan.CanAcquire = true
		plan.WriteRequired = true
		plan.Reason = "lock is not held"
		return plan, nil
	}
	if currentKey := strings.TrimSpace(request.Current.Key); currentKey != "" && currentKey != key {
		return LockAcquirePlan{}, fmt.Errorf("%w: current lock key %q does not match %q", ErrInvalidLockPlan, currentKey, key)
	}
	if request.Current.OwnedBy(token) && !request.Current.Stale(policy, now) {
		plan.Action = LockAcquireAlreadyOwned
		plan.CanAcquire = true
		plan.Metadata = request.Current
		plan.ExpiresAt = request.Current.ExpiresAt
		plan.Reason = "lock is already owned by token"
		return plan, nil
	}
	if request.Current.Stale(policy, now) {
		plan.Action = LockAcquireReplaceStale
		plan.CanAcquire = true
		plan.WriteRequired = true
		plan.ReplaceStale = true
		plan.Reason = "lock metadata is stale"
		return plan, nil
	}

	plan.Action = LockAcquireWait
	plan.Reason = "lock is held by another owner"
	return plan, nil
}

// LockReleaseAction is the provider operation selected by release planning.
type LockReleaseAction string

const (
	// LockReleaseDelete deletes metadata after ownership is verified.
	LockReleaseDelete LockReleaseAction = "delete"
	// LockReleaseSkipMissing means there is no metadata to release.
	LockReleaseSkipMissing LockReleaseAction = "skip_missing"
	// LockReleaseRejectOwner means the current metadata belongs to another
	// owner token.
	LockReleaseRejectOwner LockReleaseAction = "reject_owner"
)

// LockReleaseRequest describes one adapter-neutral release decision.
type LockReleaseRequest struct {
	CacheKey       string
	OwnerToken     LockOwnerToken
	Now            time.Time
	Current        LockMetadata
	CurrentPresent bool
}

// LockReleasePlan is the dry-run result for releasing a cache lock.
type LockReleasePlan struct {
	Key        string
	OwnerToken LockOwnerToken

	Action     LockReleaseAction
	CanRelease bool
	Delete     bool
	Stale      bool

	Current        LockMetadata
	CurrentPresent bool
	Reason         string
}

// PlanLockRelease selects the adapter action for one release attempt without
// mutating any backend state.
func PlanLockRelease(policy LockPolicy, request LockReleaseRequest) (LockReleasePlan, error) {
	policy = policy.Normalize()
	if err := policy.Validate(); err != nil {
		return LockReleasePlan{}, err
	}

	key, err := policy.BuildKey(request.CacheKey)
	if err != nil {
		return LockReleasePlan{}, err
	}
	token, ok := normalizeLockOwnerToken(request.OwnerToken)
	if !ok {
		return LockReleasePlan{}, ErrInvalidLockOwnerToken
	}

	plan := LockReleasePlan{
		Key:            key,
		OwnerToken:     token,
		Current:        request.Current,
		CurrentPresent: request.CurrentPresent,
	}
	if !request.CurrentPresent {
		plan.Action = LockReleaseSkipMissing
		plan.Reason = "lock metadata is missing"
		return plan, nil
	}
	if currentKey := strings.TrimSpace(request.Current.Key); currentKey != "" && currentKey != key {
		return LockReleasePlan{}, fmt.Errorf("%w: current lock key %q does not match %q", ErrInvalidLockPlan, currentKey, key)
	}

	plan.Stale = request.Current.Stale(policy, request.Now)
	if !request.Current.OwnedBy(token) {
		plan.Action = LockReleaseRejectOwner
		plan.Reason = "lock owner token does not match"
		return plan, nil
	}

	plan.Action = LockReleaseDelete
	plan.CanRelease = true
	plan.Delete = true
	plan.Reason = "lock owner token matches"
	return plan, nil
}

func buildLockKey(prefix, cacheKey string) (string, error) {
	prefix = strings.TrimSpace(prefix)
	cacheKey = strings.TrimSpace(cacheKey)
	if prefix == "" {
		return "", fmt.Errorf("%w: prefix is required", ErrInvalidLockKey)
	}
	if cacheKey == "" {
		return "", fmt.Errorf("%w: cache key is required", ErrInvalidLockKey)
	}
	if hasLockControlRune(prefix) || hasLockControlRune(cacheKey) {
		return "", fmt.Errorf("%w: contains control characters", ErrInvalidLockKey)
	}
	return prefix + cacheKey, nil
}

func normalizeLockOwnerToken(token LockOwnerToken) (LockOwnerToken, bool) {
	normalized := LockOwnerToken(strings.TrimSpace(string(token)))
	if !normalized.Valid() {
		return "", false
	}
	return normalized, true
}

func lockPlanNow(now time.Time) time.Time {
	if now.IsZero() {
		return time.Now()
	}
	return now
}

func hasLockControlRune(value string) bool {
	for _, r := range value {
		if r < 0x20 || r == 0x7f {
			return true
		}
	}
	return false
}
