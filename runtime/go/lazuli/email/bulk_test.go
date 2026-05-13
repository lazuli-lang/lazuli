package email

import (
	"errors"
	"strings"
	"testing"
)

func TestBuildBulkPlanBatchesSuppressesAndThrottles(t *testing.T) {
	t.Parallel()

	recipients := []BulkRecipient{
		bulkRecipient("alice@gmail.com", "u1"),
		bulkRecipient("bob@yahoo.com", "u2"),
		bulkRecipient("carol@gmail.com", "u3"),
		bulkRecipient("dave@gmail.com", "u4"),
		bulkRecipient("eve@example.com", "u5"),
		bulkRecipient("frank@gmail.com", "u6"),
	}

	plan, err := BuildBulkPlan(recipients, BulkPlanOptions{
		BatchSize:  3,
		CampaignID: "campaign-1",
		ListID:     "weekly",
		DomainThrottles: map[string]BulkDomainThrottle{
			"GMAIL.com": {MaxRecipientsPerBatch: 1},
		},
		Suppressions: []BulkSuppression{
			{Email: "DAVE@GMAIL.com", ListID: "weekly", Reason: "user unsubscribed"},
		},
	})
	if err != nil {
		t.Fatalf("BuildBulkPlan: %v", err)
	}
	if !plan.DryRun {
		t.Fatal("DryRun = false, want true")
	}
	if got := len(plan.Batches); got != 3 {
		t.Fatalf("len(Batches) = %d, want 3", got)
	}

	assertBatchRecipients(t, plan.Batches[0], "alice@gmail.com", "bob@yahoo.com")
	assertBatchRecipients(t, plan.Batches[1], "carol@gmail.com", "eve@example.com")
	assertBatchRecipients(t, plan.Batches[2], "frank@gmail.com")
	for _, batch := range plan.Batches {
		if got := batch.DomainCounts["gmail.com"]; got > 1 {
			t.Fatalf("batch %d gmail.com count = %d, want <= 1", batch.Index, got)
		}
	}

	if got := len(plan.Suppressed); got != 1 {
		t.Fatalf("len(Suppressed) = %d, want 1", got)
	}
	if got := plan.Suppressed[0]; got.Recipient.Address.Email != "dave@gmail.com" || got.Reason != "user unsubscribed" {
		t.Fatalf("Suppressed[0] = %+v, want dave with reason", got)
	}

	summary := plan.Summary
	if summary.TotalRecipients != 6 || summary.PlannedRecipients != 5 || summary.SuppressedRecipients != 1 || summary.BatchCount != 3 {
		t.Fatalf("Summary = %+v, want total 6 planned 5 suppressed 1 batches 3", summary)
	}
	assertDomainSummary(t, summary, BulkDomainSummary{
		Domain:                "gmail.com",
		PlannedRecipients:     3,
		SuppressedRecipients:  1,
		BatchCount:            3,
		MaxRecipientsPerBatch: 1,
	})

	wantKey, err := BulkIdempotencyKey(BulkIdempotencyScope{
		Namespace:  "email.bulk",
		CampaignID: "campaign-1",
		ListID:     "weekly",
	}, recipients[0])
	if err != nil {
		t.Fatalf("BulkIdempotencyKey: %v", err)
	}
	if got := plan.Batches[0].Recipients[0].IdempotencyKey; got != wantKey {
		t.Fatalf("IdempotencyKey = %q, want %q", got, wantKey)
	}
}

func TestBuildBulkPlanListScopedSuppression(t *testing.T) {
	t.Parallel()

	recipients := []BulkRecipient{
		bulkRecipient("global@example.com", "u1"),
		bulkRecipient("weekly@example.com", "u2"),
		bulkRecipient("other@example.com", "u3"),
	}

	plan, err := BuildBulkPlan(recipients, BulkPlanOptions{
		CampaignID: "campaign-1",
		ListID:     "weekly",
		Suppressions: []BulkSuppression{
			{Email: "global@example.com"},
			{Email: "weekly@example.com", ListID: "weekly"},
			{Email: "other@example.com", ListID: "security"},
		},
	})
	if err != nil {
		t.Fatalf("BuildBulkPlan: %v", err)
	}

	if got := len(plan.Suppressed); got != 2 {
		t.Fatalf("len(Suppressed) = %d, want 2", got)
	}
	if got := len(plan.Batches); got != 1 {
		t.Fatalf("len(Batches) = %d, want 1", got)
	}
	assertBatchRecipients(t, plan.Batches[0], "other@example.com")
	if plan.Suppressed[0].Reason != "unsubscribed" || plan.Suppressed[1].Reason != "unsubscribed" {
		t.Fatalf("suppression reasons = %q, %q; want default", plan.Suppressed[0].Reason, plan.Suppressed[1].Reason)
	}
}

