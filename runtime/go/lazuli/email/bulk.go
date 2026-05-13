package email

import (
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"fmt"
	"sort"
	"strings"
)

const (
	// DefaultBulkBatchSize is used when BulkPlanOptions.BatchSize is zero.
	DefaultBulkBatchSize = 1000

	defaultBulkIdempotencyNamespace = "email.bulk"
	bulkSuppressionReasonDefault    = "unsubscribed"
	bulkIdempotencyKeyPrefix        = "bulk_email:"
)

var (
	// ErrInvalidBulkPlan is returned for invalid bulk email planning inputs.
	ErrInvalidBulkPlan = errors.New("email: invalid bulk plan")
)

// BulkRecipient is one intended bulk email recipient.
type BulkRecipient struct {
	// Address is the recipient mailbox.
	Address Address
	// SubscriberID is an optional stable application identifier included in
	// idempotency keys.
	SubscriberID string
}

// BulkSuppression marks a recipient mailbox as unsubscribed for a list.
type BulkSuppression struct {
	// Email is the suppressed addr-spec.
	Email string
	// ListID scopes suppression. Empty applies to all lists.
	ListID string
	// Reason describes why the recipient was suppressed. Empty defaults to
	// "unsubscribed".
	Reason string
}

// BulkDomainThrottle caps recipients from one domain in each batch.
type BulkDomainThrottle struct {
	// MaxRecipientsPerBatch is the maximum recipients from this domain allowed
	// in one planned batch.
	MaxRecipientsPerBatch int
}

// BulkIdempotencyScope identifies the logical bulk send for deterministic
// recipient idempotency keys.
type BulkIdempotencyScope struct {
	// Namespace separates keys across applications or dispatch pipelines.
	Namespace string
	// CampaignID identifies this bulk send.
	CampaignID string
	// ListID identifies the mailing or notification list.
	ListID string
}

// BulkPlanOptions configures provider-neutral bulk email planning.
type BulkPlanOptions struct {
	// BatchSize caps total recipients in each batch. Zero uses
	// DefaultBulkBatchSize.
	BatchSize int
	// CampaignID identifies this bulk send for idempotency keys.
	CampaignID string
	// ListID identifies the mailing or notification list for suppression and
	// idempotency keys.
	ListID string
	// IdempotencyNamespace separates keys across applications or dispatch
	// pipelines. Empty uses a stable package default.
	IdempotencyNamespace string
	// DomainThrottles caps recipients per domain in each batch.
	DomainThrottles map[string]BulkDomainThrottle
	// Suppressions are unsubscribed recipients to remove from the plan.
	Suppressions []BulkSuppression
}

// BulkPlan is a dry-run plan. It does not send email or mutate storage.
type BulkPlan struct {
	DryRun     bool
	Batches    []BulkBatch
	Suppressed []BulkSuppressedRecipient
	Summary    BulkDryRunSummary
}

// BulkBatch is one planned group of recipients.
type BulkBatch struct {
	// Index is zero-based.
	Index int
	// Recipients are planned in input order, minus suppressed recipients.
	Recipients []BulkPlannedRecipient
	// DomainCounts records recipients by lower-cased recipient domain.
	DomainCounts map[string]int
}

// BulkPlannedRecipient is a recipient selected for a planned batch.
type BulkPlannedRecipient struct {
	Recipient      BulkRecipient
	Domain         string
	IdempotencyKey string
}

// BulkSuppressedRecipient is a recipient skipped because of suppression.
type BulkSuppressedRecipient struct {
	Recipient BulkRecipient
	Domain    string
	Reason    string
}

// BulkDryRunSummary is the aggregate summary for a bulk email plan.
type BulkDryRunSummary struct {
	TotalRecipients      int
	PlannedRecipients    int
	SuppressedRecipients int
	BatchCount           int
	Domains              []BulkDomainSummary
}

// BulkDomainSummary summarizes planned and suppressed recipients for one
// lower-cased mailbox domain.
type BulkDomainSummary struct {
	Domain                string
	PlannedRecipients     int
	SuppressedRecipients  int
	BatchCount            int
	MaxRecipientsPerBatch int
}

type bulkSuppressionEntry struct {
	reason string
}

type bulkDomainStats struct {
	plannedRecipients    int
	suppressedRecipients int
	batches              map[int]struct{}
	maxPerBatch          int
}

