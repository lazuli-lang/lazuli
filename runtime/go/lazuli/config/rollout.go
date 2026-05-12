package config

import (
	"crypto/sha256"
	"encoding/binary"
	"errors"
	"fmt"
	"strings"
)

var (
	// ErrInvalidRollout indicates an invalid feature rollout definition.
	ErrInvalidRollout = errors.New("config: invalid rollout")
	// ErrInvalidRolloutTarget indicates a missing or invalid rollout target.
	ErrInvalidRolloutTarget = errors.New("config: invalid rollout target")
)

// RolloutKey selects the target key used for percentage rollout hashing.
type RolloutKey string

const (
	// RolloutKeyTenant hashes RolloutTarget.TenantKey.
	RolloutKeyTenant RolloutKey = "tenant"
	// RolloutKeyUser hashes RolloutTarget.UserKey.
	RolloutKeyUser RolloutKey = "user"
)

// Rollout describes a feature rollout policy.
//
// Deny targets win over allow targets. Allow targets win over percentage
// rollout. Percentage is an integer in [0, 100].
type Rollout struct {
	// Feature is the stable feature name used as the percentage hash seed.
	Feature string
	// Percentage enables a stable percentage of targets after lists are checked.
	Percentage int
	// Key chooses whether percentage rollout hashes the tenant or user key.
	Key RolloutKey
	// Allow targets are always enabled unless also present in Deny.
	Allow RolloutTargets
	// Deny targets are always disabled.
	Deny RolloutTargets
}

// RolloutTargets lists tenant and user keys used by allow and deny rules.
type RolloutTargets struct {
	Tenants []string
	Users   []string
}

// RolloutTarget is the tenant and/or user being evaluated.
type RolloutTarget struct {
	TenantKey string
	UserKey   string
}

// Validate checks whether the rollout definition can be evaluated.
func (r Rollout) Validate() error {
	var errs []error

	if r.Percentage < 0 || r.Percentage > 100 {
		errs = append(errs, fmt.Errorf("%w: percentage must be between 0 and 100", ErrInvalidRollout))
	}
	if r.Key != "" && r.Key != RolloutKeyTenant && r.Key != RolloutKeyUser {
		errs = append(errs, fmt.Errorf("%w: key must be %q or %q", ErrInvalidRollout, RolloutKeyTenant, RolloutKeyUser))
	}
	if r.Percentage > 0 && r.Percentage < 100 {
		if strings.TrimSpace(r.Feature) == "" {
			errs = append(errs, fmt.Errorf("%w: feature is required for partial percentage rollout", ErrInvalidRollout))
		}
		if r.Key == "" {
			errs = append(errs, fmt.Errorf("%w: key is required for partial percentage rollout", ErrInvalidRollout))
		}
	}
	if err := r.Allow.validate("allow"); err != nil {
		errs = append(errs, err)
	}
	if err := r.Deny.validate("deny"); err != nil {
		errs = append(errs, err)
	}

	return errors.Join(errs...)
}

// Evaluate reports whether target is enabled by the rollout policy.
func (r Rollout) Evaluate(target RolloutTarget) (bool, error) {
	if err := r.Validate(); err != nil {
		return false, err
	}

	target = target.normalized()
	if r.Deny.matches(target) {
		return false, nil
	}
	if r.Allow.matches(target) {
		return true, nil
	}
	if r.Percentage == 0 {
		return false, nil
	}
	if r.Percentage == 100 {
		return true, nil
	}

	key, err := r.evaluationKey(target)
	if err != nil {
		return false, err
	}
	return PercentageRollout(r.Feature, key, r.Percentage)
}

// PercentageRollout reports whether key falls inside percentage for feature.
func PercentageRollout(feature, key string, percentage int) (bool, error) {
	if percentage < 0 || percentage > 100 {
		return false, fmt.Errorf("%w: percentage must be between 0 and 100", ErrInvalidRollout)
	}
	if percentage == 0 {
		return false, nil
	}
	if percentage == 100 {
		return true, nil
	}

	bucket, err := RolloutBucket(feature, key)
	if err != nil {
		return false, err
	}
	return bucket < percentage, nil
}

// RolloutBucket returns a stable bucket in [0, 99] for feature and key.
func RolloutBucket(feature, key string) (int, error) {
	feature = strings.TrimSpace(feature)
	key = strings.TrimSpace(key)
	if feature == "" {
		return 0, fmt.Errorf("%w: feature is required", ErrInvalidRollout)
	}
	if key == "" {
		return 0, fmt.Errorf("%w: key is required", ErrInvalidRolloutTarget)
	}

	sum := sha256.Sum256([]byte(feature + "\x00" + key))
	return int(binary.BigEndian.Uint64(sum[:8]) % 100), nil
}

func (r Rollout) evaluationKey(target RolloutTarget) (string, error) {
	switch r.Key {
	case RolloutKeyTenant:
		if target.TenantKey == "" {
			return "", fmt.Errorf("%w: tenant key is required", ErrInvalidRolloutTarget)
		}
		return target.TenantKey, nil
	case RolloutKeyUser:
		if target.UserKey == "" {
			return "", fmt.Errorf("%w: user key is required", ErrInvalidRolloutTarget)
		}
		return target.UserKey, nil
	default:
		return "", fmt.Errorf("%w: key must be %q or %q", ErrInvalidRollout, RolloutKeyTenant, RolloutKeyUser)
	}
}

func (targets RolloutTargets) validate(name string) error {
	var errs []error
	errs = append(errs, validateRolloutTargetValues(name+".tenants", targets.Tenants)...)
	errs = append(errs, validateRolloutTargetValues(name+".users", targets.Users)...)
	return errors.Join(errs...)
}

func validateRolloutTargetValues(field string, values []string) []error {
	seen := make(map[string]int, len(values))
	var errs []error
	for i, raw := range values {
		value := strings.TrimSpace(raw)
		if value == "" {
			errs = append(errs, fmt.Errorf("%w: %s[%d] is empty", ErrInvalidRollout, field, i))
			continue
		}
		if first, ok := seen[value]; ok {
			errs = append(errs, fmt.Errorf("%w: %s[%d] duplicates %s[%d]", ErrInvalidRollout, field, i, field, first))
			continue
		}
		seen[value] = i
	}
	return errs
}

func (targets RolloutTargets) matches(target RolloutTarget) bool {
	return containsRolloutTarget(targets.Tenants, target.TenantKey) ||
		containsRolloutTarget(targets.Users, target.UserKey)
}

func containsRolloutTarget(values []string, key string) bool {
	key = strings.TrimSpace(key)
	if key == "" {
		return false
	}
	for _, value := range values {
		if strings.TrimSpace(value) == key {
			return true
		}
	}
	return false
}

func (target RolloutTarget) normalized() RolloutTarget {
	target.TenantKey = strings.TrimSpace(target.TenantKey)
	target.UserKey = strings.TrimSpace(target.UserKey)
	return target
}
