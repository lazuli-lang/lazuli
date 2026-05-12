package notifications_test

import (
	"errors"
	"reflect"
	"testing"
	"time"

	"lazuli.dev/runtime/lazuli/notifications"
)

func TestDigestPlannerGroupsByRecipientTemplateWindow(t *testing.T) {
	t.Parallel()

	base := time.Date(2026, 5, 12, 12, 1, 0, 0, time.UTC)
	contract := digestPlannerContract("customer_digest", "15 minutes", 10, notifications.DigestStrategyAppend)
	pending := []notifications.PendingDigestNotification{
		digestPlannerPending(contract, "notif-1", "ada@example.com", base, map[string]any{"Name": "Ada"}),
		digestPlannerPending(contract, "notif-2", "ada@example.com", base.Add(5*time.Minute), map[string]any{"Name": "Grace"}),
		digestPlannerPending(contract, "notif-3", "grace@example.com", base.Add(7*time.Minute), map[string]any{"Name": "Lin"}),
		digestPlannerPending(contract, "notif-4", "ada@example.com", base.Add(16*time.Minute), map[string]any{"Name": "Katherine"}),
	}

	plans, err := notifications.PlanDigestNotifications(pending)
	if err != nil {
		t.Fatalf("PlanDigestNotifications: %v", err)
	}
	if len(plans) != 3 {
		t.Fatalf("plans = %d, want 3", len(plans))
	}

	first := plans[0]
	windowStart := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)
	if first.Recipient != "ada@example.com" {
		t.Fatalf("first recipient = %q, want ada@example.com", first.Recipient)
	}
	if first.Template != "customer_digest" {
		t.Fatalf("first template = %q, want customer_digest", first.Template)
	}
	if !first.WindowStart.Equal(windowStart) || !first.WindowEnd.Equal(windowStart.Add(15*time.Minute)) {
		t.Fatalf("first window = %s..%s", first.WindowStart, first.WindowEnd)
	}
	if len(first.Items) != 2 {
		t.Fatalf("first items = %d, want 2", len(first.Items))
	}

	digest := digestData(t, first.TemplateData)
	if digest["Recipient"] != "ada@example.com" {
		t.Fatalf("Digest.Recipient = %v", digest["Recipient"])
	}
	if digest["Template"] != "customer_digest" {
		t.Fatalf("Digest.Template = %v", digest["Template"])
	}
	if got, ok := digest["WindowStart"].(time.Time); !ok || !got.Equal(windowStart) {
		t.Fatalf("Digest.WindowStart = %#v, want %s", digest["WindowStart"], windowStart)
	}
	assertDigestCount(t, first.TemplateData, 2)

	if plans[1].Recipient != "grace@example.com" {
		t.Fatalf("second recipient = %q, want grace@example.com", plans[1].Recipient)
	}
	if !plans[2].WindowStart.Equal(windowStart.Add(15 * time.Minute)) {
		t.Fatalf("third window start = %s", plans[2].WindowStart)
	}
}

func TestDigestPlannerSplitsAtMaxBatchSizeAndKeepsOrder(t *testing.T) {
	t.Parallel()

	base := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)
	contract := digestPlannerContract("customer_digest", "1 hour", 2, notifications.DigestStrategyAppend)
	pending := []notifications.PendingDigestNotification{
		digestPlannerPending(contract, "notif-3", "ada@example.com", base.Add(3*time.Minute), map[string]any{"Name": "Third"}),
		digestPlannerPending(contract, "notif-1", "ada@example.com", base.Add(time.Minute), map[string]any{"Name": "First"}),
		digestPlannerPending(contract, "notif-5", "ada@example.com", base.Add(5*time.Minute), map[string]any{"Name": "Fifth"}),
		digestPlannerPending(contract, "notif-2", "ada@example.com", base.Add(2*time.Minute), map[string]any{"Name": "Second"}),
		digestPlannerPending(contract, "notif-4", "ada@example.com", base.Add(4*time.Minute), map[string]any{"Name": "Fourth"}),
	}

	plans, err := notifications.PlanDigestNotifications(pending)
	if err != nil {
		t.Fatalf("PlanDigestNotifications: %v", err)
	}
	if len(plans) != 3 {
		t.Fatalf("plans = %d, want 3", len(plans))
	}

	wantBatches := [][]string{
		{"First", "Second"},
		{"Third", "Fourth"},
		{"Fifth"},
	}
	for i, plan := range plans {
		if plan.BatchIndex != i+1 || plan.BatchCount != 3 {
			t.Fatalf("plan %d batch = %d/%d, want %d/3", i, plan.BatchIndex, plan.BatchCount, i+1)
		}
		assertDigestCount(t, plan.TemplateData, len(wantBatches[i]))
		digest := digestData(t, plan.TemplateData)
		if digest["BatchIndex"] != i+1 || digest["BatchCount"] != 3 {
			t.Fatalf("digest batch metadata = %v/%v, want %d/3", digest["BatchIndex"], digest["BatchCount"], i+1)
		}

		items, ok := digest["Items"].([]map[string]any)
		if !ok {
			t.Fatalf("Digest.Items = %T, want []map[string]any", digest["Items"])
		}
		var got []string
		for _, item := range items {
			got = append(got, item["Name"].(string))
		}
		if !reflect.DeepEqual(got, wantBatches[i]) {
			t.Fatalf("batch %d item names = %#v, want %#v", i, got, wantBatches[i])
		}
	}
}