// BuildBulkPlan batches recipients, applies per-domain throttles, suppresses
// unsubscribed recipients, and attaches deterministic idempotency keys. It
// does not send email.
func BuildBulkPlan(recipients []BulkRecipient, opts BulkPlanOptions) (BulkPlan, error) {
	opts, err := normalizeBulkPlanOptions(opts)
	if err != nil {
		return BulkPlan{}, err
	}
	if len(recipients) == 0 {
		return BulkPlan{}, bulkPlanInvalidf("recipients are required")
	}

	throttles, err := normalizeBulkDomainThrottles(opts.DomainThrottles)
	if err != nil {
		return BulkPlan{}, err
	}
	suppressions, err := normalizeBulkSuppressions(opts.Suppressions, opts.ListID)
	if err != nil {
		return BulkPlan{}, err
	}

	scope := BulkIdempotencyScope{
		Namespace:  opts.IdempotencyNamespace,
		CampaignID: opts.CampaignID,
		ListID:     opts.ListID,
	}
	plan := BulkPlan{DryRun: true}
	stats := make(map[string]*bulkDomainStats)
	current := newBulkBatch(0)

	for i, raw := range recipients {
		recipient, canonicalEmail, domain, err := normalizeBulkRecipient(raw)
		if err != nil {
			return BulkPlan{}, fieldError(fmt.Sprintf("recipients[%d]", i), err)
		}

		domainStats := ensureBulkDomainStats(stats, domain, throttles[domain].MaxRecipientsPerBatch)
		if suppression, ok := suppressions[canonicalEmail]; ok {
			plan.Suppressed = append(plan.Suppressed, BulkSuppressedRecipient{
				Recipient: recipient,
				Domain:    domain,
				Reason:    suppression.reason,
			})
			domainStats.suppressedRecipients++
			continue
		}

		planned, err := newBulkPlannedRecipient(scope, recipient, domain)
		if err != nil {
			return BulkPlan{}, fieldError(fmt.Sprintf("recipients[%d]", i), err)
		}
		if !canAddBulkRecipient(current, opts.BatchSize, domain, throttles[domain]) {
			plan.Batches = append(plan.Batches, current)
			current = newBulkBatch(len(plan.Batches))
		}

		current.Recipients = append(current.Recipients, planned)
		current.DomainCounts[domain]++
		domainStats.plannedRecipients++
		domainStats.batches[current.Index] = struct{}{}
	}

	if len(current.Recipients) > 0 {
		plan.Batches = append(plan.Batches, current)
	}
	plan.Summary = buildBulkDryRunSummary(len(recipients), plan, stats)
	return plan, nil
}

// BulkIdempotencyKey returns a deterministic key for a recipient in a bulk
// idempotency scope.
func BulkIdempotencyKey(scope BulkIdempotencyScope, recipient BulkRecipient) (string, error) {
	scope, err := normalizeBulkIdempotencyScope(scope)
	if err != nil {
		return "", err
	}
	recipient, canonicalEmail, _, err := normalizeBulkRecipient(recipient)
	if err != nil {
		return "", err
	}

	sum := sha256.Sum256([]byte(strings.Join([]string{
		"v1",
		scope.Namespace,
		scope.CampaignID,
		scope.ListID,
		canonicalEmail,
		recipient.SubscriberID,
	}, "\x00")))
	return bulkIdempotencyKeyPrefix + hex.EncodeToString(sum[:]), nil
}

func normalizeBulkPlanOptions(opts BulkPlanOptions) (BulkPlanOptions, error) {
	if opts.BatchSize == 0 {
		opts.BatchSize = DefaultBulkBatchSize
	}
	if opts.BatchSize < 0 {
		return BulkPlanOptions{}, bulkPlanInvalidf("batch size must be positive")
	}
	scope, err := normalizeBulkIdempotencyScope(BulkIdempotencyScope{
		Namespace:  opts.IdempotencyNamespace,
		CampaignID: opts.CampaignID,
		ListID:     opts.ListID,
	})
	if err != nil {
		return BulkPlanOptions{}, err
	}
	opts.IdempotencyNamespace = scope.Namespace
	opts.CampaignID = scope.CampaignID
	opts.ListID = scope.ListID
	return opts, nil
}

