package notifications

import (
	"errors"
	"fmt"
	"sort"
	"strings"
	"time"
)

// ErrBulkPlanInvalid is returned when bulk planning input cannot produce a
// deterministic dry-run plan.
var ErrBulkPlanInvalid = errors.New("notifications: bulk plan invalid")

// BulkPlanItemKind names the type of send unit represented in a channel batch.
type BulkPlanItemKind string

const (
	// BulkPlanItemDirect is a single notification channel dispatch.
	BulkPlanItemDirect BulkPlanItemKind = "direct"
	// BulkPlanItemDigest is a digest batch that should be rendered once for the
	// recipient/channel.
	BulkPlanItemDigest BulkPlanItemKind = "digest"
)

// BulkNotification is one notification trigger to include in a bulk dry-run.
// The planner expands it across Contract.Channels unless Envelope.Channel is
// already set.
type BulkNotification struct {
	Contract   NotificationContract
	Envelope   Envelope
	ReceivedAt time.Time
}

// PendingBulkNotification is a compatibility alias for callers that model
// bulk planning from queued notification triggers.
type PendingBulkNotification = BulkNotification

// BulkPlanner configures dry-run planning for bulk notification work.
type BulkPlanner struct {
	// MaxBatchSize caps items in each channel/rate-window batch. Zero means no
	// process-level cap.
	MaxBatchSize int
	// Suppressions skip matching recipient/channel pairs before digesting and
	// batching.
	Suppressions []BulkSuppression
	// DigestPlanner controls digest batch splitting. Its MaxBatchSize composes
	// with each contract's digest MaxSize.
	DigestPlanner DigestPlanner
	// Now is the fixed point used for rate windows and zero ReceivedAt values.
	// Zero uses time.Now.
	Now time.Time
}

// BulkSuppression skips one recipient, optionally scoped to a channel or
// notification. Empty Channel or Notification fields act as wildcards.
type BulkSuppression struct {
	Recipient    string
	Channel      Channel
	Notification string
	Reason       string
}

// BulkPlan is a provider-neutral dry-run plan. It never dispatches messages;
// concrete adapters can apply Batches later.
type BulkPlan struct {
	DryRun      bool
	GeneratedAt time.Time
	Summary     BulkPlanSummary
	Batches     []BulkChannelBatch
	DigestPlans []BulkDigestPlan
	Suppressed  []BulkSuppressedRecipient
}

// BulkPlanSummary reports compact dry-run counts.
type BulkPlanSummary struct {
	InputCount        int
	PlannedCount      int
	DirectCount       int
	DigestCount       int
	DigestSourceCount int
	SuppressedCount   int
	BatchCount        int
	RateWindowCount   int
	RateDeferredCount int
	Channels          []BulkChannelSummary
}

// BulkChannelSummary reports planned send units and batches for one channel.
type BulkChannelSummary struct {
	Channel         Channel
	PlannedCount    int
	BatchCount      int
	RateWindowCount int
}

// BulkChannelBatch is one channel-local batch. When WindowStart is non-zero,
// every item in the batch belongs to that planned rate window.
type BulkChannelBatch struct {
	Channel     Channel
	WindowStart time.Time
	WindowEnd   time.Time
	BatchIndex  int
	BatchCount  int
	Items       []BulkPlanItem
}

// BulkPlanItem is one dry-run send unit inside a channel batch.
type BulkPlanItem struct {
	Kind          BulkPlanItemKind
	Notification  string
	Notifications []string
	ID            string
	Tenant        string
	Recipient     string
	Channel       Channel
	Template      string
	Payload       map[string]any
	TemplateData  map[string]any
	ReceivedAt    time.Time

	DigestWindowStart time.Time
	DigestWindowEnd   time.Time
	DigestBatchIndex  int
	DigestBatchCount  int
	DigestItemCount   int

	RateWindow BulkRateWindow
}

// BulkRateWindow is the fixed throttle window selected for one planned send
// unit. Notification, Recipient, and Channel are the throttle bucket key, so
// Recipient or Channel may be empty when the contract throttle is not scoped to
// that axis.
type BulkRateWindow struct {
	Notification string
	Recipient    string
	Channel      Channel
	WindowStart  time.Time
	WindowEnd    time.Time
	WindowIndex  int
	Limit        int
	Position     int
	Deferred     bool
}

