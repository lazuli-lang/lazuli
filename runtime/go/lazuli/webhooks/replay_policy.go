package webhooks

import (
	"errors"
	"fmt"
	"strings"
	"time"
)

const (
	// DefaultReplayPolicyMaxAttempts is the default total delivery budget,
	// including the first attempt.
	DefaultReplayPolicyMaxAttempts = 1

	// DefaultReplaySignatureFreshness is a conservative default for providers
	// that sign a timestamped webhook envelope.
	DefaultReplaySignatureFreshness = 5 * time.Minute
)

var (
	// ErrWebhookReplayPolicyInvalid is returned when a replay policy contains
	// invalid duration or attempt bounds.
	ErrWebhookReplayPolicyInvalid = errors.New("webhooks: replay policy invalid")

	// ErrWebhookReplayAttemptsExceeded means the current delivery attempt is
	// outside the policy's total attempt budget.
	ErrWebhookReplayAttemptsExceeded = errors.New("webhooks: replay attempts exceeded")

	// ErrWebhookSignatureTimestampInvalid means the signature timestamp is
	// missing, zero, or later than the validation clock.
	ErrWebhookSignatureTimestampInvalid = errors.New("webhooks: signature timestamp invalid")

	// ErrWebhookSignatureExpired means the signature timestamp is older than
	// the accepted freshness window.
	ErrWebhookSignatureExpired = errors.New("webhooks: signature freshness expired")

	// ErrWebhookIdempotencyRequired means a replay-capable or retried webhook
	// contract does not declare an idempotency key.
	ErrWebhookIdempotencyRequired = errors.New("webhooks: idempotency required")
)

// ReplayPolicy describes provider-neutral replay and retry safety bounds.
//
// ReplayWindow and SignatureFreshness are optional caps; zero means the caller
// does not want that specific bound enforced by this policy. MaxAttempts is the
// total delivery attempt budget including the first attempt; zero normalizes to
// DefaultReplayPolicyMaxAttempts.
type ReplayPolicy struct {
	ReplayWindow       time.Duration
	MaxAttempts        int
	SignatureFreshness time.Duration
	RequireIdempotency bool
}

// DefaultReplayPolicy returns conservative defaults for timestamped webhook
// deliveries. Contracts may still tighten the replay window with ReplaySpec.
func DefaultReplayPolicy() ReplayPolicy {
	return ReplayPolicy{
		MaxAttempts:        DefaultReplayPolicyMaxAttempts,
		SignatureFreshness: DefaultReplaySignatureFreshness,
	}
}

// ReplayPolicyForContract derives the replay policy declared by a webhook
// contract and applies provider-neutral freshness defaults.
func ReplayPolicyForContract(contract WebhookContract) (ReplayPolicy, error) {
	window, replayEnabled, err := ReplayWindowForSpec(contract.Replay)
	if err != nil {
		return ReplayPolicy{}, err
	}

	policy := DefaultReplayPolicy()
	policy.MaxAttempts = MaxReplayAttempts(contract)
	policy.RequireIdempotency = RequiresReplayIdempotency(contract)
	if replayEnabled {
		policy.ReplayWindow = window
	}
	return policy, policy.Validate()
}

// Normalize fills default attempt bounds while preserving explicit duration
// caps and idempotency requirements.
func (p ReplayPolicy) Normalize() ReplayPolicy {
	if p.MaxAttempts == 0 {
		p.MaxAttempts = DefaultReplayPolicyMaxAttempts
	}
	return p
}

// Validate checks that a policy's duration and attempt bounds are coherent.
func (p ReplayPolicy) Validate() error {
	return ValidateReplayPolicy(p)
}

// ValidateContract checks the policy and contract-level idempotency
// requirements together.
func (p ReplayPolicy) ValidateContract(contract WebhookContract) error {
	return ValidateReplayPolicyForContract(p, contract)
}

// AllowsAttempt reports whether attempt is within the one-based delivery
// attempt budget.
func (p ReplayPolicy) AllowsAttempt(attempt int) bool {
	return CheckReplayAttempt(attempt, p.Normalize().MaxAttempts) == nil
}

// RequiresIdempotency reports whether policy or contract replay behavior needs
// a stable idempotency key.
func (p ReplayPolicy) RequiresIdempotency(contract WebhookContract) bool {
	p = p.Normalize()
	return p.RequireIdempotency ||
		p.ReplayWindow > 0 ||
		p.MaxAttempts > 1 ||
		RequiresReplayIdempotency(contract)
}

