package storage

import (
	"errors"
	"fmt"
	"strings"
)

var (
	// ErrQuotaPolicyInvalid is returned when a quota rule has invalid limits or
	// duplicates another rule scope.
	ErrQuotaPolicyInvalid = errors.New("lazuli/storage: quota_policy_invalid")

	// ErrQuotaUsageInvalid is returned when usage snapshots or planned deltas
	// cannot produce a coherent non-negative byte total.
	ErrQuotaUsageInvalid = errors.New("lazuli/storage: quota_usage_invalid")

	// ErrQuotaHardLimitExceeded is returned by ValidateQuotaPlan when at least
	// one planned positive delta would exceed a hard byte limit.
	ErrQuotaHardLimitExceeded = errors.New("lazuli/storage: quota_hard_limit_exceeded")
)

const (
	maxInt64 = int64(1<<63 - 1)
	minInt64 = -1 << 63
)

// QuotaScope identifies the usage partition a policy rule, current usage row,
// or planned delta belongs to. Empty fields are wildcards on policy rules and
// unscoped dimensions on usage/delta rows.
type QuotaScope struct {
	Tenant string
	User   string
	Bucket string
}

// Normalize trims scope dimensions so callers may pass values read directly
// from request or config boundaries.
func (s QuotaScope) Normalize() QuotaScope {
	return QuotaScope{
		Tenant: strings.TrimSpace(s.Tenant),
		User:   strings.TrimSpace(s.User),
		Bucket: strings.TrimSpace(s.Bucket),
	}
}

// Matches reports whether scope, interpreted as a policy predicate, matches a
// concrete usage or delta scope. Empty predicate fields match any value.
func (s QuotaScope) Matches(target QuotaScope) bool {
	predicate := s.Normalize()
	target = target.Normalize()
	return quotaScopeFieldMatches(predicate.Tenant, target.Tenant) &&
		quotaScopeFieldMatches(predicate.User, target.User) &&
		quotaScopeFieldMatches(predicate.Bucket, target.Bucket)
}

// String renders the scope as stable label text for diagnostics and plan
// entries.
func (s QuotaScope) String() string {
	s = s.Normalize()
	parts := make([]string, 0, 3)
	if s.Tenant != "" {
		parts = append(parts, "tenant="+s.Tenant)
	}
	if s.User != "" {
		parts = append(parts, "user="+s.User)
	}
	if s.Bucket != "" {
		parts = append(parts, "bucket="+s.Bucket)
	}
	if len(parts) == 0 {
		return "global"
	}
	return strings.Join(parts, ",")
}

// QuotaLimit is the provider-neutral byte budget for one scope. Zero values
// disable the corresponding limit. Soft limits are warnings; hard limits block
// positive usage deltas that would cross them.
type QuotaLimit struct {
	SoftBytes int64
	HardBytes int64
}

// Validate checks the limit bounds.
func (l QuotaLimit) Validate() error {
	return validateQuotaLimit(l)
}

// QuotaPolicy is an ordered set of independent quota checks. A single upload
// delta may match multiple rules, e.g. tenant, user, and bucket rules, and all
// matching rules are included in the plan.
type QuotaPolicy struct {
	Rules []QuotaRule
}

// Validate checks that every rule is structurally valid.
func (p QuotaPolicy) Validate() error {
	return ValidateQuotaPolicy(p)
}

// QuotaRule binds a scope predicate to a soft/hard byte limit.
type QuotaRule struct {
	Name  string
	Scope QuotaScope
	Limit QuotaLimit
}

// Named returns a copy of the rule with a stable human-readable name for plan
// output.
func (r QuotaRule) Named(name string) QuotaRule {
	r.Name = name
	return r
}

// GlobalQuota returns a rule that evaluates all usage and deltas together.
func GlobalQuota(limit QuotaLimit) QuotaRule {
	return QuotaRule{Limit: limit}
}

