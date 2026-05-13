package notifications_test

import (
	"errors"
	"reflect"
	"testing"
	"time"

	"lazuli.dev/runtime/lazuli/notifications"
)

func TestBulkPlannerBuildsChannelBatchesAndSuppressesRecipients(t *testing.T) {
	t.Parallel()

	now := time.Date(2026, 5, 12, 14, 0, 0, 0, time.UTC)
	contract := bulkPlannerContract([]notifications.Channel{
		notifications.ChannelSlack,
		notifications.ChannelEmail,
	})
	payload := map[string]any{
		"email": "ada@example.com",
		"meta":  map[string]any{"count": 1},
	}
	pending := []notifications.BulkNotification{
		bulkPlannerPending(contract, "notif-1", now.Add(time.Minute), payload),
		bulkPlannerPending(contract, "notif-2", now.Add(2*time.Minute), map[string]any{"email": "grace@example.com"}),
	}

	plan, err := notifications.BulkPlanner{
		MaxBatchSize: 2,
		Now:          now,
		Suppressions: []notifications.BulkSuppression{{
			Recipient: "ada@example.com",
			Channel:   notifications.ChannelEmail,
			Reason:    "unsubscribed",
		}},
	}.Plan(pending)
	if err != nil {
		t.Fatalf("Plan: %v", err)
	}

	if !plan.DryRun {
		t.Fatalf("DryRun = false, want true")
	}
	if len(plan.Suppressed) != 1 {
		t.Fatalf("suppressed = %d, want 1", len(plan.Suppressed))
	}
	suppressed := plan.Suppressed[0]
	if suppressed.Recipient != "ada@example.com" || suppressed.Channel != notifications.ChannelEmail || suppressed.Reason != "unsubscribed" {
		t.Fatalf("suppressed = %+v", suppressed)
	}
	if len(plan.Batches) != 2 {
		t.Fatalf("batches = %d, want 2", len(plan.Batches))
	}

	email := plan.Batches[0]
	if email.Channel != notifications.ChannelEmail {
		t.Fatalf("first batch channel = %q, want email", email.Channel)
	}
	if got := bulkPlannerRecipients(email.Items); !reflect.DeepEqual(got, []string{"grace@example.com"}) {
		t.Fatalf("email recipients = %#v", got)
	}

	slack := plan.Batches[1]
	if slack.Channel != notifications.ChannelSlack {
		t.Fatalf("second batch channel = %q, want slack", slack.Channel)
	}
	if got := bulkPlannerRecipients(slack.Items); !reflect.DeepEqual(got, []string{"ada@example.com", "grace@example.com"}) {
		t.Fatalf("slack recipients = %#v", got)
	}

	payload["meta"].(map[string]any)["count"] = 99
	gotNested := slack.Items[0].Payload["meta"].(map[string]any)["count"]
	if gotNested != 1 {
		t.Fatalf("planned payload meta.count = %v, want 1", gotNested)
	}

	if plan.Summary.InputCount != 2 ||
		plan.Summary.PlannedCount != 3 ||
		plan.Summary.DirectCount != 3 ||
		plan.Summary.SuppressedCount != 1 ||
		plan.Summary.BatchCount != 2 {
		t.Fatalf("summary = %+v", plan.Summary)
	}
	if got := bulkPlannerChannelCounts(plan.Summary.Channels); !reflect.DeepEqual(got, map[notifications.Channel]int{
		notifications.ChannelEmail: 1,
		notifications.ChannelSlack: 2,
	}) {
		t.Fatalf("channel counts = %#v", got)
	}
}

func TestBulkPlannerPlansDigestBatchesPerChannel(t *testing.T) {
	t.Parallel()

	now := time.Date(2026, 5, 12, 14, 0, 0, 0, time.UTC)
	contract := bulkPlannerContract([]notifications.Channel{
		notifications.ChannelEmail,
		notifications.ChannelInApp,
	})
	contract.Digest = &notifications.NotificationDigest{
		Every:            "1h",
		MaxSize:          2,
		TemplateStrategy: notifications.DigestStrategyAppend,
	}

	plan, err := notifications.BulkPlanner{Now: now}.Plan([]notifications.BulkNotification{
		bulkPlannerPending(contract, "notif-1", now.Add(time.Minute), map[string]any{"email": "ada@example.com", "Name": "First"}),
		bulkPlannerPending(contract, "notif-2", now.Add(2*time.Minute), map[string]any{"email": "ada@example.com", "Name": "Second"}),
		bulkPlannerPending(contract, "notif-3", now.Add(3*time.Minute), map[string]any{"email": "ada@example.com", "Name": "Third"}),
	})
	if err != nil {
		t.Fatalf("Plan: %v", err)
	}

	if len(plan.DigestPlans) != 4 {
		t.Fatalf("digest plans = %d, want 4", len(plan.DigestPlans))
	}
	if len(plan.Batches) != 2 {
		t.Fatalf("batches = %d, want 2", len(plan.Batches))
	}
	firstBatch := plan.Batches[0]
	if firstBatch.Channel != notifications.ChannelEmail {
		t.Fatalf("first batch channel = %q, want email", firstBatch.Channel)
	}
	if len(firstBatch.Items) != 2 {
		t.Fatalf("email digest items = %d, want 2", len(firstBatch.Items))
	}
	firstItem := firstBatch.Items[0]
	if firstItem.Kind != notifications.BulkPlanItemDigest {
		t.Fatalf("first item kind = %q, want digest", firstItem.Kind)
	}
	if firstItem.DigestItemCount != 2 || firstItem.DigestBatchIndex != 1 || firstItem.DigestBatchCount != 2 {
		t.Fatalf("first digest item = %+v", firstItem)
	}
	assertDigestCount(t, firstItem.TemplateData, 2)

	if plan.Summary.PlannedCount != 4 ||
		plan.Summary.DirectCount != 0 ||
		plan.Summary.DigestCount != 4 ||
		plan.Summary.DigestSourceCount != 6 {
		t.Fatalf("summary = %+v", plan.Summary)
	}
}