// ReplayWindowForSpec resolves the replay window declared by spec.
//
// The bool result reports whether replay-window enforcement is enabled. Nil
// specs and ReplayDeny return false with no error.
func ReplayWindowForSpec(spec *ReplaySpec) (time.Duration, bool, error) {
	if spec == nil {
		return 0, false, nil
	}

	switch spec.Mode {
	case ReplayDeny:
		return 0, false, nil
	case ReplayAllow:
	default:
		return 0, false, ErrWebhookReplayModeInvalid
	}

	window, err := parseReplayWindow(spec.Window)
	if err != nil {
		return 0, false, fmt.Errorf("%w: %v", ErrWebhookReplayWindowInvalid, err)
	}
	return window, true, nil
}

// MaxReplayAttempts returns the total attempt budget for a webhook contract,
// including the first delivery attempt. A nil retry policy means one attempt.
func MaxReplayAttempts(contract WebhookContract) int {
	if contract.Retry == nil {
		return DefaultReplayPolicyMaxAttempts
	}
	maxAttempts := int(^uint(0) >> 1)
	if uint64(contract.Retry.Count) >= uint64(maxAttempts) {
		return maxAttempts
	}
	return int(contract.Retry.Count) + 1
}

// RequiresReplayIdempotency reports whether the contract declares behavior
// that must be guarded by an idempotency key.
func RequiresReplayIdempotency(contract WebhookContract) bool {
	if contract.Replay != nil && contract.Replay.Mode == ReplayAllow {
		return true
	}
	return MaxReplayAttempts(contract) > 1
}

// ValidateReplayPolicy checks that policy fields can be safely interpreted.
func ValidateReplayPolicy(policy ReplayPolicy) error {
	policy = policy.Normalize()

	var errs []error
	if policy.ReplayWindow < 0 {
		errs = append(errs, fmt.Errorf("%w: replay window must be non-negative", ErrWebhookReplayPolicyInvalid))
	}
	if policy.MaxAttempts < 1 {
		errs = append(errs, fmt.Errorf("%w: max attempts must be positive", ErrWebhookReplayPolicyInvalid))
	}
	if policy.SignatureFreshness < 0 {
		errs = append(errs, fmt.Errorf("%w: signature freshness must be non-negative", ErrWebhookReplayPolicyInvalid))
	}
	return errors.Join(errs...)
}

// ValidateReplayPolicyForContract checks policy shape, contract replay spec,
// and any idempotency requirement implied by replay/retry behavior.
func ValidateReplayPolicyForContract(policy ReplayPolicy, contract WebhookContract) error {
	var errs []error
	if err := ValidateReplayPolicy(policy); err != nil {
		errs = append(errs, err)
	}
	if _, _, err := ReplayWindowForSpec(contract.Replay); err != nil {
		errs = append(errs, fmt.Errorf("%w: %w", ErrWebhookReplayPolicyInvalid, err))
	}
	if policy.RequiresIdempotency(contract) && !hasReplayIdempotencyKey(contract) {
		errs = append(errs, ErrWebhookIdempotencyRequired)
	}
	return errors.Join(errs...)
}

// CheckReplayAttempt validates a one-based delivery attempt against the total
// attempt budget.
func CheckReplayAttempt(attempt, maxAttempts int) error {
	if maxAttempts < 1 {
		return fmt.Errorf("%w: max attempts must be positive", ErrWebhookReplayPolicyInvalid)
	}
	if attempt < 1 {
		return fmt.Errorf("%w: attempt must be positive", ErrWebhookReplayPolicyInvalid)
	}
	if attempt > maxAttempts {
		return fmt.Errorf("%w: attempt %d exceeds max attempts %d", ErrWebhookReplayAttemptsExceeded, attempt, maxAttempts)
	}
	return nil
}

// CheckSignatureFreshness validates a signature timestamp against a freshness
// window. The timestamp must be non-zero, not in the future, and within the
// supplied positive freshness duration.
func CheckSignatureFreshness(now, signedAt time.Time, freshness time.Duration) error {
	if freshness <= 0 {
		return fmt.Errorf("%w: signature freshness must be positive", ErrWebhookReplayPolicyInvalid)
	}
	if now.IsZero() {
		now = time.Now()
	}
	if signedAt.IsZero() || signedAt.After(now) {
		return ErrWebhookSignatureTimestampInvalid
	}
	if now.Sub(signedAt) > freshness {
		return ErrWebhookSignatureExpired
	}
	return nil
}

func hasReplayIdempotencyKey(contract WebhookContract) bool {
	for _, path := range strings.Split(contract.IdempotencyBy, ",") {
		if strings.TrimSpace(path) != "" {
			return true
		}
	}
	return false
}