// TenantQuota returns a rule scoped to one tenant.
func TenantQuota(tenant string, limit QuotaLimit) QuotaRule {
	return QuotaRule{
		Scope: QuotaScope{Tenant: tenant},
		Limit: limit,
	}
}

// UserQuota returns a rule scoped to one user. Tenant may be empty when user
// identifiers are globally unique.
func UserQuota(tenant, user string, limit QuotaLimit) QuotaRule {
	return QuotaRule{
		Scope: QuotaScope{Tenant: tenant, User: user},
		Limit: limit,
	}
}

// BucketQuota returns a rule scoped to one storage bucket. Tenant may be empty
// when the bucket name is global.
func BucketQuota(tenant, bucket string, limit QuotaLimit) QuotaRule {
	return QuotaRule{
		Scope: QuotaScope{Tenant: tenant, Bucket: bucket},
		Limit: limit,
	}
}

// QuotaUsage is the adapter-neutral current usage snapshot for a scope.
type QuotaUsage struct {
	Scope QuotaScope
	Bytes int64
}

// QuotaUsageDelta is a planned usage change for a scope. Positive bytes model
// uploads or copies; negative bytes model deletes or replacements.
type QuotaUsageDelta struct {
	Scope QuotaScope
	Bytes int64
}

// QuotaStatus is the selected limit class for a plan entry.
type QuotaStatus int

const (
	QuotaWithinLimit QuotaStatus = iota
	QuotaSoftExceeded
	QuotaHardExceeded
)

// String renders the status as a stable lowercase token.
func (s QuotaStatus) String() string {
	switch s {
	case QuotaWithinLimit:
		return "within_limit"
	case QuotaSoftExceeded:
		return "soft_limit_exceeded"
	case QuotaHardExceeded:
		return "hard_limit_exceeded"
	default:
		return "unknown"
	}
}

// QuotaPlan is the dry-run result for applying usage deltas to the current
// usage snapshot under a policy.
type QuotaPlan struct {
	DryRun  bool
	Allowed bool
	Entries []QuotaPlanEntry
}

// Validate returns ErrQuotaHardLimitExceeded when any entry blocks the plan.
func (p QuotaPlan) Validate() error {
	return ValidateQuotaPlan(p)
}

// SoftLimitExceeded reports whether any entry is above its soft limit.
func (p QuotaPlan) SoftLimitExceeded() bool {
	for _, entry := range p.Entries {
		if entry.Status == QuotaSoftExceeded || (entry.SoftBytes > 0 && entry.AfterBytes > entry.SoftBytes) {
			return true
		}
	}
	return false
}

// HardLimitExceeded reports whether any entry is above its hard limit,
// including entries that are allowed because the delta reduces usage.
func (p QuotaPlan) HardLimitExceeded() bool {
	for _, entry := range p.Entries {
		if entry.Status == QuotaHardExceeded {
			return true
		}
	}
	return false
}

// QuotaPlanEntry describes one evaluated rule or, when no rule matches, one
// unlimited delta scope.
type QuotaPlanEntry struct {
	Scope       QuotaScope
	RuleName    string
	BeforeBytes int64
	DeltaBytes  int64
	AfterBytes  int64
	SoftBytes   int64
	HardBytes   int64
	Status      QuotaStatus
	Allowed     bool
	Reason      string
}

// ValidateQuotaPolicy checks quota rules for structural validity.
func ValidateQuotaPolicy(policy QuotaPolicy) error {
	seen := make(map[QuotaScope]int, len(policy.Rules))
	for i, rule := range policy.Rules {
		if err := validateQuotaLimit(rule.Limit); err != nil {
			return fmt.Errorf("%w: rule %d: %v", ErrQuotaPolicyInvalid, i, err)
		}
		scope := rule.Scope.Normalize()
		if previous, ok := seen[scope]; ok {
			return fmt.Errorf("%w: rule %d duplicates rule %d scope %s", ErrQuotaPolicyInvalid, i, previous, scope)
		}
		seen[scope] = i
	}
	return nil
}

