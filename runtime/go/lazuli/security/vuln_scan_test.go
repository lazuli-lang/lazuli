package security

import (
	"errors"
	"reflect"
	"testing"
)

func TestVulnerabilityScannerCommandNormalizesMetadata(t *testing.T) {
	t.Parallel()

	command := VulnerabilityScannerCommand{
		Command:    []string{" /usr/local/bin/govulncheck ", " -json "},
		WorkingDir: " ./runtime/go ",
		Environment: []VulnerabilityScannerEnvVar{
			VulnerabilityScannerEnv("GOFLAGS", "-mod=readonly"),
			VulnerabilityScannerEnv("CGO_ENABLED", "0"),
		},
		OutputFormat: " JSON ",
	}

	got, err := command.Normalized()
	if err != nil {
		t.Fatalf("Normalized() error = %v", err)
	}

	want := VulnerabilityScannerCommand{
		Name:       "govulncheck",
		Command:    []string{"/usr/local/bin/govulncheck", "-json"},
		WorkingDir: "./runtime/go",
		Environment: []VulnerabilityScannerEnvVar{
			{Name: "CGO_ENABLED", Value: "0"},
			{Name: "GOFLAGS", Value: "-mod=readonly"},
		},
		OutputFormat: "json",
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("Normalized() = %#v, want %#v", got, want)
	}
}

func TestVulnerabilityScannerCommandRejectsInvalidMetadata(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name    string
		command VulnerabilityScannerCommand
	}{
		{
			name: "empty command token",
			command: VulnerabilityScannerCommand{
				Command: []string{" "},
			},
		},
		{
			name: "duplicate environment",
			command: VulnerabilityScannerCommand{
				Name: "scanner",
				Environment: []VulnerabilityScannerEnvVar{
					VulnerabilityScannerEnv("GOFLAGS", "-mod=readonly"),
					VulnerabilityScannerEnv("GOFLAGS", "-race"),
				},
			},
		},
		{
			name: "invalid environment name",
			command: VulnerabilityScannerCommand{
				Name:        "scanner",
				Environment: []VulnerabilityScannerEnvVar{VulnerabilityScannerEnv("1BAD", "value")},
			},
		},
		{
			name: "metadata without name or command",
			command: VulnerabilityScannerCommand{
				OutputFormat: "json",
			},
		},
	}

	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			if err := tt.command.Validate(); !errors.Is(err, ErrInvalidVulnerabilityScanConfig) {
				t.Fatalf("Validate() error = %v, want ErrInvalidVulnerabilityScanConfig", err)
			}
		})
	}
}

func TestNormalizeVulnerabilityScanReportAppliesIgnoreRules(t *testing.T) {
	t.Parallel()

	report := VulnerabilityReport{
		Findings: []VulnerabilityFinding{
			{ID: " GO-2026-0001 ", Module: " example.com/app ", Package: " example.com/app/http ", Severity: VulnerabilitySeverityHigh},
			{ID: "GO-2026-0002", Module: "example.com/app", Package: "example.com/app/db", Severity: VulnerabilitySeverityCritical},
			{ID: "GO-2026-0002", Module: "example.com/app", Package: "example.com/app/db", Severity: VulnerabilitySeverityCritical},
			{},
		},
	}
	policy := VulnerabilityScanPolicy{
		Scanner: VulnerabilityScannerCommand{Name: "govulncheck"},
		IgnoreRules: []VulnerabilityScanIgnoreRule{
			{
				ID:      " GO-2026-0001 ",
				Package: " example.com/app/http ",
				Reason:  " accepted until patched ",
			},
		},
	}

	got, err := NormalizeVulnerabilityScanReport(report, policy)
	if err != nil {
		t.Fatalf("NormalizeVulnerabilityScanReport() error = %v", err)
	}

	want := VulnerabilityScanReport{
		Scanner: VulnerabilityScannerCommand{Name: "govulncheck"},
		Findings: []VulnerabilityFinding{
			{ID: "GO-2026-0002", Module: "example.com/app", Package: "example.com/app/db", Severity: VulnerabilitySeverityCritical},
		},
		Ignored: []VulnerabilityIgnoredFinding{
			{
				Finding: VulnerabilityFinding{
					ID:       "GO-2026-0001",
					Module:   "example.com/app",
					Package:  "example.com/app/http",
					Severity: VulnerabilitySeverityHigh,
				},
				Rule: VulnerabilityScanIgnoreRule{
					ID:      "GO-2026-0001",
					Package: "example.com/app/http",
					Reason:  "accepted until patched",
				},
				Reason: "accepted until patched",
			},
		},
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("NormalizeVulnerabilityScanReport() = %#v, want %#v", got, want)
	}
}