func normalizeBulkIdempotencyScope(scope BulkIdempotencyScope) (BulkIdempotencyScope, error) {
	scope.Namespace = strings.TrimSpace(scope.Namespace)
	if scope.Namespace == "" {
		scope.Namespace = defaultBulkIdempotencyNamespace
	}
	if err := validateBulkToken("idempotency namespace", scope.Namespace); err != nil {
		return BulkIdempotencyScope{}, err
	}
	if err := validateBulkOptionalToken("campaign id", scope.CampaignID); err != nil {
		return BulkIdempotencyScope{}, err
	}
	if err := validateBulkOptionalToken("list id", scope.ListID); err != nil {
		return BulkIdempotencyScope{}, err
	}
	return scope, nil
}

func normalizeBulkDomainThrottles(throttles map[string]BulkDomainThrottle) (map[string]BulkDomainThrottle, error) {
	if len(throttles) == 0 {
		return nil, nil
	}

	normalized := make(map[string]BulkDomainThrottle, len(throttles))
	for rawDomain, throttle := range throttles {
		domain, err := normalizeBulkDomain(rawDomain)
		if err != nil {
			return nil, fieldError(fmt.Sprintf("domain_throttles[%q]", rawDomain), err)
		}
		if throttle.MaxRecipientsPerBatch < 1 {
			return nil, fieldError(fmt.Sprintf("domain_throttles[%q].max_recipients_per_batch", rawDomain), bulkPlanInvalidf("must be positive"))
		}
		if _, exists := normalized[domain]; exists {
			return nil, fieldError(fmt.Sprintf("domain_throttles[%q]", rawDomain), bulkPlanInvalidf("duplicate normalized domain %q", domain))
		}
		normalized[domain] = throttle
	}
	return normalized, nil
}

func normalizeBulkSuppressions(suppressions []BulkSuppression, planListID string) (map[string]bulkSuppressionEntry, error) {
	if len(suppressions) == 0 {
		return nil, nil
	}

	normalized := make(map[string]bulkSuppressionEntry, len(suppressions))
	for i, suppression := range suppressions {
		email, _, err := normalizeBulkEmail(suppression.Email)
		if err != nil {
			return nil, fieldError(fmt.Sprintf("suppressions[%d].email", i), err)
		}

		listID := strings.TrimSpace(suppression.ListID)
		if listID != suppression.ListID {
			return nil, fieldError(fmt.Sprintf("suppressions[%d].list_id", i), bulkPlanInvalidf("has surrounding whitespace"))
		}
		if err := validateBulkOptionalToken("suppression list id", listID); err != nil {
			return nil, fieldError(fmt.Sprintf("suppressions[%d].list_id", i), err)
		}
		if listID != "" && listID != planListID {
			continue
		}

		reason := strings.TrimSpace(suppression.Reason)
		if reason == "" {
			reason = bulkSuppressionReasonDefault
		}
		if reason != suppression.Reason && suppression.Reason != "" {
			return nil, fieldError(fmt.Sprintf("suppressions[%d].reason", i), bulkPlanInvalidf("has surrounding whitespace"))
		}
		if containsControl(reason) {
			return nil, fieldError(fmt.Sprintf("suppressions[%d].reason", i), bulkPlanInvalidf("contains control characters"))
		}
		if _, exists := normalized[email]; !exists {
			normalized[email] = bulkSuppressionEntry{reason: reason}
		}
	}
	return normalized, nil
}

func newBulkPlannedRecipient(scope BulkIdempotencyScope, recipient BulkRecipient, domain string) (BulkPlannedRecipient, error) {
	key, err := BulkIdempotencyKey(scope, recipient)
	if err != nil {
		return BulkPlannedRecipient{}, err
	}
	return BulkPlannedRecipient{
		Recipient:      recipient,
		Domain:         domain,
		IdempotencyKey: key,
	}, nil
}

func normalizeBulkRecipient(raw BulkRecipient) (BulkRecipient, string, string, error) {
	recipient := raw
	if err := ValidateAddress(recipient.Address); err != nil {
		return BulkRecipient{}, "", "", fmt.Errorf("%w: %v", ErrInvalidBulkPlan, err)
	}
	if err := validateBulkOptionalToken("subscriber id", recipient.SubscriberID); err != nil {
		return BulkRecipient{}, "", "", err
	}
	email, domain, err := normalizeBulkEmail(recipient.Address.Email)
	if err != nil {
		return BulkRecipient{}, "", "", err
	}
	return recipient, email, domain, nil
}