func TestBulkIdempotencyKeyStableAndScoped(t *testing.T) {
	t.Parallel()

	scope := BulkIdempotencyScope{
		CampaignID: "campaign-1",
		ListID:     "weekly",
	}
	first, err := BulkIdempotencyKey(scope, bulkRecipient("User@Example.com", "u1"))
	if err != nil {
		t.Fatalf("BulkIdempotencyKey(first): %v", err)
	}
	second, err := BulkIdempotencyKey(scope, bulkRecipient("user@example.com", "u1"))
	if err != nil {
		t.Fatalf("BulkIdempotencyKey(second): %v", err)
	}
	if first != second {
		t.Fatalf("case-normalized keys differ: %q != %q", first, second)
	}
	if !strings.HasPrefix(first, "bulk_email:") || len(strings.TrimPrefix(first, "bulk_email:")) != 64 {
		t.Fatalf("BulkIdempotencyKey = %q, want bulk_email: plus sha256 hex", first)
	}

	otherCampaign, err := BulkIdempotencyKey(BulkIdempotencyScope{
		CampaignID: "campaign-2",
		ListID:     "weekly",
	}, bulkRecipient("user@example.com", "u1"))
	if err != nil {
		t.Fatalf("BulkIdempotencyKey(otherCampaign): %v", err)
	}
	if otherCampaign == first {
		t.Fatal("BulkIdempotencyKey did not change across campaign scope")
	}
}

func TestBuildBulkPlanRejectsInvalidInputs(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name       string
		recipients []BulkRecipient
		opts       BulkPlanOptions
		wantText   string
	}{
		{
			name:     "empty recipients",
			wantText: "recipients",
		},
		{
			name: "invalid recipient",
			recipients: []BulkRecipient{
				bulkRecipient("not an address", "u1"),
			},
			wantText: "recipients[0]",
		},
		{
			name: "invalid throttle",
			recipients: []BulkRecipient{
				bulkRecipient("user@example.com", "u1"),
			},
			opts: BulkPlanOptions{
				DomainThrottles: map[string]BulkDomainThrottle{
					"example.com": {},
				},
			},
			wantText: "domain_throttles",
		},
		{
			name: "invalid suppression",
			recipients: []BulkRecipient{
				bulkRecipient("user@example.com", "u1"),
			},
			opts: BulkPlanOptions{
				Suppressions: []BulkSuppression{
					{Email: "bad address"},
				},
			},
			wantText: "suppressions[0].email",
		},
		{
			name: "invalid batch size",
			recipients: []BulkRecipient{
				bulkRecipient("user@example.com", "u1"),
			},
			opts:     BulkPlanOptions{BatchSize: -1},
			wantText: "batch size",
		},
	}

	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			_, err := BuildBulkPlan(tt.recipients, tt.opts)
			if !errors.Is(err, ErrInvalidBulkPlan) {
				t.Fatalf("BuildBulkPlan error = %v, want ErrInvalidBulkPlan", err)
			}
			if !strings.Contains(err.Error(), tt.wantText) {
				t.Fatalf("BuildBulkPlan error = %q, want %q", err, tt.wantText)
			}
		})
	}
}

func bulkRecipient(email, subscriberID string) BulkRecipient {
	return BulkRecipient{
		Address: Address{
			Email: email,
		},
		SubscriberID: subscriberID,
	}
}

func assertBatchRecipients(t *testing.T, batch BulkBatch, want ...string) {
	t.Helper()

	if got := len(batch.Recipients); got != len(want) {
		t.Fatalf("batch %d recipient count = %d, want %d", batch.Index, got, len(want))
	}
	for i, email := range want {
		if got := batch.Recipients[i].Recipient.Address.Email; got != email {
			t.Fatalf("batch %d recipient %d = %q, want %q", batch.Index, i, got, email)
		}
		if batch.Recipients[i].Domain == "" {
			t.Fatalf("batch %d recipient %d has empty domain", batch.Index, i)
		}
		if batch.Recipients[i].IdempotencyKey == "" {
			t.Fatalf("batch %d recipient %d has empty idempotency key", batch.Index, i)
		}
	}
}

func assertDomainSummary(t *testing.T, summary BulkDryRunSummary, want BulkDomainSummary) {
	t.Helper()

	for _, got := range summary.Domains {
		if got.Domain == want.Domain {
			if got != want {
				t.Fatalf("domain summary for %q = %+v, want %+v", want.Domain, got, want)
			}
			return
		}
	}
	t.Fatalf("domain summary for %q not found in %+v", want.Domain, summary.Domains)
}
