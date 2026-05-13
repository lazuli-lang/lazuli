package observability

import (
	"errors"
	"reflect"
	"strings"
	"testing"
	"time"
)

func TestPlanPrometheusScrapeNormalizesDefaultsAndMetadata(t *testing.T) {
	labels := map[string]string{
		" service-name ": " api ",
		"empty":          " ",
	}
	annotations := map[string]string{
		" prometheus.io/scrape ": " true ",
	}

	plan, err := PlanPrometheusScrape(PrometheusScrapeDescriptor{
		Port:              8080,
		Labels:            labels,
		Annotations:       annotations,
		HistogramsEnabled: true,
	})
	if err != nil {
		t.Fatalf("PlanPrometheusScrape() error = %v", err)
	}

	if plan.Path != DefaultPrometheusScrapePath {
		t.Fatalf("Path = %q, want %q", plan.Path, DefaultPrometheusScrapePath)
	}
	if plan.Scheme != DefaultPrometheusScrapeScheme {
		t.Fatalf("Scheme = %q, want %q", plan.Scheme, DefaultPrometheusScrapeScheme)
	}
	if plan.Interval != DefaultPrometheusScrapeInterval {
		t.Fatalf("Interval = %s, want %s", plan.Interval, DefaultPrometheusScrapeInterval)
	}
	if plan.Timeout != DefaultPrometheusScrapeTimeout {
		t.Fatalf("Timeout = %s, want %s", plan.Timeout, DefaultPrometheusScrapeTimeout)
	}
	if !plan.HistogramsEnabled {
		t.Fatal("HistogramsEnabled = false, want true")
	}

	wantLabels := map[string]string{"service_name": "api"}
	if !reflect.DeepEqual(plan.Labels, wantLabels) {
		t.Fatalf("Labels = %#v, want %#v", plan.Labels, wantLabels)
	}
	wantAnnotations := map[string]string{"prometheus.io/scrape": "true"}
	if !reflect.DeepEqual(plan.Annotations, wantAnnotations) {
		t.Fatalf("Annotations = %#v, want %#v", plan.Annotations, wantAnnotations)
	}

	labels[" service-name "] = "mutated"
	annotations[" prometheus.io/scrape "] = "mutated"
	if !reflect.DeepEqual(plan.Labels, wantLabels) {
		t.Fatalf("PlanPrometheusScrape aliases labels: %#v", plan.Labels)
	}
	if !reflect.DeepEqual(plan.Annotations, wantAnnotations) {
		t.Fatalf("PlanPrometheusScrape aliases annotations: %#v", plan.Annotations)
	}
}

func TestPrometheusScrapeDescriptorRejectsInvalidInputs(t *testing.T) {
	tests := []struct {
		name       string
		descriptor PrometheusScrapeDescriptor
		fragment   string
	}{
		{
			name:       "relative path",
			descriptor: PrometheusScrapeDescriptor{Path: "metrics", Port: 8080},
			fragment:   "path",
		},
		{
			name:       "path query",
			descriptor: PrometheusScrapeDescriptor{Path: "/metrics?debug=true", Port: 8080},
			fragment:   "query",
		},
		{
			name:       "zero port",
			descriptor: PrometheusScrapeDescriptor{Port: 0},
			fragment:   "port",
		},
		{
			name:       "high port",
			descriptor: PrometheusScrapeDescriptor{Port: 70000},
			fragment:   "port",
		},
		{
			name:       "bad scheme",
			descriptor: PrometheusScrapeDescriptor{Port: 8080, Scheme: "grpc"},
			fragment:   "scheme",
		},
		{
			name:       "bad label",
			descriptor: PrometheusScrapeDescriptor{Port: 8080, Labels: map[string]string{"9bad": "value"}},
			fragment:   "label",
		},
		{
			name:       "bad annotation",
			descriptor: PrometheusScrapeDescriptor{Port: 8080, Annotations: map[string]string{"bad key": "value"}},
			fragment:   "annotation",
		},
		{
			name:       "short interval",
			descriptor: PrometheusScrapeDescriptor{Port: 8080, Interval: time.Millisecond},
			fragment:   "interval",
		},
		{
			name:       "long interval",
			descriptor: PrometheusScrapeDescriptor{Port: 8080, Interval: 25 * time.Hour},
			fragment:   "interval",
		},
		{
			name:       "short timeout",
			descriptor: PrometheusScrapeDescriptor{Port: 8080, Timeout: time.Millisecond},
			fragment:   "timeout",
		},
		{
			name:       "long timeout",
			descriptor: PrometheusScrapeDescriptor{Port: 8080, Timeout: 6 * time.Minute},
			fragment:   "timeout",
		},
		{
			name: "timeout exceeds interval",
			descriptor: PrometheusScrapeDescriptor{
				Port:     8080,
				Interval: 5 * time.Second,
				Timeout:  10 * time.Second,
			},
			fragment: "timeout",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := ValidatePrometheusScrapeDescriptor(tt.descriptor)
			if !errors.Is(err, ErrPrometheusScrapeInvalid) {
				t.Fatalf("ValidatePrometheusScrapeDescriptor() error = %v, want ErrPrometheusScrapeInvalid", err)
			}
			if !strings.Contains(err.Error(), tt.fragment) {
				t.Fatalf("ValidatePrometheusScrapeDescriptor() error = %q, want fragment %q", err, tt.fragment)
			}
		})
	}
}

