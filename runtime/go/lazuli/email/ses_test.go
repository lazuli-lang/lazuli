package email

import (
	"errors"
	"strings"
	"testing"
)

func TestAmazonSESDescriptorNormalize(t *testing.T) {
	t.Parallel()

	descriptor := AmazonSESDescriptor{
		Region:           " US-East-1 ",
		IdentityARN:      " ARN:AWS:SES:US-East-1:123456789012:identity/Example.COM ",
		SourceARN:        " arn:aws:ses:US-East-1:123456789012:identity/sender@example.com ",
		Sender:           " Acme <Sender@Example.COM> ",
		Sandbox:          true,
		ConfigurationSet: " transactional ",
		Tags: []AmazonSESTag{
			{Name: " tenant ", Value: " lazuli "},
			{Name: " campaign ", Value: " welcome "},
		},
	}

	normalized := descriptor.Normalize()
	if normalized.Region != "us-east-1" {
		t.Fatalf("Region = %q, want us-east-1", normalized.Region)
	}
	if normalized.IdentityARN != "arn:aws:ses:us-east-1:123456789012:identity/Example.COM" {
		t.Fatalf("IdentityARN = %q", normalized.IdentityARN)
	}
	if normalized.SourceARN != "arn:aws:ses:us-east-1:123456789012:identity/sender@example.com" {
		t.Fatalf("SourceARN = %q", normalized.SourceARN)
	}
	if normalized.Sender != "Acme <Sender@Example.COM>" {
		t.Fatalf("Sender = %q", normalized.Sender)
	}
	if normalized.ConfigurationSet != "transactional" {
		t.Fatalf("ConfigurationSet = %q", normalized.ConfigurationSet)
	}
	if len(normalized.Tags) != 2 || normalized.Tags[0].Name != "campaign" || normalized.Tags[1].Name != "tenant" {
		t.Fatalf("Tags = %+v, want sorted normalized tags", normalized.Tags)
	}
}

func TestPlanAmazonSESDescriptorBuildsDryRunPlan(t *testing.T) {
	t.Parallel()

	plan, err := PlanAmazonSESDescriptor(validAmazonSESDescriptor())
	if err != nil {
		t.Fatalf("PlanAmazonSESDescriptor() error = %v", err)
	}
	if plan.Region != "us-east-1" || plan.IdentityARN == "" || plan.SourceARN == "" {
		t.Fatalf("plan metadata = %+v", plan)
	}
	if !plan.Sandbox {
		t.Fatalf("Sandbox = false, want true")
	}
	if plan.ConfigurationSet != "transactional" {
		t.Fatalf("ConfigurationSet = %q", plan.ConfigurationSet)
	}
	if len(plan.Tags) != 2 || plan.Tags[0].Name != "campaign" || plan.Tags[1].Name != "tenant" {
		t.Fatalf("Tags = %+v, want deterministic tag order", plan.Tags)
	}
	if plan.Summary.Provider != "amazon_ses" || plan.Summary.TagCount != 2 {
		t.Fatalf("Summary = %+v", plan.Summary)
	}
}

func TestAmazonSESDescriptorRedactedSummary(t *testing.T) {
	t.Parallel()

	summary := validAmazonSESDescriptor().RedactedSummary()
	if strings.Contains(summary.IdentityARN, "123456789012") {
		t.Fatalf("IdentityARN = %q, want account redacted", summary.IdentityARN)
	}
	if strings.Contains(summary.SourceARN, "123456789012") {
		t.Fatalf("SourceARN = %q, want account redacted", summary.SourceARN)
	}
	if summary.Sender != `"Acme Ops" <***@example.com>` {
		t.Fatalf("Sender = %q, want redacted local part", summary.Sender)
	}
	if summary.Sandbox != true || summary.ConfigurationSet != "transactional" || summary.TagCount != 2 {
		t.Fatalf("Summary = %+v", summary)
	}
}

func TestValidateAmazonSESDescriptorRejectsInvalidShape(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name     string
		mutate   func(*AmazonSESDescriptor)
		wantText string
	}{
		{
			name: "invalid region",
			mutate: func(d *AmazonSESDescriptor) {
				d.Region = "east"
			},
			wantText: "region",
		},
		{
			name: "identity missing",
			mutate: func(d *AmazonSESDescriptor) {
				d.IdentityARN = ""
			},
			wantText: "identity_arn",
		},
		{
			name: "identity wrong service",
			mutate: func(d *AmazonSESDescriptor) {
				d.IdentityARN = "arn:aws:sns:us-east-1:123456789012:identity/example.com"
			},
			wantText: "service",
		},
		{
			name: "source region mismatch",
			mutate: func(d *AmazonSESDescriptor) {
				d.SourceARN = "arn:aws:ses:us-west-2:123456789012:identity/example.com"
			},
			wantText: "does not match",
		},
		{
			name: "invalid sender",
			mutate: func(d *AmazonSESDescriptor) {
				d.Sender = "not an address"
			},
			wantText: "sender",
		},
		{
			name: "invalid configuration set",
			mutate: func(d *AmazonSESDescriptor) {
				d.ConfigurationSet = "with space"
			},
			wantText: "configuration_set",
		},
		{
			name: "duplicate tag",
			mutate: func(d *AmazonSESDescriptor) {
				d.Tags = append(d.Tags, AmazonSESTag{Name: "tenant", Value: "other"})
			},
			wantText: "duplicates",
		},
		{
			name: "tag name whitespace",
			mutate: func(d *AmazonSESDescriptor) {
				d.Tags = []AmazonSESTag{{Name: "bad tag", Value: "value"}}
			},
			wantText: "tags[0].name",
		},
	}

	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			descriptor := validAmazonSESDescriptor()
			tt.mutate(&descriptor)

			err := ValidateAmazonSESDescriptor(descriptor)
			if !errors.Is(err, ErrInvalidAmazonSESDescriptor) {
				t.Fatalf("ValidateAmazonSESDescriptor() error = %v, want ErrInvalidAmazonSESDescriptor", err)
			}
			if !strings.Contains(err.Error(), tt.wantText) {
				t.Fatalf("ValidateAmazonSESDescriptor() error = %q, want %q", err, tt.wantText)
			}
		})
	}
}

func validAmazonSESDescriptor() AmazonSESDescriptor {
	return AmazonSESDescriptor{
		Region:           "us-east-1",
		IdentityARN:      "arn:aws:ses:us-east-1:123456789012:identity/example.com",
		SourceARN:        "arn:aws:ses:us-east-1:123456789012:identity/sender@example.com",
		Sender:           "Acme Ops <sender@example.com>",
		Sandbox:          true,
		ConfigurationSet: "transactional",
		Tags: []AmazonSESTag{
			{Name: "tenant", Value: "lazuli"},
			{Name: "campaign", Value: "welcome"},
		},
	}
}