// BulkDigestPlan is one digest planner output scoped to the channel that will
// receive the rendered digest.
type BulkDigestPlan struct {
	Channel Channel
	DigestPlan
}

// BulkSuppressedRecipient records a skipped recipient/channel send unit.
type BulkSuppressedRecipient struct {
	Notification string
	Recipient    string
	Channel      Channel
	ID           string
	Reason       string
}

// PlanBulkNotifications builds a dry-run plan using the default BulkPlanner.
func PlanBulkNotifications(pending []BulkNotification) (BulkPlan, error) {
	return BulkPlanner{}.Plan(pending)
}

// PlanPendingBulkNotifications is an alias for PlanBulkNotifications.
func PlanPendingBulkNotifications(pending []PendingBulkNotification) (BulkPlan, error) {
	return BulkPlanner{}.Plan(pending)
}

// Plan expands pending notifications into channel batches, suppression skips,
// digest plans, rate windows, and summary counts. It does not send.
func (p BulkPlanner) Plan(pending []BulkNotification) (BulkPlan, error) {
	if p.MaxBatchSize < 0 {
		return BulkPlan{}, fmt.Errorf("%w: MaxBatchSize must be non-negative", ErrBulkPlanInvalid)
	}

	now := bulkPlanNow(p.Now)
	plan := BulkPlan{
		DryRun:      true,
		GeneratedAt: now,
		Summary: BulkPlanSummary{
			InputCount: len(pending),
		},
	}

	var sendItems []bulkSendWork
	digestGroups := make(map[bulkDigestGroupKey]*bulkDigestGroup)
	for i, next := range pending {
		base, channels, err := normalizeBulkNotification(next, now, i)
		if err != nil {
			return BulkPlan{}, err
		}

		for _, channel := range channels {
			env := cloneBulkEnvelope(base.env)
			env.Channel = channel
			notification := notificationName(base.contract)
			if suppression, ok := p.matchSuppression(notification, env.Recipient, channel); ok {
				plan.Suppressed = append(plan.Suppressed, BulkSuppressedRecipient{
					Notification: notification,
					Recipient:    env.Recipient,
					Channel:      channel,
					ID:           env.ID,
					Reason:       bulkSuppressionReason(suppression),
				})
				continue
			}

			work := bulkExpandedWork{
				contract:   base.contract,
				env:        env,
				receivedAt: base.receivedAt,
				seq:        i,
			}
			if base.contract.Digest != nil {
				key := bulkDigestKey(base.contract, channel)
				group := digestGroups[key]
				if group == nil {
					group = &bulkDigestGroup{contract: base.contract, channel: channel}
					digestGroups[key] = group
				}
				group.pending = append(group.pending, PendingDigestNotification{
					Contract:   base.contract,
					Envelope:   env,
					ReceivedAt: base.receivedAt,
				})
				continue
			}

			sendItems = append(sendItems, directBulkSendWork(work))
		}
	}

	digestWorks, digestPlans, err := p.planDigests(digestGroups)
	if err != nil {
		return BulkPlan{}, err
	}
	sendItems = append(sendItems, digestWorks...)
	plan.DigestPlans = digestPlans

	sort.SliceStable(sendItems, func(i, j int) bool {
		return bulkSendWorkLess(sendItems[i], sendItems[j])
	})
	if err := assignBulkRateWindows(sendItems, now); err != nil {
		return BulkPlan{}, err
	}

	plan.Batches = buildBulkChannelBatches(sendItems, p.MaxBatchSize)
	plan.Summary = summarizeBulkPlan(len(pending), plan.Batches, plan.DigestPlans, plan.Suppressed)
	return plan, nil
}

type bulkNormalizedNotification struct {
	contract   NotificationContract
	env        Envelope
	receivedAt time.Time
}

type bulkExpandedWork struct {
	contract   NotificationContract
	env        Envelope
	receivedAt time.Time
	seq        int
}

type bulkSendWork struct {
	contract NotificationContract
	env      Envelope
	item     BulkPlanItem
	seq      int
}

