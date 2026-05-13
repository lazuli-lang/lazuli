package testkit_test

import (
	"errors"
	"fmt"
	"reflect"
	"testing"
	"testing/fstest"

	"lazuli.dev/runtime/lazuli/testkit"
)

func TestCoverageSummaryAggregatesPackagesFilesAndFunctions(t *testing.T) {
	summary := coverageFixtureSummary(t)

	if summary.Mode != "set" {
		t.Fatalf("summary mode = %q, want set", summary.Mode)
	}
	if summary.Total != (testkit.CoverageTotal{CoveredStatements: 3, Statements: 6}) {
		t.Fatalf("summary total = %#v, want 3/6", summary.Total)
	}

	wantPackages := []testkit.CoveragePackageSummary{
		{Package: "pkg/order", Total: testkit.CoverageTotal{CoveredStatements: 3, Statements: 4}},
		{Package: "pkg/user", Total: testkit.CoverageTotal{CoveredStatements: 0, Statements: 2}},
	}
	if !reflect.DeepEqual(summary.Packages, wantPackages) {
		t.Fatalf("summary packages = %#v, want %#v", summary.Packages, wantPackages)
	}

	wantFiles := []testkit.CoverageFileSummary{
		{Package: "pkg/order", File: "pkg/order/order.go", Total: testkit.CoverageTotal{CoveredStatements: 3, Statements: 4}},
		{Package: "pkg/user", File: "pkg/user/user.go", Total: testkit.CoverageTotal{CoveredStatements: 0, Statements: 2}},
	}
	if !reflect.DeepEqual(summary.Files, wantFiles) {
		t.Fatalf("summary files = %#v, want %#v", summary.Files, wantFiles)
	}

	wantFunctions := []testkit.CoverageFunctionSummary{
		{Package: "pkg/order", File: "pkg/order/order.go", Name: "Covered", StartLine: 3, Total: testkit.CoverageTotal{CoveredStatements: 1, Statements: 1}},
		{Package: "pkg/order", File: "pkg/order/order.go", Name: "Partial", StartLine: 7, Total: testkit.CoverageTotal{CoveredStatements: 2, Statements: 3}},
		{Package: "pkg/user", File: "pkg/user/user.go", Name: "Build", StartLine: 3, Total: testkit.CoverageTotal{CoveredStatements: 0, Statements: 2}},
	}
	if !reflect.DeepEqual(summary.Functions, wantFunctions) {
		t.Fatalf("summary functions = %#v, want %#v", summary.Functions, wantFunctions)
	}
	if got := summary.Functions[1].FullName(); got != "pkg/order.Partial" {
		t.Fatalf("function full name = %q, want pkg/order.Partial", got)
	}
}

func TestCoverageThresholdEvaluationReportsDeterministicFailures(t *testing.T) {
	summary := coverageFixtureSummary(t)
	threshold := testkit.CoverageThreshold{
		Total: 55,
		Packages: map[string]float64{
			"pkg/order": 75,
			"pkg/user":  1,
		},
		Files: map[string]float64{
			"pkg/missing.go": 80,
		},
		Functions: map[string]float64{
			"pkg/order.Covered": 100,
			"pkg/order.Partial": 70,
		},
	}

	result := summary.EvaluateThreshold(threshold)
	if result.Passed {
		t.Fatalf("EvaluateThreshold passed = true, want false")
	}
	if summary.MeetsThreshold(threshold) {
		t.Fatalf("MeetsThreshold = true, want false")
	}

	got := coverageFailureLines(result)
	want := []string{
		"total|total|50.0|55.0|3/6|false",
		"package|pkg/user|0.0|1.0|0/2|false",
		"file|pkg/missing.go|missing|80.0|0/0|true",
		"function|pkg/order.Partial|66.7|70.0|2/3|false",
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("threshold failures = %#v, want %#v", got, want)
	}
}

