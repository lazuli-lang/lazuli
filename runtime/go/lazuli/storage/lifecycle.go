package storage

import (
	"errors"
	"fmt"
	"time"
)

var (
	// ErrLifecyclePolicyInvalid is returned when a lifecycle rule contains an
	// unknown transition, invalid age predicate, or invalid visibility filter.
	ErrLifecyclePolicyInvalid = errors.New("lazuli/storage: lifecycle_policy_invalid")

	// ErrLifecycleObjectInvalid is returned when dry-run planning receives an
	// object snapshot that cannot be addressed or classified.
	ErrLifecycleObjectInvalid = errors.New("lazuli/storage: lifecycle_object_invalid")
)

// LifecycleTransition is the provider-neutral action selected by a storage
// lifecycle policy. Retain is an explicit no-op transition used for exceptions
// and dry-run output; Delete and Archive are applied by adapter-specific
// executors outside this package.
type LifecycleTransition int

const (
	LifecycleRetain LifecycleTransition = iota
	LifecycleDelete
	LifecycleArchive
)

// String renders the transition as the stable lowercase token used in plans.
func (t LifecycleTransition) String() string {
	switch t {
	case LifecycleRetain:
		return "retain"
	case LifecycleDelete:
		return "delete"
	case LifecycleArchive:
		return "archive"
	default:
		return "unknown"
	}
}

// LifecyclePolicy is an ordered list of rules. BuildLifecyclePlan evaluates
// rules in slice order and uses the first rule whose age and visibility
// predicates match an object.
type LifecyclePolicy struct {
	Rules []LifecycleRule
}

// Validate checks that all policy rules use known provider-neutral tokens.
func (p LifecyclePolicy) Validate() error {
	return ValidateLifecyclePolicy(p)
}

// LifecycleRule binds an age predicate and optional visibility predicate to a
// lifecycle transition. An empty Visibility list matches every visibility.
type LifecycleRule struct {
	Name       string
	Transition LifecycleTransition
	Age        time.Duration
	Visibility []FileVisibility
}

// RetainAfter returns a lifecycle rule that explicitly retains matching
// objects once the age predicate is met.
func RetainAfter(age time.Duration, visibility ...FileVisibility) LifecycleRule {
	return lifecycleRule(LifecycleRetain, age, visibility...)
}

// DeleteAfter returns a lifecycle rule that deletes matching objects once the
// age predicate is met.
func DeleteAfter(age time.Duration, visibility ...FileVisibility) LifecycleRule {
	return lifecycleRule(LifecycleDelete, age, visibility...)
}

// ArchiveAfter returns a lifecycle rule that archives matching objects once the
// age predicate is met.
func ArchiveAfter(age time.Duration, visibility ...FileVisibility) LifecycleRule {
	return lifecycleRule(LifecycleArchive, age, visibility...)
}

// Named returns a copy of the rule with a stable human-readable name for dry-run
// plan output.
func (r LifecycleRule) Named(name string) LifecycleRule {
	r.Name = name
	return r
}

// LifecycleObject is the adapter-neutral metadata snapshot used to evaluate a
// lifecycle policy without touching object bytes.
type LifecycleObject struct {
	Key          Key
	CreatedAt    time.Time
	LastModified time.Time
	Visibility   FileVisibility
	ContentType  string
	Size         int64
}

// LifecyclePlan is the dry-run result for a policy evaluation. It intentionally
// contains only provider-neutral transitions; applying Archive/Delete belongs to
// the bound adapter or job runner.
type LifecyclePlan struct {
	DryRun      bool
	GeneratedAt time.Time
	Entries     []LifecyclePlanEntry
}

// LifecyclePlanEntry describes the selected transition for one object.
type LifecyclePlanEntry struct {
	Key        Key
	Transition LifecycleTransition
	RuleName   string
	Visibility FileVisibility
	Age        time.Duration
	EligibleAt time.Time
	Reason     string
}

// ValidateLifecyclePolicy checks lifecycle rules for structural validity.
func ValidateLifecyclePolicy(policy LifecyclePolicy) error {
	for i, rule := range policy.Rules {
		if !isKnownLifecycleTransition(rule.Transition) {
			return fmt.Errorf("%w: rule %d has unknown transition %q", ErrLifecyclePolicyInvalid, i, rule.Transition)
		}
		if rule.Age < 0 {
			return fmt.Errorf("%w: rule %d has negative age %s", ErrLifecyclePolicyInvalid, i, rule.Age)
		}
		for _, visibility := range rule.Visibility {
			if !isKnownFileVisibility(visibility) {
				return fmt.Errorf("%w: rule %d has unknown visibility %q", ErrLifecyclePolicyInvalid, i, visibility)
			}
		}
	}
	return nil
}