type bulkDigestGroupKey struct {
	channel      Channel
	notification string
	template     string
	every        string
	maxSize      uint32
	strategy     DigestStrategy
}

type bulkDigestGroup struct {
	contract NotificationContract
	channel  Channel
	pending  []PendingDigestNotification
}

type bulkRateState struct {
	count int
}

type bulkBatchKey struct {
	channel     Channel
	windowStart time.Time
	windowEnd   time.Time
}

type bulkRateWindowSummaryKey struct {
	notification string
	recipient    string
	channel      Channel
	windowStart  time.Time
	windowEnd    time.Time
}

func normalizeBulkNotification(
	next BulkNotification,
	now time.Time,
	index int,
) (bulkNormalizedNotification, []Channel, error) {
	contract := next.Contract
	env := cloneBulkEnvelope(next.Envelope)
	channels := bulkPlanChannels(contract, env)
	if len(channels) == 0 {
		return bulkNormalizedNotification{}, nil, fmt.Errorf("%w: notification[%d] %s", ErrNotificationNoChannels, index, notificationName(contract))
	}

	payload := firstBulkPayload(env)
	var err error
	env.Recipient = strings.TrimSpace(env.Recipient)
	if env.Recipient == "" {
		env.Recipient, err = resolveRequiredPathString(
			payload,
			contract.Recipient,
			ErrNotificationRecipientUnresolved,
			contract,
		)
		if err != nil {
			return bulkNormalizedNotification{}, nil, fmt.Errorf("notification[%d]: %w", index, err)
		}
	}

	env.Tenant = strings.TrimSpace(env.Tenant)
	if env.Tenant == "" && contract.TenantFrom != nil && contract.TenantFrom.Path != "" {
		env.Tenant, err = resolveRequiredPathString(
			payload,
			contract.TenantFrom.Path,
			ErrNotificationTenantUnresolved,
			contract,
		)
		if err != nil {
			return bulkNormalizedNotification{}, nil, fmt.Errorf("notification[%d]: %w", index, err)
		}
	}

	env.ID = strings.TrimSpace(env.ID)
	if env.ID == "" && contract.Idempotency != nil && contract.Idempotency.Path != "" {
		env.ID, err = resolveRequiredPathString(
			payload,
			contract.Idempotency.Path,
			ErrNotificationIdempotencyUnresolved,
			contract,
		)
		if err != nil {
			return bulkNormalizedNotification{}, nil, fmt.Errorf("notification[%d]: %w", index, err)
		}
	}

	if env.TemplateData == nil {
		env.TemplateData = cloneNotificationPayload(env.Payload)
	}

	receivedAt := next.ReceivedAt
	if receivedAt.IsZero() {
		receivedAt = now
	}

	return bulkNormalizedNotification{
		contract:   contract,
		env:        env,
		receivedAt: receivedAt,
	}, channels, nil
}

func bulkPlanChannels(contract NotificationContract, env Envelope) []Channel {
	if env.Channel != "" {
		return []Channel{env.Channel}
	}
	if len(contract.Channels) == 0 {
		return nil
	}
	return append([]Channel(nil), contract.Channels...)
}

func (p BulkPlanner) matchSuppression(notification string, recipient string, channel Channel) (BulkSuppression, bool) {
	for _, suppression := range p.Suppressions {
		if !bulkSuppressionStringMatches(suppression.Notification, notification) {
			continue
		}
		if !bulkSuppressionStringMatches(suppression.Recipient, recipient) {
			continue
		}
		if suppression.Channel != "" && suppression.Channel != channel {
			continue
		}
		return suppression, true
	}
	return BulkSuppression{}, false
}

func bulkSuppressionStringMatches(pattern string, value string) bool {
	pattern = strings.TrimSpace(pattern)
	return pattern == "" || pattern == value
}

func bulkSuppressionReason(suppression BulkSuppression) string {
	reason := strings.TrimSpace(suppression.Reason)
	if reason == "" {
		return "suppressed"
	}
	return reason
}