func TestPlanPrometheusScrapeAcceptsBoundsAndHTTPS(t *testing.T) {
	plan, err := PlanPrometheusScrape(PrometheusScrapeDescriptor{
		Path:     " /internal/metrics ",
		Port:     65535,
		Scheme:   " HTTPS ",
		Interval: MinPrometheusScrapeInterval,
		Timeout:  MinPrometheusScrapeTimeout,
	})
	if err != nil {
		t.Fatalf("PlanPrometheusScrape() error = %v", err)
	}
	if plan.Path != "/internal/metrics" {
		t.Fatalf("Path = %q, want /internal/metrics", plan.Path)
	}
	if plan.Scheme != "https" {
		t.Fatalf("Scheme = %q, want https", plan.Scheme)
	}
	if plan.Port != 65535 {
		t.Fatalf("Port = %d, want 65535", plan.Port)
	}
}

func TestPrometheusScrapeSafeSummaryRedactsSensitiveMetadata(t *testing.T) {
	plan, err := PlanPrometheusScrape(PrometheusScrapeDescriptor{
		Path:   "/metrics",
		Port:   9090,
		Scheme: "http",
		Labels: map[string]string{
			"service":    "api",
			"token_hint": "not-a-real-token",
		},
		Annotations: map[string]string{
			"dashboard_url": "https://example.invalid/dashboard?token=placeholder",
			"owner":         "platform",
		},
		Interval:          15 * time.Second,
		Timeout:           5 * time.Second,
		HistogramsEnabled: true,
	})
	if err != nil {
		t.Fatalf("PlanPrometheusScrape() error = %v", err)
	}

	summary := plan.SafeSummary()
	if summary.Labels["service"] != "api" {
		t.Fatalf("service label = %q, want api", summary.Labels["service"])
	}
	if summary.Labels["token_hint"] != "[redacted]" {
		t.Fatalf("token label = %q, want [redacted]", summary.Labels["token_hint"])
	}
	if summary.Annotations["dashboard_url"] != "[redacted]" {
		t.Fatalf("dashboard_url = %q, want [redacted]", summary.Annotations["dashboard_url"])
	}
	if summary.Annotations["owner"] != "platform" {
		t.Fatalf("owner annotation = %q, want platform", summary.Annotations["owner"])
	}
	if summary.Interval != "15s" || summary.Timeout != "5s" {
		t.Fatalf("summary durations = %q/%q, want 15s/5s", summary.Interval, summary.Timeout)
	}
	if !summary.HistogramsEnabled {
		t.Fatal("HistogramsEnabled = false, want true")
	}

	summary.Labels["service"] = "mutated"
	if plan.Labels["service"] != "api" {
		t.Fatalf("SafeSummary aliases plan labels: %#v", plan.Labels)
	}
}