func TestDecideVulnerabilityScanHonorsThresholdAndIgnores(t *testing.T) {
	t.Parallel()

	report := VulnerabilityReport{
		Findings: []VulnerabilityFinding{
			{ID: "GO-critical", Module: "example.com/app", Severity: VulnerabilitySeverityCritical},
			{ID: "GO-medium", Module: "example.com/lib", Severity: VulnerabilitySeverityMedium},
			{ID: "GO-unknown", Module: "example.com/unknown", Severity: VulnerabilitySeverityUnknown},
		},
	}

	passDecision, err := DecideVulnerabilityScan(report, VulnerabilityScanPolicy{
		Threshold: VulnerabilityScanThreshold{MinimumSeverity: VulnerabilitySeverityHigh},
		IgnoreRules: []VulnerabilityScanIgnoreRule{
			{ID: "GO-critical", Reason: "temporary waiver"},
		},
	})
	if err != nil {
		t.Fatalf("DecideVulnerabilityScan(pass) error = %v", err)
	}
	if !passDecision.Passed || passDecision.Failed {
		t.Fatalf("pass decision passed/failed = %v/%v, want true/false", passDecision.Passed, passDecision.Failed)
	}
	if got := findingIDs(passDecision.FailingFindings); len(got) != 0 {
		t.Fatalf("pass decision failing IDs = %#v, want none", got)
	}
	if got, want := len(passDecision.IgnoredFindings), 1; got != want {
		t.Fatalf("pass decision ignored findings = %d, want %d", got, want)
	}

	failDecision, err := DecideVulnerabilityScan(report, VulnerabilityScanPolicy{
		Threshold: VulnerabilityScanThreshold{
			MinimumSeverity: VulnerabilitySeverityCritical,
			IncludeUnknown:  true,
		},
		IgnoreRules: []VulnerabilityScanIgnoreRule{
			{ID: "GO-critical", Reason: "temporary waiver"},
		},
	})
	if err != nil {
		t.Fatalf("DecideVulnerabilityScan(fail) error = %v", err)
	}
	if failDecision.Passed || !failDecision.Failed {
		t.Fatalf("fail decision passed/failed = %v/%v, want false/true", failDecision.Passed, failDecision.Failed)
	}
	if got, want := findingIDs(failDecision.FailingFindings), []string{"GO-unknown"}; !reflect.DeepEqual(got, want) {
		t.Fatalf("fail decision failing IDs = %#v, want %#v", got, want)
	}
}

func TestVulnerabilityScanPolicyRejectsInvalidRulesAndThresholds(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name   string
		policy VulnerabilityScanPolicy
	}{
		{
			name:   "empty ignore rule",
			policy: VulnerabilityScanPolicy{IgnoreRules: []VulnerabilityScanIgnoreRule{{Reason: "no selector"}}},
		},
		{
			name: "invalid ignore severity",
			policy: VulnerabilityScanPolicy{
				IgnoreRules: []VulnerabilityScanIgnoreRule{{Severity: VulnerabilitySeverity(99)}},
			},
		},
		{
			name: "invalid threshold",
			policy: VulnerabilityScanPolicy{
				Threshold: VulnerabilityScanThreshold{MinimumSeverity: VulnerabilitySeverity(99)},
			},
		},
	}

	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			if err := tt.policy.Validate(); !errors.Is(err, ErrInvalidVulnerabilityScanConfig) {
				t.Fatalf("Validate() error = %v, want ErrInvalidVulnerabilityScanConfig", err)
			}
		})
	}
}