func directBulkSendWork(work bulkExpandedWork) bulkSendWork {
	notification := notificationName(work.contract)
	return bulkSendWork{
		contract: work.contract,
		env:      cloneBulkEnvelope(work.env),
		seq:      work.seq,
		item: BulkPlanItem{
			Kind:          BulkPlanItemDirect,
			Notification:  notification,
			Notifications: []string{notification},
			ID:            work.env.ID,
			Tenant:        work.env.Tenant,
			Recipient:     work.env.Recipient,
			Channel:       work.env.Channel,
			Template:      work.contract.Template,
			Payload:       cloneNotificationPayload(work.env.Payload),
			TemplateData:  cloneNotificationPayload(work.env.TemplateData),
			ReceivedAt:    work.receivedAt,
		},
	}
}

func (p BulkPlanner) planDigests(groups map[bulkDigestGroupKey]*bulkDigestGroup) ([]bulkSendWork, []BulkDigestPlan, error) {
	ordered := make([]*bulkDigestGroup, 0, len(groups))
	for _, group := range groups {
		ordered = append(ordered, group)
	}
	sort.SliceStable(ordered, func(i, j int) bool {
		return bulkDigestGroupLess(ordered[i], ordered[j])
	})

	var sendItems []bulkSendWork
	var digestPlans []BulkDigestPlan
	for _, group := range ordered {
		plans, err := p.DigestPlanner.Plan(group.pending)
		if err != nil {
			return nil, nil, err
		}
		for planIndex, digest := range plans {
			cloned := cloneBulkDigestPlan(digest)
			digestPlans = append(digestPlans, BulkDigestPlan{
				Channel:    group.channel,
				DigestPlan: cloned,
			})
			sendItems = append(sendItems, digestBulkSendWork(group.contract, group.channel, cloned, planIndex))
		}
	}
	return sendItems, digestPlans, nil
}

func digestBulkSendWork(contract NotificationContract, channel Channel, digest DigestPlan, seq int) bulkSendWork {
	notifications := bulkDigestNotifications(digest)
	notification := ""
	if len(notifications) == 1 {
		notification = notifications[0]
	}
	env := Envelope{
		Tenant:       bulkDigestTenant(digest),
		Channel:      channel,
		Recipient:    digest.Recipient,
		TemplateData: cloneNotificationPayload(digest.TemplateData),
	}
	return bulkSendWork{
		contract: contract,
		env:      env,
		seq:      seq,
		item: BulkPlanItem{
			Kind:              BulkPlanItemDigest,
			Notification:      notification,
			Notifications:     notifications,
			Tenant:            env.Tenant,
			Recipient:         digest.Recipient,
			Channel:           channel,
			Template:          digest.Template,
			TemplateData:      cloneNotificationPayload(digest.TemplateData),
			ReceivedAt:        digest.WindowEnd,
			DigestWindowStart: digest.WindowStart,
			DigestWindowEnd:   digest.WindowEnd,
			DigestBatchIndex:  digest.BatchIndex,
			DigestBatchCount:  digest.BatchCount,
			DigestItemCount:   len(digest.Items),
		},
	}
}

func assignBulkRateWindows(items []bulkSendWork, now time.Time) error {
	states := make(map[ThrottleKey]*bulkRateState)
	for i := range items {
		throttle := items[i].contract.Throttle
		if throttle == nil {
			continue
		}
		window, err := parseDuration(throttle.MaxPer)
		if err != nil || window <= 0 {
			return fmt.Errorf("%w: %s throttle max_per %q", ErrInvalidDuration, notificationName(items[i].contract), throttle.MaxPer)
		}
		limit := int(throttle.Burst)
		if limit == 0 {
			limit = 1
		}

		key := throttleKey(items[i].contract, items[i].env)
		state := states[key]
		if state == nil {
			state = &bulkRateState{}
			states[key] = state
		}
		windowOffset := state.count / limit
		position := state.count%limit + 1
		windowStart := now.Add(time.Duration(windowOffset) * window)
		items[i].item.RateWindow = BulkRateWindow{
			Notification: key.Notification,
			Recipient:    key.Recipient,
			Channel:      key.Channel,
			WindowStart:  windowStart,
			WindowEnd:    windowStart.Add(window),
			WindowIndex:  windowOffset + 1,
			Limit:        limit,
			Position:     position,
			Deferred:     windowOffset > 0,
		}
		state.count++
	}
	return nil
}

