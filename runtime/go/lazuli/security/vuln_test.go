package security

import (
	"errors"
	"reflect"
	"testing"
)

func TestNormalizeVulnerabilitySeverity(t *testing.T) {
	t.Parallel()
	tests := []struct {
		raw  string
		want VulnerabilitySeverity
	}{
		{raw: "", want: VulnerabilitySeverityUnknown},
		{raw: "NEGLIGIBLE", want: VulnerabilitySeverityNone},
		{raw: "info", want: VulnerabilitySeverityLow},
		{raw: "moderate", want: VulnerabilitySeverityMedium},
		{raw: "Important", want: VulnerabilitySeverityHigh},
		{raw: "crit", want: VulnerabilitySeverityCritical},
		{raw: "3.9", want: VulnerabilitySeverityLow},
		{raw: "4.0", want: VulnerabilitySeverityMedium},
		{raw: "7.0", want: VulnerabilitySeverityHigh},
		{raw: "9.0", want: VulnerabilitySeverityCritical},
	}
	for _, tt := range tests {
		tt := tt
		t.Run(tt.raw, func(t *testing.T) {
			t.Parallel()
			if got := NormalizeVulnerabilitySeverity(tt.raw); got != tt.want {
				t.Fatalf("NormalizeVulnerabilitySeverity(%q) = %s, want %s", tt.raw, got, tt.want)
			}
		})
	}
}

func TestParseVulnerabilityReportMergesGovulncheckLikeRecords(t *testing.T) {
	t.Parallel()
	input := []byte(`
{"config":{"scanner_name":"govulncheck"}}
{"osv":{"id":"GO-2026-0001","summary":"HTTP request smuggling","database_specific":{"severity":"HIGH"},"affected":[{"package":{"name":"golang.org/x/net"}}]}}
{"finding":{"osv":"GO-2026-0001","fixed_version":"v0.35.0","trace":[{"module":"golang.org/x/net","version":"v0.34.0"},{"package":"golang.org/x/net/http2"}]}}
`)

	report, err := ParseVulnerabilityReport(input)
	if err != nil {
		t.Fatalf("ParseVulnerabilityReport: %v", err)
	}

	want := []VulnerabilityFinding{
		{
			ID:           "GO-2026-0001",
			Summary:      "HTTP request smuggling",
			Module:       "golang.org/x/net",
			Version:      "v0.34.0",
			Package:      "golang.org/x/net/http2",
			FixedVersion: "v0.35.0",
			Severity:     VulnerabilitySeverityHigh,
		},
	}
	if !reflect.DeepEqual(report.Findings, want) {
		t.Fatalf("Findings = %#v, want %#v", report.Findings, want)
	}
}

func TestParseVulnerabilityReportAcceptsDirectArraysAndScores(t *testing.T) {
	t.Parallel()
	input := []byte(`{
		"vulns": [
			{"id":"GO-2026-0002","module":"example.com/app","package":"example.com/app/auth","severity":[{"type":"CVSS_V3","score":"9.8"}]},
			{"id":"GO-2026-0003","affected":[{"package":{"name":"example.com/lib"}}],"severity":"low"}
		]
	}`)

	report, err := ParseVulnerabilityReport(input)
	if err != nil {
		t.Fatalf("ParseVulnerabilityReport: %v", err)
	}

	want := []VulnerabilityFinding{
		{
			ID:       "GO-2026-0002",
			Module:   "example.com/app",
			Package:  "example.com/app/auth",
			Severity: VulnerabilitySeverityCritical,
		},
		{
			ID:       "GO-2026-0003",
			Module:   "example.com/lib",
			Package:  "example.com/lib",
			Severity: VulnerabilitySeverityLow,
		},
	}
	if !reflect.DeepEqual(report.Findings, want) {
		t.Fatalf("Findings = %#v, want %#v", report.Findings, want)
	}
}

func TestVulnerabilityReportAffectedModules(t *testing.T) {
	t.Parallel()
	report := VulnerabilityReport{
		Findings: []VulnerabilityFinding{
			{ID: "GO-2026-0002", Module: "example.com/app", FixedVersion: "v1.2.3", Severity: VulnerabilitySeverityMedium},
			{ID: "GO-2026-0001", Module: "example.com/app", FixedVersion: "v1.2.3", Severity: VulnerabilitySeverityHigh},
			{ID: "GO-2026-0001", Module: "example.com/app", FixedVersion: "v1.2.4", Severity: VulnerabilitySeverityHigh},
			{ID: "GO-2026-0003", Module: "example.com/lib", Severity: VulnerabilitySeverityLow},
		},
	}

	got := report.AffectedModules()
	want := []AffectedModuleSummary{
		{
			Module:          "example.com/app",
			Count:           2,
			HighestSeverity: VulnerabilitySeverityHigh,
			Vulnerabilities: []string{"GO-2026-0001", "GO-2026-0002"},
			FixedVersions:   []string{"v1.2.3", "v1.2.4"},
		},
		{
			Module:          "example.com/lib",
			Count:           1,
			HighestSeverity: VulnerabilitySeverityLow,
			Vulnerabilities: []string{"GO-2026-0003"},
		},
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("AffectedModules = %#v, want %#v", got, want)
	}
}

func TestVulnerabilityReportEvaluateFailThreshold(t *testing.T) {
	t.Parallel()
	report := VulnerabilityReport{
		Findings: []VulnerabilityFinding{
			{ID: "GO-low", Severity: VulnerabilitySeverityLow},
			{ID: "GO-high", Severity: VulnerabilitySeverityHigh},
			{ID: "GO-unknown", Severity: VulnerabilitySeverityUnknown},
		},
	}

	result := report.EvaluateFailThreshold(VulnerabilityFailThreshold{
		MinimumSeverity: VulnerabilitySeverityHigh,
	})
	if !result.Failed {
		t.Fatalf("EvaluateFailThreshold failed = false, want true")
	}
	if result.HighestSeverity != VulnerabilitySeverityHigh {
		t.Fatalf("HighestSeverity = %s, want high", result.HighestSeverity)
	}
	if got, want := findingIDs(result.Findings), []string{"GO-high"}; !reflect.DeepEqual(got, want) {
		t.Fatalf("failing IDs = %#v, want %#v", got, want)
	}

	if report.FailsThreshold(VulnerabilityFailThreshold{MinimumSeverity: VulnerabilitySeverityCritical}) {
		t.Fatalf("FailsThreshold critical = true, want false")
	}

	result = report.EvaluateFailThreshold(VulnerabilityFailThreshold{
		MinimumSeverity: VulnerabilitySeverityCritical,
		IncludeUnknown:  true,
	})
	if got, want := findingIDs(result.Findings), []string{"GO-unknown"}; !reflect.DeepEqual(got, want) {
		t.Fatalf("unknown-inclusive failing IDs = %#v, want %#v", got, want)
	}
}

func TestParseVulnerabilityReportRejectsMalformedJSON(t *testing.T) {
	t.Parallel()
	_, err := ParseVulnerabilityReport([]byte(`{"finding":`))
	if !errors.Is(err, ErrInvalidVulnerabilityReport) {
		t.Fatalf("ParseVulnerabilityReport error = %v, want ErrInvalidVulnerabilityReport", err)
	}
}

func findingIDs(findings []VulnerabilityFinding) []string {
	ids := make([]string, len(findings))
	for i, finding := range findings {
		ids[i] = finding.ID
	}
	return ids
}
