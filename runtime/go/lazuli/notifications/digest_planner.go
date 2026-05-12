package notifications

import (
	"fmt"
	"sort"
	"time"
)

// PendingDigestNotification is one buffered notification trigger waiting to be
// emitted as part of a digest.
type PendingDigestNotification struct {
	Contract   NotificationContract
	Envelope   Envelope
	ReceivedAt time.Time
}

// DigestPlanner groups pending digest notifications into render-ready batches.
//
// MaxBatchSize is an optional process-level cap. When zero, the planner uses
// each contract's NotificationDigest.MaxSize. When both are set, the smaller
// positive cap wins.
type DigestPlanner struct {
	MaxBatchSize uint32
}

// DigestPlan is one digest batch for a recipient/template/window group.
type DigestPlan struct {
	Recipient    string
	Template     string
	WindowStart  time.Time
	WindowEnd    time.Time
	BatchIndex   int
	BatchCount   int
	Items        []DigestPlanItem
	TemplateData map[string]any
}

// DigestPlanItem is one original pending notification inside a digest batch.
type DigestPlanItem struct {
	Notification string
	Tenant       string
	ID           string
	Channel      Channel
	Payload      map[string]any
	TemplateData map[string]any
	ReceivedAt   time.Time
}

// PlanDigestNotifications groups pending notifications using the default
// DigestPlanner.
func PlanDigestNotifications(pending []PendingDigestNotification) ([]DigestPlan, error) {
	return DigestPlanner{}.Plan(pending)
}

// Plan groups pending notifications by recipient, template, and digest window.
// Batches are emitted in deterministic order and are render-ready via
// DigestPlan.TemplateData.
func (p DigestPlanner) Plan(pending []PendingDigestNotification) ([]DigestPlan, error) {
	groups := make(map[digestPlanKey]*digestPlanGroup)

	for _, next := range pending {
		digest := next.Contract.Digest
		if digest == nil {
			return nil, fmt.Errorf("notifications: digest not configured for %s", notificationName(next.Contract))
		}

		window, err := parseDuration(digest.Every)
		if err != nil || window <= 0 {
			return nil, fmt.Errorf("%w: %s digest every %q", ErrInvalidDuration, notificationName(next.Contract), digest.Every)
		}

		recipient := next.Envelope.Recipient
		if recipient == "" {
			recipient, err = resolveRequiredPathString(
				firstDigestPayload(next.Envelope),
				next.Contract.Recipient,
				ErrNotificationRecipientUnresolved,
				next.Contract,
			)
			if err != nil {
				return nil, err
			}
		}

		receivedAt := next.ReceivedAt
		windowStart := receivedAt.Truncate(window)
		windowEnd := windowStart.Add(window)
		key := digestPlanKey{
			recipient:   recipient,
			template:    next.Contract.Template,
			windowStart: windowStart,
			windowEnd:   windowEnd,
		}

		group := groups[key]
		if group == nil {
			group = &digestPlanGroup{
				key:      key,
				strategy: digest.TemplateStrategy,
			}
			groups[key] = group
		}
		group.maxBatchSize = digestPlanEffectiveMaxBatchSize(group.maxBatchSize, p.MaxBatchSize, digest.MaxSize)
		group.items = append(group.items, digestPlanItemFromPending(next, recipient))
	}

	orderedGroups := make([]*digestPlanGroup, 0, len(groups))
	for _, group := range groups {
		orderedGroups = append(orderedGroups, group)
	}
	sort.Slice(orderedGroups, func(i, j int) bool {
		return digestPlanKeyLess(orderedGroups[i].key, orderedGroups[j].key)
	})

	var plans []DigestPlan
	for _, group := range orderedGroups {
		sort.SliceStable(group.items, func(i, j int) bool {
			left := group.items[i]
			right := group.items[j]
			if !left.ReceivedAt.Equal(right.ReceivedAt) {
				return left.ReceivedAt.Before(right.ReceivedAt)
			}
			if left.Notification != right.Notification {
				return left.Notification < right.Notification
			}
			return left.ID < right.ID
		})

		chunks := digestPlanChunks(group.items, group.maxBatchSize)
		for i, chunk := range chunks {
			templateData, err := digestPlanTemplateData(group.strategy, group.key, chunk, i+1, len(chunks))
			if err != nil {
				return nil, err
			}
			plans = append(plans, DigestPlan{
				Recipient:    group.key.recipient,
				Template:     group.key.template,
				WindowStart:  group.key.windowStart,
				WindowEnd:    group.key.windowEnd,
				BatchIndex:   i + 1,
				BatchCount:   len(chunks),
				Items:        cloneDigestPlanItems(chunk),
				TemplateData: templateData,
			})
		}
	}

	return plans, nil
}

