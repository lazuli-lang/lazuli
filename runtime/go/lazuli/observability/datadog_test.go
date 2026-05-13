package observability

import (
	"errors"
	"strings"
	"testing"
	"time"
)

func TestDatadogDescriptorNormalizeValidateAndPlan(t *testing.T) {
	descriptor := DatadogDescriptor{
		Site:           "DATADOGHQ.EU/",
		APIKey:         " invalid-api-key-placeholder ",
		AppKey:         " invalid-app-key-placeholder ",
		Service:        " Orders_API ",
		Env:            " Production ",
		Version:        " 2026.05.13 ",
		LogsEnabled:    true,
		TracesEnabled:  true,
		MetricsEnabled: true,
	}

	normalized, err := descriptor.Normalize()
	if err != nil {
		t.Fatalf("Normalize() error = %v", err)
	}
	if normalized.Site != "https://datadoghq.eu" {
		t.Fatalf("Site = %q, want https://datadoghq.eu", normalized.Site)
	}
	if normalized.Service != "orders-api" || normalized.Env != "production" || normalized.Version != "2026.05.13" {
		t.Fatalf("metadata = %q/%q/%q, want normalized values", normalized.Service, normalized.Env, normalized.Version)
	}
	if normalized.FlushInterval != DatadogDefaultFlushInterval {
		t.Fatalf("FlushInterval = %s, want default %s", normalized.FlushInterval, DatadogDefaultFlushInterval)
	}
	if err := normalized.Validate(); err != nil {
		t.Fatalf("Validate() error = %v", err)
	}

	plan, err := descriptor.Plan()
	if err != nil {
		t.Fatalf("Plan() error = %v", err)
	}
	if !plan.HasAPIKey || !plan.HasAppKey {
		t.Fatalf("key presence = %v/%v, want true/true", plan.HasAPIKey, plan.HasAppKey)
	}
	if plan.Summary.APIKey != "[redacted]" || plan.Summary.AppKey != "[redacted]" {
		t.Fatalf("summary keys = %q/%q, want redacted", plan.Summary.APIKey, plan.Summary.AppKey)
	}
}

func TestDatadogNormalizers(t *testing.T) {
	site, err := NormalizeDatadogSite("https://user:secret@DD.EXAMPLE.test/intake/?api_key=placeholder#frag")
	if err != nil {
		t.Fatalf("NormalizeDatadogSite() error = %v", err)
	}
	if site != "https://dd.example.test/intake" {
		t.Fatalf("NormalizeDatadogSite() = %q, want sanitized url", site)
	}

	if got := NormalizeDatadogService(" Billing_Worker "); got != "billing-worker" {
		t.Fatalf("NormalizeDatadogService() = %q, want billing-worker", got)
	}
	if got := NormalizeDatadogEnv(" Staging "); got != "staging" {
		t.Fatalf("NormalizeDatadogEnv() = %q, want staging", got)
	}
	if got := NormalizeDatadogVersion(" 1.2.3+build "); got != "1.2.3+build" {
		t.Fatalf("NormalizeDatadogVersion() = %q, want trimmed version", got)
	}
	if got := NormalizeDatadogFlushInterval(0); got != DatadogDefaultFlushInterval {
		t.Fatalf("NormalizeDatadogFlushInterval(0) = %s, want default", got)
	}
}

func TestDatadogDescriptorRedactedSummary(t *testing.T) {
	summary := DatadogDescriptor{
		Site:           "https://user:secret@logs.example.test/path?token=placeholder",
		APIKey:         strings.Join([]string{"invalid", "api", "key"}, "-"),
		AppKey:         strings.Join([]string{"invalid", "app", "key"}, "-"),
		Service:        " Checkout ",
		Env:            " Test ",
		LogsEnabled:    true,
		FlushInterval:  2 * time.Second,
		MetricsEnabled: true,
	}.RedactedSummary()

	if summary.Site != "https://logs.example.test/path" {
		t.Fatalf("Site = %q, want sanitized url", summary.Site)
	}
	if summary.APIKey != "[redacted]" || summary.AppKey != "[redacted]" {
		t.Fatalf("keys = %q/%q, want redacted", summary.APIKey, summary.AppKey)
	}
	if !summary.HasAPIKey || !summary.HasAppKey {
		t.Fatalf("key presence = %v/%v, want true/true", summary.HasAPIKey, summary.HasAppKey)
	}
	if !summary.LogsEnabled || !summary.MetricsEnabled || summary.TracesEnabled {
		t.Fatalf("toggles = logs:%v traces:%v metrics:%v, want true/false/true", summary.LogsEnabled, summary.TracesEnabled, summary.MetricsEnabled)
	}
}

func TestValidateDatadogDescriptorRejectsInvalidDescriptors(t *testing.T) {
	tests := []struct {
		name       string
		descriptor DatadogDescriptor
	}{
		{
			name:       "invalid site",
			descriptor: DatadogDescriptor{Site: "ftp://datadog.example.test"},
		},
		{
			name:       "enabled without api key",
			descriptor: DatadogDescriptor{MetricsEnabled: true},
		},
		{
			name: "flush interval below minimum",
			descriptor: DatadogDescriptor{
				APIKey:         "invalid-api-key-placeholder",
				MetricsEnabled: true,
				FlushInterval:  time.Millisecond,
			},
		},
		{
			name: "flush interval above maximum",
			descriptor: DatadogDescriptor{
				APIKey:         "invalid-api-key-placeholder",
				MetricsEnabled: true,
				FlushInterval:  DatadogMaxFlushInterval + time.Second,
			},
		},
		{
			name: "invalid service label",
			descriptor: DatadogDescriptor{
				APIKey:         "invalid-api-key-placeholder",
				MetricsEnabled: true,
				Service:        "orders worker",
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := ValidateDatadogDescriptor(tt.descriptor)
			if !errors.Is(err, ErrDatadogDescriptorInvalid) {
				t.Fatalf("ValidateDatadogDescriptor() error = %v, want ErrDatadogDescriptorInvalid", err)
			}
		})
	}
}

func TestDatadogRedactionHelpers(t *testing.T) {
	if got := RedactDatadogSecret(""); got != "" {
		t.Fatalf("RedactDatadogSecret(empty) = %q, want empty", got)
	}
	if got := RedactDatadogSecret("placeholder-secret"); got != "[redacted]" {
		t.Fatalf("RedactDatadogSecret(non-empty) = %q, want [redacted]", got)
	}
	if got := RedactDatadogURL("https://user:secret@example.test/path?api_key=placeholder"); got != "https://example.test/path" {
		t.Fatalf("RedactDatadogURL() = %q, want sanitized url", got)
	}
	if got := RedactDatadogURL("://bad"); got != "[redacted]" {
		t.Fatalf("RedactDatadogURL(invalid) = %q, want [redacted]", got)
	}
}
