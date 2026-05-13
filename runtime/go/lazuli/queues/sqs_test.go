package queues_test

import (
	"errors"
	"reflect"
	"strings"
	"testing"
	"time"

	"lazuli.dev/runtime/lazuli/queues"
)

func TestPlanSQSQueueNormalizesDescriptorAndRedactsSummary(t *testing.T) {
	plan, err := queues.PlanSQSQueue(queues.SQSQueueDescriptor{
		URL:                       " HTTPS://SQS.US-EAST-1.AMAZONAWS.COM/123456789012/orders.fifo?Action=SendMessage ",
		ARN:                       " ARN:AWS:SQS:US-EAST-1:123456789012:orders.fifo ",
		ContentBasedDeduplication: true,
		MessageGroupID:            " tenant-42 ",
		VisibilityTimeout:         30 * time.Second,
		Delay:                     5 * time.Second,
	})
	if err != nil {
		t.Fatalf("PlanSQSQueue() error = %v", err)
	}

	wantDescriptor := queues.SQSQueueDescriptor{
		Name:                      "orders.fifo",
		URL:                       "https://sqs.us-east-1.amazonaws.com/123456789012/orders.fifo",
		ARN:                       "arn:aws:sqs:us-east-1:123456789012:orders.fifo",
		Region:                    "us-east-1",
		FIFO:                      true,
		ContentBasedDeduplication: true,
		MessageGroupID:            "tenant-42",
		VisibilityTimeout:         30 * time.Second,
		Delay:                     5 * time.Second,
	}
	if !reflect.DeepEqual(plan.Descriptor, wantDescriptor) {
		t.Fatalf("descriptor = %#v, want %#v", plan.Descriptor, wantDescriptor)
	}

	wantSummary := queues.SQSQueueSummary{
		Name:                      "orders.fifo",
		URL:                       "https://sqs.us-east-1.amazonaws.com/************/orders.fifo",
		ARN:                       "arn:aws:sqs:us-east-1:************:orders.fifo",
		Region:                    "us-east-1",
		FIFO:                      true,
		ContentBasedDeduplication: true,
		VisibilityTimeoutSeconds:  30,
		DelaySeconds:              5,
	}
	if !reflect.DeepEqual(plan.Summary, wantSummary) {
		t.Fatalf("summary = %#v, want %#v", plan.Summary, wantSummary)
	}
	if strings.Contains(plan.Summary.URL, "123456789012") || strings.Contains(plan.Summary.ARN, "123456789012") {
		t.Fatalf("summary leaked account id: %#v", plan.Summary)
	}
}

func TestNormalizeSQSQueueDescriptorInfersFromARN(t *testing.T) {
	got := queues.NormalizeSQSQueueDescriptor(queues.SQSQueueDescriptor{
		ARN:                    "arn:aws:sqs:eu-west-1:123456789012:billing",
		MessageGroupID:         "  ",
		MessageDeduplicationID: "  ",
	})

	if got.Name != "billing" {
		t.Fatalf("Name = %q, want billing", got.Name)
	}
	if got.Region != "eu-west-1" {
		t.Fatalf("Region = %q, want eu-west-1", got.Region)
	}
	if got.FIFO {
		t.Fatalf("FIFO = true, want false")
	}
	if got.MessageGroupID != "" || got.MessageDeduplicationID != "" {
		t.Fatalf("fifo metadata was not trimmed: %#v", got)
	}
}

func TestValidateSQSQueueDescriptorRejectsInvalidMetadata(t *testing.T) {
	tests := []struct {
		name string
		desc queues.SQSQueueDescriptor
		want string
	}{
		{
			name: "missing address",
			want: "name, url, or arn must be set",
		},
		{
			name: "bad queue name",
			desc: queues.SQSQueueDescriptor{Name: "bad/name"},
			want: "queue name",
		},
		{
			name: "invalid url",
			desc: queues.SQSQueueDescriptor{URL: "ftp://sqs.us-east-1.amazonaws.com/123456789012/orders"},
			want: "queue url scheme",
		},
		{
			name: "invalid arn",
			desc: queues.SQSQueueDescriptor{ARN: "arn:aws:sqs:us-east-1:not-account:orders"},
			want: "account id",
		},
		{
			name: "fifo requires group",
			desc: queues.SQSQueueDescriptor{Name: "orders.fifo"},
			want: "message group id",
		},
		{
			name: "fifo requires dedup",
			desc: queues.SQSQueueDescriptor{Name: "orders.fifo", MessageGroupID: "tenant-42"},
			want: "deduplication id",
		},
		{
			name: "standard rejects fifo metadata",
			desc: queues.SQSQueueDescriptor{Name: "orders", MessageGroupID: "tenant-42"},
			want: "standard queue",
		},
		{
			name: "visibility timeout above sqs max",
			desc: queues.SQSQueueDescriptor{Name: "orders", VisibilityTimeout: 12*time.Hour + time.Second},
			want: "visibility timeout",
		},
		{
			name: "delay above sqs max",
			desc: queues.SQSQueueDescriptor{Name: "orders", Delay: 15*time.Minute + time.Second},
			want: "delay",
		},
		{
			name: "duration needs whole seconds",
			desc: queues.SQSQueueDescriptor{Name: "orders", Delay: 1500 * time.Millisecond},
			want: "whole seconds",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := queues.ValidateSQSQueueDescriptor(tt.desc)
			if !errors.Is(err, queues.ErrSQSQueueInvalid) {
				t.Fatalf("ValidateSQSQueueDescriptor() error = %v, want ErrSQSQueueInvalid", err)
			}
			if !strings.Contains(err.Error(), tt.want) {
				t.Fatalf("ValidateSQSQueueDescriptor() error = %q, want substring %q", err, tt.want)
			}
		})
	}
}

func TestValidateSQSQueueDescriptorAcceptsFIFOWithExplicitDedup(t *testing.T) {
	desc := queues.SQSQueueDescriptor{
		Name:                   "events.fifo",
		MessageGroupID:         "tenant-42",
		MessageDeduplicationID: "event-1",
		VisibilityTimeout:      queues.SQSMaxVisibilityTimeout,
		Delay:                  queues.SQSMaxDelay,
	}
	if err := queues.ValidateSQSQueueDescriptor(desc); err != nil {
		t.Fatalf("ValidateSQSQueueDescriptor() error = %v", err)
	}
}

func TestSQSQueueDescriptorRedactedSummaryOnInvalidInput(t *testing.T) {
	summary := (queues.SQSQueueDescriptor{
		URL: "https://sqs.us-east-1.amazonaws.com/123456789012/bad/name",
	}).RedactedSummary()

	if strings.Contains(summary.URL, "123456789012") {
		t.Fatalf("RedactedSummary() leaked account id: %#v", summary)
	}
}