func normalizeBulkEmail(raw string) (string, string, error) {
	address := Address{Email: strings.TrimSpace(raw)}
	if err := ValidateAddress(address); err != nil {
		return "", "", fmt.Errorf("%w: %v", ErrInvalidBulkPlan, err)
	}
	canonical := strings.ToLower(address.Email)
	domain := bulkEmailDomain(canonical)
	if domain == "" {
		return "", "", bulkPlanInvalidf("address email requires a domain")
	}
	return canonical, domain, nil
}

func normalizeBulkDomain(raw string) (string, error) {
	domain := strings.ToLower(strings.TrimSpace(raw))
	if domain == "" {
		return "", bulkPlanInvalidf("domain is required")
	}
	if domain != raw && strings.TrimSpace(raw) != raw {
		return "", bulkPlanInvalidf("domain has surrounding whitespace")
	}
	if containsControl(domain) || containsWhitespace(domain) || strings.ContainsAny(domain, "@<>[],") {
		return "", bulkPlanInvalidf("domain contains invalid characters")
	}
	return domain, nil
}

func canAddBulkRecipient(batch BulkBatch, batchSize int, domain string, throttle BulkDomainThrottle) bool {
	if len(batch.Recipients) == 0 {
		return true
	}
	if len(batch.Recipients) >= batchSize {
		return false
	}
	if throttle.MaxRecipientsPerBatch > 0 && batch.DomainCounts[domain] >= throttle.MaxRecipientsPerBatch {
		return false
	}
	return true
}

func newBulkBatch(index int) BulkBatch {
	return BulkBatch{
		Index:        index,
		DomainCounts: make(map[string]int),
	}
}

func buildBulkDryRunSummary(total int, plan BulkPlan, stats map[string]*bulkDomainStats) BulkDryRunSummary {
	summary := BulkDryRunSummary{
		TotalRecipients:      total,
		SuppressedRecipients: len(plan.Suppressed),
		BatchCount:           len(plan.Batches),
	}
	for _, batch := range plan.Batches {
		summary.PlannedRecipients += len(batch.Recipients)
	}

	domains := make([]string, 0, len(stats))
	for domain := range stats {
		domains = append(domains, domain)
	}
	sort.Strings(domains)

	summary.Domains = make([]BulkDomainSummary, 0, len(domains))
	for _, domain := range domains {
		stat := stats[domain]
		summary.Domains = append(summary.Domains, BulkDomainSummary{
			Domain:                domain,
			PlannedRecipients:     stat.plannedRecipients,
			SuppressedRecipients:  stat.suppressedRecipients,
			BatchCount:            len(stat.batches),
			MaxRecipientsPerBatch: stat.maxPerBatch,
		})
	}
	return summary
}

func ensureBulkDomainStats(stats map[string]*bulkDomainStats, domain string, maxPerBatch int) *bulkDomainStats {
	stat, ok := stats[domain]
	if ok {
		if stat.maxPerBatch == 0 {
			stat.maxPerBatch = maxPerBatch
		}
		return stat
	}
	stat = &bulkDomainStats{
		batches:     make(map[int]struct{}),
		maxPerBatch: maxPerBatch,
	}
	stats[domain] = stat
	return stat
}

func bulkEmailDomain(email string) string {
	at := strings.LastIndex(email, "@")
	if at < 0 || at == len(email)-1 {
		return ""
	}
	return email[at+1:]
}

func validateBulkOptionalToken(field, value string) error {
	if value == "" {
		return nil
	}
	if strings.TrimSpace(value) != value {
		return bulkPlanInvalidf("%s has surrounding whitespace", field)
	}
	return validateBulkToken(field, value)
}

func validateBulkToken(field, value string) error {
	if strings.TrimSpace(value) == "" {
		return bulkPlanInvalidf("%s is required", field)
	}
	if containsControl(value) {
		return bulkPlanInvalidf("%s contains control characters", field)
	}
	return nil
}

func bulkPlanInvalidf(format string, args ...any) error {
	return fmt.Errorf("%w: "+format, append([]any{ErrInvalidBulkPlan}, args...)...)
}