// ValidateQuotaUsage checks current usage snapshots for non-negative,
// aggregatable byte counts.
func ValidateQuotaUsage(usage []QuotaUsage) error {
	_, err := aggregateQuotaUsage(usage)
	return err
}

// ValidateQuotaUsageDeltas checks planned deltas for aggregatable byte counts.
func ValidateQuotaUsageDeltas(deltas []QuotaUsageDelta) error {
	_, _, err := aggregateQuotaDeltas(deltas)
	return err
}

// ValidateQuotaPlan checks whether a built plan may be applied. Soft-limit
// entries are allowed; hard-limit entries only fail when the planned delta
// increases usage beyond the hard limit.
func ValidateQuotaPlan(plan QuotaPlan) error {
	for _, entry := range plan.Entries {
		if !entry.Allowed {
			return fmt.Errorf("%w: scope %s would use %d bytes over hard limit %d", ErrQuotaHardLimitExceeded, entry.Scope, entry.AfterBytes, entry.HardBytes)
		}
	}
	return nil
}

// BuildQuotaPlan evaluates policy against current usage and planned deltas.
// It does not call ObjectStore or mutate any ledger; callers apply successful
// deltas in their own transaction after ValidateQuotaPlan passes.
func BuildQuotaPlan(policy QuotaPolicy, usage []QuotaUsage, deltas []QuotaUsageDelta) (QuotaPlan, error) {
	if err := ValidateQuotaPolicy(policy); err != nil {
		return QuotaPlan{}, err
	}

	usageByScope, err := aggregateQuotaUsage(usage)
	if err != nil {
		return QuotaPlan{}, err
	}
	deltaByScope, deltaOrder, err := aggregateQuotaDeltas(deltas)
	if err != nil {
		return QuotaPlan{}, err
	}

	plan := QuotaPlan{
		DryRun:  true,
		Allowed: true,
	}
	matchedDeltaScopes := make(map[QuotaScope]bool, len(deltaByScope))
	for _, rule := range policy.Rules {
		scope := rule.Scope.Normalize()
		before, err := sumQuotaUsageMatching(usageByScope, scope)
		if err != nil {
			return QuotaPlan{}, err
		}
		delta, matched, err := sumQuotaDeltasMatching(deltaByScope, scope, matchedDeltaScopes)
		if err != nil {
			return QuotaPlan{}, err
		}
		if !matched {
			continue
		}

		entry, err := buildQuotaPlanEntry(scope, rule.Name, rule.Limit, before, delta, "rule matched")
		if err != nil {
			return QuotaPlan{}, err
		}
		if !entry.Allowed {
			plan.Allowed = false
		}
		plan.Entries = append(plan.Entries, entry)
	}

	for _, scope := range deltaOrder {
		if matchedDeltaScopes[scope] {
			continue
		}
		entry, err := buildQuotaPlanEntry(scope, "", QuotaLimit{}, usageByScope[scope], deltaByScope[scope], "no matching quota rule")
		if err != nil {
			return QuotaPlan{}, err
		}
		plan.Entries = append(plan.Entries, entry)
	}
	return plan, nil
}

func validateQuotaLimit(limit QuotaLimit) error {
	if limit.SoftBytes < 0 {
		return fmt.Errorf("soft bytes must be non-negative")
	}
	if limit.HardBytes < 0 {
		return fmt.Errorf("hard bytes must be non-negative")
	}
	if limit.SoftBytes > 0 && limit.HardBytes > 0 && limit.SoftBytes > limit.HardBytes {
		return fmt.Errorf("soft bytes must not exceed hard bytes")
	}
	return nil
}