func TestDigestPlannerMergeRendersMetadataAndCopiesPayloads(t *testing.T) {
	t.Parallel()

	base := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)
	contract := digestPlannerContract("customer_digest", "1 hour", 10, notifications.DigestStrategyMerge)
	firstPayload := map[string]any{
		"Status": "queued",
		"Nested": map[string]any{"Count": 1},
	}
	secondPayload := map[string]any{"Status": "sent"}
	pending := []notifications.PendingDigestNotification{
		digestPlannerPending(contract, "notif-1", "ada@example.com", base, firstPayload),
		digestPlannerPending(contract, "notif-2", "ada@example.com", base.Add(time.Minute), secondPayload),
	}

	plans, err := notifications.PlanDigestNotifications(pending)
	if err != nil {
		t.Fatalf("PlanDigestNotifications: %v", err)
	}
	if len(plans) != 1 {
		t.Fatalf("plans = %d, want 1", len(plans))
	}

	firstPayload["Status"] = "mutated"
	firstPayload["Nested"].(map[string]any)["Count"] = 99
	secondPayload["Status"] = "mutated"

	plan := plans[0]
	if got := plan.TemplateData["Status"]; got != "sent" {
		t.Fatalf("merged Status = %v, want sent", got)
	}
	if got := plan.Items[0].Payload["Status"]; got != "queued" {
		t.Fatalf("plan item payload Status = %v, want queued", got)
	}
	nested := plan.Items[0].Payload["Nested"].(map[string]any)
	if nested["Count"] != 1 {
		t.Fatalf("nested payload Count = %v, want 1", nested["Count"])
	}
	digest := digestData(t, plan.TemplateData)
	if digest["Items"] != nil {
		t.Fatalf("merge Digest.Items = %#v, want nil", digest["Items"])
	}
	if got := digest["Notifications"]; !reflect.DeepEqual(got, []string{"billing.invoice_ready"}) {
		t.Fatalf("Digest.Notifications = %#v", got)
	}
}

func TestDigestPlannerRejectsInvalidWindow(t *testing.T) {
	t.Parallel()

	contract := digestPlannerContract("customer_digest", "soon", 10, notifications.DigestStrategyAppend)
	_, err := notifications.PlanDigestNotifications([]notifications.PendingDigestNotification{
		digestPlannerPending(contract, "notif-1", "ada@example.com", time.Now(), map[string]any{"Name": "Ada"}),
	})
	if !errors.Is(err, notifications.ErrInvalidDuration) {
		t.Fatalf("error = %v, want ErrInvalidDuration", err)
	}
}

func digestPlannerContract(template, every string, maxSize uint32, strategy notifications.DigestStrategy) notifications.NotificationContract {
	return notifications.NotificationContract{
		Feature:   "billing",
		Name:      "invoice_ready",
		Template:  template,
		Recipient: "payload.email",
		Digest: &notifications.NotificationDigest{
			Every:            every,
			MaxSize:          maxSize,
			TemplateStrategy: strategy,
		},
	}
}

func digestPlannerPending(
	contract notifications.NotificationContract,
	id string,
	recipient string,
	receivedAt time.Time,
	payload map[string]any,
) notifications.PendingDigestNotification {
	payload["email"] = recipient
	return notifications.PendingDigestNotification{
		Contract: contract,
		Envelope: notifications.Envelope{
			ID:        id,
			Tenant:    "tenant-1",
			Recipient: recipient,
			Payload:   payload,
		},
		ReceivedAt: receivedAt,
	}
}