func TestCoverageReportsRenderDeterministically(t *testing.T) {
	summary := coverageFixtureSummary(t)
	options := testkit.CoverageReportOptions{
		Threshold: testkit.CoverageThreshold{
			Total: 55,
			Packages: map[string]float64{
				"pkg/user": 1,
			},
			Files: map[string]float64{
				"pkg/missing.go": 80,
			},
			Functions: map[string]float64{
				"pkg/order.Partial": 70,
			},
		},
	}

	gotMarkdown := summary.Markdown(options)
	wantMarkdown := `# Coverage Report

Mode: ` + "`set`" + `

| Scope | Covered | Statements | Coverage |
| --- | ---: | ---: | ---: |
| Total | 3 | 6 | 50.0% |

## Packages

| Package | Covered | Statements | Coverage |
| --- | ---: | ---: | ---: |
| pkg/order | 3 | 4 | 75.0% |
| pkg/user | 0 | 2 | 0.0% |

## Files

| File | Package | Covered | Statements | Coverage |
| --- | --- | ---: | ---: | ---: |
| pkg/order/order.go | pkg/order | 3 | 4 | 75.0% |
| pkg/user/user.go | pkg/user | 0 | 2 | 0.0% |

## Functions

| Function | File | Covered | Statements | Coverage |
| --- | --- | ---: | ---: | ---: |
| pkg/order.Covered | pkg/order/order.go | 1 | 1 | 100.0% |
| pkg/order.Partial | pkg/order/order.go | 2 | 3 | 66.7% |
| pkg/user.Build | pkg/user/user.go | 0 | 2 | 0.0% |

## Thresholds

Failed.

| Scope | Name | Actual | Minimum |
| --- | --- | ---: | ---: |
| Total | total | 50.0% | 55.0% |
| Package | pkg/user | 0.0% | 1.0% |
| File | pkg/missing.go | missing | 80.0% |
| Function | pkg/order.Partial | 66.7% | 70.0% |
`
	if gotMarkdown != wantMarkdown {
		t.Fatalf("Markdown mismatch\nwant:\n%s\ngot:\n%s", wantMarkdown, gotMarkdown)
	}

	gotText := testkit.RenderCoverageText(summary, options)
	wantText := `Coverage Report
Mode: set
Total: 3/6 statements (50.0%)

Packages:
- pkg/order: 3/4 statements (75.0%)
- pkg/user: 0/2 statements (0.0%)

Files:
- pkg/order/order.go [pkg/order]: 3/4 statements (75.0%)
- pkg/user/user.go [pkg/user]: 0/2 statements (0.0%)

Functions:
- pkg/order.Covered [pkg/order/order.go]: 1/1 statements (100.0%)
- pkg/order.Partial [pkg/order/order.go]: 2/3 statements (66.7%)
- pkg/user.Build [pkg/user/user.go]: 0/2 statements (0.0%)

Thresholds:
Failed.
- Total: 50.0% below 55.0%
- Package pkg/user: 0.0% below 1.0%
- File pkg/missing.go: missing below 80.0%
- Function pkg/order.Partial: 66.7% below 70.0%
`
	if gotText != wantText {
		t.Fatalf("Text mismatch\nwant:\n%s\ngot:\n%s", wantText, gotText)
	}
}

func TestParseCoverageProfileRejectsInvalidProfiles(t *testing.T) {
	tests := []struct {
		name    string
		profile string
	}{
		{name: "empty"},
		{name: "bad mode", profile: "mode: html\n"},
		{name: "bad block", profile: "mode: set\nnot a block\n"},
		{name: "backwards range", profile: "mode: set\npkg/a.go:2.1,1.1 1 0\n"},
		{name: "zero statements", profile: "mode: set\npkg/a.go:1.1,1.2 0 0\n"},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			_, err := testkit.ParseCoverageProfile([]byte(tt.profile))
			if !errors.Is(err, testkit.ErrInvalidCoverageProfile) {
				t.Fatalf("ParseCoverageProfile() error = %v, want ErrInvalidCoverageProfile", err)
			}
		})
	}
}

func coverageFixtureSummary(t *testing.T) testkit.CoverageSummary {
	t.Helper()

	summary, err := testkit.SummarizeCoverageProfile([]byte(coverageFixtureProfile), testkit.CoverageSummaryOptions{
		Source: coverageFixtureSource(),
	})
	if err != nil {
		t.Fatalf("SummarizeCoverageProfile() error = %v", err)
	}
	return summary
}

func coverageFailureLines(result testkit.CoverageThresholdResult) []string {
	lines := make([]string, 0, len(result.Failures))
	for _, failure := range result.Failures {
		actual := "missing"
		if !failure.Missing {
			actual = fmt.Sprintf("%.1f", failure.Actual)
		}
		lines = append(lines, fmt.Sprintf(
			"%s|%s|%s|%.1f|%d/%d|%t",
			failure.Scope,
			failure.Name,
			actual,
			failure.Minimum,
			failure.Coverage.CoveredStatements,
			failure.Coverage.Statements,
			failure.Missing,
		))
	}
	return lines
}

func coverageFixtureSource() fstest.MapFS {
	return fstest.MapFS{
		"pkg/order/order.go": {
			Data: []byte(`package order

func Covered() int {
	return 1
}

func Partial(flag bool) int {
	if flag {
		return 1
	}
	return 0
}
`),
		},
		"pkg/user/user.go": {
			Data: []byte(`package user

func Build() string {
	return "x"
}
`),
		},
	}
}

const coverageFixtureProfile = `mode: set
pkg/user/user.go:4.1,4.12 2 0
pkg/order/order.go:8.1,8.10 1 1
pkg/order/order.go:9.1,9.11 1 0
pkg/order/order.go:4.1,4.10 1 1
pkg/order/order.go:11.1,11.10 1 1
`