func aggregateQuotaUsage(usage []QuotaUsage) (map[QuotaScope]int64, error) {
	usageByScope := make(map[QuotaScope]int64, len(usage))
	for i, row := range usage {
		if row.Bytes < 0 {
			return nil, fmt.Errorf("%w: usage %d has negative bytes", ErrQuotaUsageInvalid, i)
		}
		scope := row.Scope.Normalize()
		total, err := checkedQuotaAdd(usageByScope[scope], row.Bytes)
		if err != nil {
			return nil, fmt.Errorf("%w: usage %d overflows byte total", ErrQuotaUsageInvalid, i)
		}
		usageByScope[scope] = total
	}
	return usageByScope, nil
}

func aggregateQuotaDeltas(deltas []QuotaUsageDelta) (map[QuotaScope]int64, []QuotaScope, error) {
	deltaByScope := make(map[QuotaScope]int64, len(deltas))
	order := make([]QuotaScope, 0, len(deltas))
	for i, delta := range deltas {
		scope := delta.Scope.Normalize()
		if _, ok := deltaByScope[scope]; !ok {
			order = append(order, scope)
		}
		total, err := checkedQuotaAdd(deltaByScope[scope], delta.Bytes)
		if err != nil {
			return nil, nil, fmt.Errorf("%w: delta %d overflows byte total", ErrQuotaUsageInvalid, i)
		}
		deltaByScope[scope] = total
	}
	return deltaByScope, order, nil
}

func sumQuotaUsageMatching(usageByScope map[QuotaScope]int64, ruleScope QuotaScope) (int64, error) {
	var total int64
	for scope, bytes := range usageByScope {
		if !ruleScope.Matches(scope) {
			continue
		}
		next, err := checkedQuotaAdd(total, bytes)
		if err != nil {
			return 0, err
		}
		total = next
	}
	return total, nil
}

func sumQuotaDeltasMatching(deltaByScope map[QuotaScope]int64, ruleScope QuotaScope, matched map[QuotaScope]bool) (int64, bool, error) {
	var total int64
	var ok bool
	for scope, bytes := range deltaByScope {
		if !ruleScope.Matches(scope) {
			continue
		}
		next, err := checkedQuotaAdd(total, bytes)
		if err != nil {
			return 0, false, err
		}
		total = next
		ok = true
		matched[scope] = true
	}
	return total, ok, nil
}

func buildQuotaPlanEntry(scope QuotaScope, ruleName string, limit QuotaLimit, before, delta int64, reason string) (QuotaPlanEntry, error) {
	after, err := checkedQuotaAdd(before, delta)
	if err != nil {
		return QuotaPlanEntry{}, fmt.Errorf("%w: scope %s overflows byte total", ErrQuotaUsageInvalid, scope)
	}
	if after < 0 {
		return QuotaPlanEntry{}, fmt.Errorf("%w: scope %s would have negative bytes", ErrQuotaUsageInvalid, scope)
	}

	entry := QuotaPlanEntry{
		Scope:       scope.Normalize(),
		RuleName:    ruleName,
		BeforeBytes: before,
		DeltaBytes:  delta,
		AfterBytes:  after,
		SoftBytes:   limit.SoftBytes,
		HardBytes:   limit.HardBytes,
		Status:      QuotaWithinLimit,
		Allowed:     true,
		Reason:      reason,
	}
	if limit.HardBytes > 0 && after > limit.HardBytes {
		entry.Status = QuotaHardExceeded
		entry.Reason = "hard limit exceeded"
		entry.Allowed = delta <= 0
		return entry, nil
	}
	if limit.SoftBytes > 0 && after > limit.SoftBytes {
		entry.Status = QuotaSoftExceeded
		entry.Reason = "soft limit exceeded"
	}
	return entry, nil
}

func checkedQuotaAdd(a, b int64) (int64, error) {
	if b > 0 && a > maxInt64-b {
		return 0, ErrQuotaUsageInvalid
	}
	if b < 0 && a < minInt64-b {
		return 0, ErrQuotaUsageInvalid
	}
	return a + b, nil
}

func quotaScopeFieldMatches(predicate, target string) bool {
	return predicate == "" || predicate == target
}