func TestBulkPlannerAssignsThrottleRateWindows(t *testing.T) {
	t.Parallel()

	now := time.Date(2026, 5, 12, 14, 0, 0, 0, time.UTC)
	contract := bulkPlannerContract([]notifications.Channel{notifications.ChannelEmail})
	contract.Throttle = &notifications.NotificationThrottle{
		MaxPer:       "1h",
		PerRecipient: true,
		PerChannel:   true,
		Burst:        2,
	}

	plan, err := notifications.BulkPlanner{Now: now}.Plan([]notifications.BulkNotification{
		bulkPlannerPending(contract, "notif-1", now.Add(time.Minute), map[string]any{"email": "ada@example.com"}),
		bulkPlannerPending(contract, "notif-2", now.Add(2*time.Minute), map[string]any{"email": "ada@example.com"}),
		bulkPlannerPending(contract, "notif-3", now.Add(3*time.Minute), map[string]any{"email": "ada@example.com"}),
	})
	if err != nil {
		t.Fatalf("Plan: %v", err)
	}

	if len(plan.Batches) != 2 {
		t.Fatalf("batches = %d, want 2", len(plan.Batches))
	}
	first := plan.Batches[0]
	if !first.WindowStart.Equal(now) || !first.WindowEnd.Equal(now.Add(time.Hour)) {
		t.Fatalf("first window = %s..%s", first.WindowStart, first.WindowEnd)
	}
	if len(first.Items) != 2 {
		t.Fatalf("first window items = %d, want 2", len(first.Items))
	}
	for i, item := range first.Items {
		if item.RateWindow.Position != i+1 || item.RateWindow.Limit != 2 {
			t.Fatalf("item %d rate window = %+v", i, item.RateWindow)
		}
	}
	second := plan.Batches[1]
	if !second.WindowStart.Equal(now.Add(time.Hour)) || len(second.Items) != 1 {
		t.Fatalf("second batch = %+v", second)
	}
	if second.Items[0].RateWindow.Position != 1 {
		t.Fatalf("second item position = %d, want 1", second.Items[0].RateWindow.Position)
	}

	if plan.Summary.RateWindowCount != 2 || plan.Summary.RateDeferredCount != 1 {
		t.Fatalf("summary = %+v", plan.Summary)
	}
}

func TestBulkPlannerRejectsInvalidThrottleWindow(t *testing.T) {
	t.Parallel()

	contract := bulkPlannerContract([]notifications.Channel{notifications.ChannelEmail})
	contract.Throttle = &notifications.NotificationThrottle{
		MaxPer:       "soon",
		PerRecipient: true,
		Burst:        1,
	}

	_, err := notifications.BulkPlanner{}.Plan([]notifications.BulkNotification{
		bulkPlannerPending(contract, "notif-1", time.Now(), map[string]any{"email": "ada@example.com"}),
	})
	if !errors.Is(err, notifications.ErrInvalidDuration) {
		t.Fatalf("error = %v, want ErrInvalidDuration", err)
	}
}

func bulkPlannerContract(channels []notifications.Channel) notifications.NotificationContract {
	return notifications.NotificationContract{
		Feature:    "billing",
		Name:       "invoice_ready",
		Channels:   channels,
		Recipient:  "payload.email",
		Template:   "invoice_ready",
		TenantFrom: &notifications.TenantFromSpec{Path: "payload.tenant"},
		Idempotency: &notifications.IdempotencyKeySpec{
			Path: "payload.id",
		},
	}
}

func bulkPlannerPending(
	contract notifications.NotificationContract,
	id string,
	receivedAt time.Time,
	payload map[string]any,
) notifications.BulkNotification {
	payload["id"] = id
	if _, ok := payload["tenant"]; !ok {
		payload["tenant"] = "tenant-1"
	}
	return notifications.BulkNotification{
		Contract: contract,
		Envelope: notifications.Envelope{
			Payload: payload,
		},
		ReceivedAt: receivedAt,
	}
}

func bulkPlannerRecipients(items []notifications.BulkPlanItem) []string {
	recipients := make([]string, 0, len(items))
	for _, item := range items {
		recipients = append(recipients, item.Recipient)
	}
	return recipients
}

func bulkPlannerChannelCounts(summaries []notifications.BulkChannelSummary) map[notifications.Channel]int {
	out := make(map[notifications.Channel]int, len(summaries))
	for _, summary := range summaries {
		out[summary.Channel] = summary.PlannedCount
	}
	return out
}