func buildBulkChannelBatches(items []bulkSendWork, maxBatchSize int) []BulkChannelBatch {
	if len(items) == 0 {
		return nil
	}

	groups := make(map[bulkBatchKey][]BulkPlanItem)
	var order []bulkBatchKey
	for _, work := range items {
		key := bulkBatchKey{channel: work.item.Channel}
		if bulkRateWindowActive(work.item.RateWindow) {
			key.windowStart = work.item.RateWindow.WindowStart
			key.windowEnd = work.item.RateWindow.WindowEnd
		}
		if _, ok := groups[key]; !ok {
			order = append(order, key)
		}
		groups[key] = append(groups[key], cloneBulkPlanItem(work.item))
	}
	sort.SliceStable(order, func(i, j int) bool {
		return bulkBatchKeyLess(order[i], order[j])
	})

	var batches []BulkChannelBatch
	for _, key := range order {
		items := groups[key]
		chunks := bulkPlanItemChunks(items, maxBatchSize)
		for i, chunk := range chunks {
			batches = append(batches, BulkChannelBatch{
				Channel:     key.channel,
				WindowStart: key.windowStart,
				WindowEnd:   key.windowEnd,
				BatchIndex:  i + 1,
				BatchCount:  len(chunks),
				Items:       cloneBulkPlanItems(chunk),
			})
		}
	}
	return batches
}

func summarizeBulkPlan(
	inputCount int,
	batches []BulkChannelBatch,
	digests []BulkDigestPlan,
	suppressed []BulkSuppressedRecipient,
) BulkPlanSummary {
	summary := BulkPlanSummary{
		InputCount:      inputCount,
		SuppressedCount: len(suppressed),
		BatchCount:      len(batches),
		DigestCount:     len(digests),
	}

	channelSummaries := make(map[Channel]*BulkChannelSummary)
	rateWindows := make(map[bulkRateWindowSummaryKey]struct{})
	for _, batch := range batches {
		channel := channelSummaries[batch.Channel]
		if channel == nil {
			channel = &BulkChannelSummary{Channel: batch.Channel}
			channelSummaries[batch.Channel] = channel
		}
		channel.BatchCount++
		channel.PlannedCount += len(batch.Items)
		summary.PlannedCount += len(batch.Items)

		for _, item := range batch.Items {
			switch item.Kind {
			case BulkPlanItemDigest:
				summary.DigestSourceCount += item.DigestItemCount
			default:
				summary.DirectCount++
			}
			if bulkRateWindowActive(item.RateWindow) {
				key := bulkRateWindowSummaryKey{
					notification: item.RateWindow.Notification,
					recipient:    item.RateWindow.Recipient,
					channel:      item.RateWindow.Channel,
					windowStart:  item.RateWindow.WindowStart,
					windowEnd:    item.RateWindow.WindowEnd,
				}
				if _, ok := rateWindows[key]; !ok {
					rateWindows[key] = struct{}{}
					channel.RateWindowCount++
					summary.RateWindowCount++
				}
			}
			if item.RateWindow.Deferred {
				summary.RateDeferredCount++
			}
		}
	}

	channels := make([]BulkChannelSummary, 0, len(channelSummaries))
	for _, channel := range channelSummaries {
		channels = append(channels, *channel)
	}
	sort.SliceStable(channels, func(i, j int) bool {
		return channels[i].Channel < channels[j].Channel
	})
	summary.Channels = channels

	return summary
}

func bulkDigestKey(contract NotificationContract, channel Channel) bulkDigestGroupKey {
	key := bulkDigestGroupKey{
		channel:      channel,
		notification: notificationName(contract),
		template:     contract.Template,
	}
	if contract.Digest != nil {
		key.every = contract.Digest.Every
		key.maxSize = contract.Digest.MaxSize
		key.strategy = contract.Digest.TemplateStrategy
	}
	return key
}

func firstBulkPayload(env Envelope) map[string]any {
	if env.Payload != nil {
		return env.Payload
	}
	return env.TemplateData
}

func cloneBulkEnvelope(env Envelope) Envelope {
	env.Payload = cloneNotificationPayload(env.Payload)
	env.TemplateData = cloneNotificationPayload(env.TemplateData)
	return env
}