type digestPlanKey struct {
	recipient   string
	template    string
	windowStart time.Time
	windowEnd   time.Time
}

type digestPlanGroup struct {
	key          digestPlanKey
	strategy     DigestStrategy
	maxBatchSize uint32
	items        []DigestPlanItem
}

func digestPlanItemFromPending(pending PendingDigestNotification, recipient string) DigestPlanItem {
	env := pending.Envelope
	env.Recipient = recipient
	return DigestPlanItem{
		Notification: notificationName(pending.Contract),
		Tenant:       env.Tenant,
		ID:           env.ID,
		Channel:      env.Channel,
		Payload:      cloneNotificationPayload(env.Payload),
		TemplateData: cloneNotificationPayload(env.TemplateData),
		ReceivedAt:   pending.ReceivedAt,
	}
}

func firstDigestPayload(env Envelope) map[string]any {
	if env.Payload != nil {
		return env.Payload
	}
	return env.TemplateData
}

func digestPlanEffectiveMaxBatchSize(current, plannerMax, contractMax uint32) uint32 {
	next := current
	for _, candidate := range []uint32{plannerMax, contractMax} {
		if candidate == 0 {
			continue
		}
		if next == 0 || candidate < next {
			next = candidate
		}
	}
	return next
}

func digestPlanChunks(items []DigestPlanItem, maxBatchSize uint32) [][]DigestPlanItem {
	if len(items) == 0 {
		return nil
	}
	if maxBatchSize == 0 || int(maxBatchSize) >= len(items) {
		return [][]DigestPlanItem{items}
	}

	chunks := make([][]DigestPlanItem, 0, (len(items)+int(maxBatchSize)-1)/int(maxBatchSize))
	for start := 0; start < len(items); start += int(maxBatchSize) {
		end := start + int(maxBatchSize)
		if end > len(items) {
			end = len(items)
		}
		chunks = append(chunks, items[start:end])
	}
	return chunks
}

func digestPlanTemplateData(
	strategy DigestStrategy,
	key digestPlanKey,
	items []DigestPlanItem,
	batchIndex int,
	batchCount int,
) (map[string]any, error) {
	payloads := make([]map[string]any, 0, len(items))
	notifications := make([]string, 0, len(items))
	seenNotifications := make(map[string]struct{})
	for _, item := range items {
		payload := item.TemplateData
		if payload == nil {
			payload = item.Payload
		}
		payloads = append(payloads, cloneNotificationPayload(payload))

		if _, ok := seenNotifications[item.Notification]; !ok {
			seenNotifications[item.Notification] = struct{}{}
			notifications = append(notifications, item.Notification)
		}
	}
	sort.Strings(notifications)

	data, err := DigestTemplateData(strategy, nil, payloads)
	if err != nil {
		return nil, err
	}

	digest, ok := data[DigestTemplateDataKey].(map[string]any)
	if !ok {
		digest = make(map[string]any)
		data[DigestTemplateDataKey] = digest
	}
	digest["Recipient"] = key.recipient
	digest["Template"] = key.template
	digest["WindowStart"] = key.windowStart
	digest["WindowEnd"] = key.windowEnd
	digest["BatchIndex"] = batchIndex
	digest["BatchCount"] = batchCount
	digest["Notifications"] = notifications
	return data, nil
}

func cloneDigestPlanItems(items []DigestPlanItem) []DigestPlanItem {
	out := make([]DigestPlanItem, len(items))
	for i := range items {
		out[i] = items[i]
		out[i].Payload = cloneNotificationPayload(items[i].Payload)
		out[i].TemplateData = cloneNotificationPayload(items[i].TemplateData)
	}
	return out
}

func digestPlanKeyLess(left, right digestPlanKey) bool {
	if !left.windowStart.Equal(right.windowStart) {
		return left.windowStart.Before(right.windowStart)
	}
	if !left.windowEnd.Equal(right.windowEnd) {
		return left.windowEnd.Before(right.windowEnd)
	}
	if left.recipient != right.recipient {
		return left.recipient < right.recipient
	}
	return left.template < right.template
}
