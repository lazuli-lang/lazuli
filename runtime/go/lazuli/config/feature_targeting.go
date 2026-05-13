package config

import "strings"

// RolloutTargetKind identifies the target dimension that matched a rollout
// rule or supplied a percentage bucket key.
type RolloutTargetKind string

const (
	// RolloutTargetKindTenant identifies a tenant rollout target.
	RolloutTargetKindTenant RolloutTargetKind = "tenant"
	// RolloutTargetKindUser identifies a user rollout target.
	RolloutTargetKindUser RolloutTargetKind = "user"
)

// RolloutTargetMatch records the tenant or user key that matched a rollout
// rule or was used for percentage bucketing.
type RolloutTargetMatch struct {
	Kind RolloutTargetKind
	Key  string
}

// RolloutReason classifies why a rollout target was enabled or disabled.
type RolloutReason string

const (
	// RolloutReasonDenyTarget means a deny tenant or user target matched.
	RolloutReasonDenyTarget RolloutReason = "deny_target"
	// RolloutReasonAllowTarget means an allow tenant or user target matched.
	RolloutReasonAllowTarget RolloutReason = "allow_target"
	// RolloutReasonPercentage means the target was decided by stable bucketing.
	RolloutReasonPercentage RolloutReason = "percentage"
	// RolloutReasonPercentageZero means no explicit target matched and the
	// rollout percentage is 0.
	RolloutReasonPercentageZero RolloutReason = "percentage_zero"
	// RolloutReasonPercentageFull means no explicit target matched and the
	// rollout percentage is 100.
	RolloutReasonPercentageFull RolloutReason = "percentage_full"
)

// RolloutExplanation is the provider-neutral evaluation output for a rollout.
//
// Bucket is meaningful only when Bucketed is true. Match records the allow/deny
// target that matched, or the target key used for percentage bucketing.
type RolloutExplanation struct {
	Enabled    bool
	Reason     RolloutReason
	Feature    string
	Percentage int
	Key        RolloutKey
	Target     RolloutTarget
	Match      RolloutTargetMatch
	Bucket     int
	Bucketed   bool
}

// Explain evaluates target and returns a structured explanation for the
// decision.
func (r Rollout) Explain(target RolloutTarget) (RolloutExplanation, error) {
	if err := r.Validate(); err != nil {
		return RolloutExplanation{}, err
	}

	target = target.normalized()
	explanation := RolloutExplanation{
		Feature:    strings.TrimSpace(r.Feature),
		Percentage: r.Percentage,
		Key:        r.Key,
		Target:     target,
	}

	if match, ok := r.Deny.Match(target); ok {
		explanation.Reason = RolloutReasonDenyTarget
		explanation.Match = match
		return explanation, nil
	}
	if match, ok := r.Allow.Match(target); ok {
		explanation.Enabled = true
		explanation.Reason = RolloutReasonAllowTarget
		explanation.Match = match
		return explanation, nil
	}
	if r.Percentage == 0 {
		explanation.Reason = RolloutReasonPercentageZero
		return explanation, nil
	}
	if r.Percentage == 100 {
		explanation.Enabled = true
		explanation.Reason = RolloutReasonPercentageFull
		return explanation, nil
	}

	key, err := r.evaluationKey(target)
	if err != nil {
		return RolloutExplanation{}, err
	}
	bucket, err := RolloutBucket(r.Feature, key)
	if err != nil {
		return RolloutExplanation{}, err
	}

	explanation.Enabled = bucket < r.Percentage
	explanation.Reason = RolloutReasonPercentage
	explanation.Match = RolloutTargetMatch{
		Kind: rolloutTargetKindForKey(r.Key),
		Key:  key,
	}
	explanation.Bucket = bucket
	explanation.Bucketed = true
	return explanation, nil
}

// Match reports whether targets contains target's tenant or user key.
//
// Tenant matches are reported before user matches, matching Rollout evaluation
// order.
func (targets RolloutTargets) Match(target RolloutTarget) (RolloutTargetMatch, bool) {
	target = target.normalized()
	if key, ok := matchRolloutTargetKey(targets.Tenants, target.TenantKey); ok {
		return RolloutTargetMatch{Kind: RolloutTargetKindTenant, Key: key}, true
	}
	if key, ok := matchRolloutTargetKey(targets.Users, target.UserKey); ok {
		return RolloutTargetMatch{Kind: RolloutTargetKindUser, Key: key}, true
	}
	return RolloutTargetMatch{}, false
}

func matchRolloutTargetKey(values []string, key string) (string, bool) {
	key = strings.TrimSpace(key)
	if key == "" {
		return "", false
	}
	for _, value := range values {
		if strings.TrimSpace(value) == key {
			return key, true
		}
	}
	return "", false
}

func rolloutTargetKindForKey(key RolloutKey) RolloutTargetKind {
	switch key {
	case RolloutKeyTenant:
		return RolloutTargetKindTenant
	case RolloutKeyUser:
		return RolloutTargetKindUser
	default:
		return ""
	}
}