func cloneBulkDigestPlan(plan DigestPlan) DigestPlan {
	plan.Items = cloneDigestPlanItems(plan.Items)
	plan.TemplateData = cloneNotificationPayload(plan.TemplateData)
	return plan
}

func cloneBulkPlanItem(item BulkPlanItem) BulkPlanItem {
	item.Notifications = append([]string(nil), item.Notifications...)
	item.Payload = cloneNotificationPayload(item.Payload)
	item.TemplateData = cloneNotificationPayload(item.TemplateData)
	return item
}

func cloneBulkPlanItems(items []BulkPlanItem) []BulkPlanItem {
	out := make([]BulkPlanItem, len(items))
	for i := range items {
		out[i] = cloneBulkPlanItem(items[i])
	}
	return out
}

func bulkPlanItemChunks(items []BulkPlanItem, maxBatchSize int) [][]BulkPlanItem {
	if len(items) == 0 {
		return nil
	}
	if maxBatchSize == 0 || maxBatchSize >= len(items) {
		return [][]BulkPlanItem{items}
	}

	chunks := make([][]BulkPlanItem, 0, (len(items)+maxBatchSize-1)/maxBatchSize)
	for start := 0; start < len(items); start += maxBatchSize {
		end := start + maxBatchSize
		if end > len(items) {
			end = len(items)
		}
		chunks = append(chunks, items[start:end])
	}
	return chunks
}

func bulkDigestNotifications(plan DigestPlan) []string {
	seen := make(map[string]struct{})
	notifications := make([]string, 0, len(plan.Items))
	for _, item := range plan.Items {
		if item.Notification == "" {
			continue
		}
		if _, ok := seen[item.Notification]; ok {
			continue
		}
		seen[item.Notification] = struct{}{}
		notifications = append(notifications, item.Notification)
	}
	sort.Strings(notifications)
	return notifications
}

func bulkDigestTenant(plan DigestPlan) string {
	for _, item := range plan.Items {
		if item.Tenant != "" {
			return item.Tenant
		}
	}
	return ""
}

func bulkRateWindowActive(window BulkRateWindow) bool {
	return window.Limit > 0
}

func bulkPlanNow(now time.Time) time.Time {
	if now.IsZero() {
		return time.Now().UTC()
	}
	return now.UTC()
}

func bulkDigestGroupLess(left, right *bulkDigestGroup) bool {
	leftKey := bulkDigestKey(left.contract, left.channel)
	rightKey := bulkDigestKey(right.contract, right.channel)
	if leftKey.channel != rightKey.channel {
		return leftKey.channel < rightKey.channel
	}
	if leftKey.notification != rightKey.notification {
		return leftKey.notification < rightKey.notification
	}
	if leftKey.template != rightKey.template {
		return leftKey.template < rightKey.template
	}
	if leftKey.every != rightKey.every {
		return leftKey.every < rightKey.every
	}
	if leftKey.maxSize != rightKey.maxSize {
		return leftKey.maxSize < rightKey.maxSize
	}
	return leftKey.strategy < rightKey.strategy
}

func bulkSendWorkLess(left, right bulkSendWork) bool {
	if left.item.Channel != right.item.Channel {
		return left.item.Channel < right.item.Channel
	}
	if !left.item.ReceivedAt.Equal(right.item.ReceivedAt) {
		return left.item.ReceivedAt.Before(right.item.ReceivedAt)
	}
	if left.item.Recipient != right.item.Recipient {
		return left.item.Recipient < right.item.Recipient
	}
	if left.item.Notification != right.item.Notification {
		return left.item.Notification < right.item.Notification
	}
	if left.item.ID != right.item.ID {
		return left.item.ID < right.item.ID
	}
	return left.seq < right.seq
}

func bulkBatchKeyLess(left, right bulkBatchKey) bool {
	if left.channel != right.channel {
		return left.channel < right.channel
	}
	if !left.windowStart.Equal(right.windowStart) {
		if left.windowStart.IsZero() {
			return true
		}
		if right.windowStart.IsZero() {
			return false
		}
		return left.windowStart.Before(right.windowStart)
	}
	return left.windowEnd.Before(right.windowEnd)
}