// ValidateLifecycleObjects checks the object snapshots required for dry-run
// lifecycle planning.
func ValidateLifecycleObjects(objects []LifecycleObject) error {
	for i, object := range objects {
		if object.Key == "" {
			return fmt.Errorf("%w: object %d has empty key", ErrLifecycleObjectInvalid, i)
		}
		if !isKnownFileVisibility(object.Visibility) {
			return fmt.Errorf("%w: object %d has unknown visibility %q", ErrLifecycleObjectInvalid, i, object.Visibility)
		}
	}
	return nil
}

// LifecycleAgeAt returns the object's age at now. CreatedAt is preferred; when
// it is absent, LastModified is used. Objects with unknown or future timestamps
// have age zero.
func LifecycleAgeAt(object LifecycleObject, now time.Time) time.Duration {
	stamp := object.lifecycleTimestamp()
	if stamp.IsZero() || now.Before(stamp) {
		return 0
	}
	return now.Sub(stamp)
}

// LifecycleAgeAtLeast reports whether object satisfies an age predicate at now.
// A non-positive age predicate matches immediately.
func LifecycleAgeAtLeast(object LifecycleObject, now time.Time, age time.Duration) bool {
	if age <= 0 {
		return true
	}
	return LifecycleAgeAt(object, now) >= age
}

// LifecycleVisibilityMatches reports whether object satisfies a visibility
// predicate. An empty predicate matches every known visibility.
func LifecycleVisibilityMatches(object LifecycleObject, visibility ...FileVisibility) bool {
	if len(visibility) == 0 {
		return isKnownFileVisibility(object.Visibility)
	}
	for _, candidate := range visibility {
		if object.Visibility == candidate {
			return true
		}
	}
	return false
}

// BuildLifecyclePlan evaluates policy against objects and returns a dry-run
// plan. It does not call ObjectStore, so adapters remain responsible for
// executing delete/archive transitions after the caller reviews the plan.
func BuildLifecyclePlan(policy LifecyclePolicy, objects []LifecycleObject, now time.Time) (LifecyclePlan, error) {
	if err := ValidateLifecyclePolicy(policy); err != nil {
		return LifecyclePlan{}, err
	}
	if err := ValidateLifecycleObjects(objects); err != nil {
		return LifecyclePlan{}, err
	}
	if now.IsZero() {
		now = time.Now()
	}

	plan := LifecyclePlan{
		DryRun:      true,
		GeneratedAt: now,
		Entries:     make([]LifecyclePlanEntry, 0, len(objects)),
	}
	for _, object := range objects {
		plan.Entries = append(plan.Entries, lifecyclePlanEntry(policy, object, now))
	}
	return plan, nil
}

func lifecycleRule(transition LifecycleTransition, age time.Duration, visibility ...FileVisibility) LifecycleRule {
	rule := LifecycleRule{
		Transition: transition,
		Age:        age,
	}
	if len(visibility) > 0 {
		rule.Visibility = append([]FileVisibility(nil), visibility...)
	}
	return rule
}

func lifecyclePlanEntry(policy LifecyclePolicy, object LifecycleObject, now time.Time) LifecyclePlanEntry {
	entry := LifecyclePlanEntry{
		Key:        object.Key,
		Transition: LifecycleRetain,
		Visibility: object.Visibility,
		Age:        LifecycleAgeAt(object, now),
		Reason:     "no matching lifecycle rule",
	}

	var (
		nextRule LifecycleRule
		nextDue  time.Time
		hasNext  bool
	)
	for _, rule := range policy.Rules {
		if !LifecycleVisibilityMatches(object, rule.Visibility...) {
			continue
		}
		if LifecycleAgeAtLeast(object, now, rule.Age) {
			entry.Transition = rule.Transition
			entry.RuleName = rule.Name
			entry.EligibleAt = lifecycleEligibleAt(object, rule)
			entry.Reason = "rule matched"
			return entry
		}

		due := lifecycleEligibleAt(object, rule)
		if due.IsZero() {
			continue
		}
		if !hasNext || due.Before(nextDue) {
			nextRule = rule
			nextDue = due
			hasNext = true
		}
	}

	if hasNext {
		entry.RuleName = nextRule.Name
		entry.EligibleAt = nextDue
		entry.Reason = "age predicate not met"
	}
	return entry
}

func lifecycleEligibleAt(object LifecycleObject, rule LifecycleRule) time.Time {
	if rule.Age <= 0 {
		return object.lifecycleTimestamp()
	}
	stamp := object.lifecycleTimestamp()
	if stamp.IsZero() {
		return time.Time{}
	}
	return stamp.Add(rule.Age)
}

func (o LifecycleObject) lifecycleTimestamp() time.Time {
	if !o.CreatedAt.IsZero() {
		return o.CreatedAt
	}
	return o.LastModified
}

func isKnownLifecycleTransition(transition LifecycleTransition) bool {
	switch transition {
	case LifecycleRetain, LifecycleDelete, LifecycleArchive:
		return true
	default:
		return false
	}
}

func isKnownFileVisibility(visibility FileVisibility) bool {
	switch visibility {
	case VisibilityPrivate, VisibilityPublic, VisibilitySigned:
		return true
	default:
		return false
	}
}
