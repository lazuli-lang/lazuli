package webhooks

import (
	"errors"
	"testing"
	"time"

	"lazuli.dev/runtime/lazuli/jobs"
)

func TestReplayWindowForSpec(t *testing.T) {
	window, enabled, err := ReplayWindowForSpec(&ReplaySpec{Mode: ReplayAllow, Window: "2h"})
	if err != nil {
		t.Fatalf("ReplayWindowForSpec returned error: %v", err)
	}
	if !enabled {
		t.Fatal("ReplayWindowForSpec enabled = false, want true")
	}
	if window != 2*time.Hour {
		t.Fatalf("ReplayWindowForSpec window = %s, want 2h", window)
	}

	window, enabled, err = ReplayWindowForSpec(&ReplaySpec{Mode: ReplayDeny})
	if err != nil {
		t.Fatalf("ReplayWindowForSpec deny returned error: %v", err)
	}
	if enabled || window != 0 {
		t.Fatalf("ReplayWindowForSpec deny = (%s, %t), want disabled zero window", window, enabled)
	}
}

func TestReplayWindowForSpecRejectsInvalidValues(t *testing.T) {
	_, _, err := ReplayWindowForSpec(&ReplaySpec{Mode: ReplayAllow, Window: "later"})
	if !errors.Is(err, ErrWebhookReplayWindowInvalid) {
		t.Fatalf("ReplayWindowForSpec invalid window error = %v, want ErrWebhookReplayWindowInvalid", err)
	}

	_, _, err = ReplayWindowForSpec(&ReplaySpec{Mode: ReplayMode(99), Window: "1h"})
	if !errors.Is(err, ErrWebhookReplayModeInvalid) {
		t.Fatalf("ReplayWindowForSpec invalid mode error = %v, want ErrWebhookReplayModeInvalid", err)
	}
}

func TestReplayPolicyForContractDerivesWindowAttemptsAndIdempotency(t *testing.T) {
	contract := WebhookContract{
		Replay: &ReplaySpec{Mode: ReplayAllow, Window: "90m"},
		Retry:  &jobs.RetryPolicy{Count: 2},
	}

	policy, err := ReplayPolicyForContract(contract)
	if err != nil {
		t.Fatalf("ReplayPolicyForContract returned error: %v", err)
	}
	if policy.ReplayWindow != 90*time.Minute {
		t.Fatalf("ReplayWindow = %s, want 90m", policy.ReplayWindow)
	}
	if policy.MaxAttempts != 3 {
		t.Fatalf("MaxAttempts = %d, want 3", policy.MaxAttempts)
	}
	if policy.SignatureFreshness != DefaultReplaySignatureFreshness {
		t.Fatalf("SignatureFreshness = %s, want %s", policy.SignatureFreshness, DefaultReplaySignatureFreshness)
	}
	if !policy.RequireIdempotency {
		t.Fatal("RequireIdempotency = false, want true")
	}
}

func TestValidateReplayPolicyRejectsInvalidBounds(t *testing.T) {
	policy := ReplayPolicy{
		ReplayWindow:       -time.Second,
		MaxAttempts:        -1,
		SignatureFreshness: -time.Minute,
	}

	err := ValidateReplayPolicy(policy)
	if !errors.Is(err, ErrWebhookReplayPolicyInvalid) {
		t.Fatalf("ValidateReplayPolicy error = %v, want ErrWebhookReplayPolicyInvalid", err)
	}
}

func TestValidateReplayPolicyForContractRequiresIdempotency(t *testing.T) {
	contract := WebhookContract{
		Replay: &ReplaySpec{Mode: ReplayAllow, Window: "1h"},
	}

	err := ValidateReplayPolicyForContract(ReplayPolicy{}, contract)
	if !errors.Is(err, ErrWebhookIdempotencyRequired) {
		t.Fatalf("ValidateReplayPolicyForContract error = %v, want ErrWebhookIdempotencyRequired", err)
	}

	contract.IdempotencyBy = "payload.id"
	if err := ValidateReplayPolicyForContract(ReplayPolicy{}, contract); err != nil {
		t.Fatalf("ValidateReplayPolicyForContract with idempotency returned error: %v", err)
	}
}

func TestReplayPolicyExplicitIdempotencyRequirement(t *testing.T) {
	policy := ReplayPolicy{RequireIdempotency: true}
	contract := WebhookContract{}

	err := policy.ValidateContract(contract)
	if !errors.Is(err, ErrWebhookIdempotencyRequired) {
		t.Fatalf("ValidateContract error = %v, want ErrWebhookIdempotencyRequired", err)
	}

	contract.IdempotencyBy = " payload.account_id , payload.event_id "
	if err := policy.ValidateContract(contract); err != nil {
		t.Fatalf("ValidateContract with idempotency returned error: %v", err)
	}
}

func TestCheckReplayAttempt(t *testing.T) {
	if err := CheckReplayAttempt(2, 3); err != nil {
		t.Fatalf("CheckReplayAttempt within budget returned error: %v", err)
	}

	err := CheckReplayAttempt(4, 3)
	if !errors.Is(err, ErrWebhookReplayAttemptsExceeded) {
		t.Fatalf("CheckReplayAttempt exceeded error = %v, want ErrWebhookReplayAttemptsExceeded", err)
	}

	err = CheckReplayAttempt(0, 3)
	if !errors.Is(err, ErrWebhookReplayPolicyInvalid) {
		t.Fatalf("CheckReplayAttempt invalid attempt error = %v, want ErrWebhookReplayPolicyInvalid", err)
	}
}

func TestCheckSignatureFreshness(t *testing.T) {
	now := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)

	if err := CheckSignatureFreshness(now, now.Add(-2*time.Minute), 5*time.Minute); err != nil {
		t.Fatalf("CheckSignatureFreshness fresh signature returned error: %v", err)
	}

	err := CheckSignatureFreshness(now, now.Add(-6*time.Minute), 5*time.Minute)
	if !errors.Is(err, ErrWebhookSignatureExpired) {
		t.Fatalf("CheckSignatureFreshness expired error = %v, want ErrWebhookSignatureExpired", err)
	}

	err = CheckSignatureFreshness(now, now.Add(time.Second), 5*time.Minute)
	if !errors.Is(err, ErrWebhookSignatureTimestampInvalid) {
		t.Fatalf("CheckSignatureFreshness future error = %v, want ErrWebhookSignatureTimestampInvalid", err)
	}

	err = CheckSignatureFreshness(now, now, 0)
	if !errors.Is(err, ErrWebhookReplayPolicyInvalid) {
		t.Fatalf("CheckSignatureFreshness zero freshness error = %v, want ErrWebhookReplayPolicyInvalid", err)
	}
}
