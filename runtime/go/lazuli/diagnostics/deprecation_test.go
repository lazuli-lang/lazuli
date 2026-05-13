package diagnostics_test

import (
	"reflect"
	"testing"
	"time"

	"lazuli.dev/runtime/lazuli/diagnostics"
)

func TestBuildDeprecationReportNormalizesEntriesAndGroupsBySeverity(t *testing.T) {
	t.Parallel()

	now := time.Date(2026, 5, 12, 15, 30, 0, 0, time.FixedZone("test", -3*60*60))
	inputs := []diagnostics.DeprecationReportInput{
		{
			Surface: " customer.command.archive ",
			Metadata: diagnostics.DeprecationMetadata{
				Since:       " 2026.04 ",
				Replacement: " customer.command.archive_v2 ",
				Sunset:      " 2026-12-31 ",
			},
			Location: diagnostics.DeprecationLocation{Path: " b.lzi ", Line: 10, Column: 5},
			Severity: diagnostics.SeverityInfo,
		},
		{
			Surface:  "customer.command.reassign",
			Metadata: diagnostics.DeprecationMetadata{Sunset: "2026-01-01"},
			Location: diagnostics.DeprecationLocation{Path: "a.lzi", Line: 3, Column: 2},
			Severity: diagnostics.SeverityHint,
		},
		{
			Surface:  "customer.command.legacy",
			Metadata: diagnostics.DeprecationMetadata{Sunset: "2026/12/31"},
			Location: diagnostics.DeprecationLocation{Path: "a.lzi", Line: 2, Column: -1},
		},
	}

	report := diagnostics.BuildDeprecationReport(now, inputs)
	if inputs[0].Surface != " customer.command.archive " {
		t.Fatal("BuildDeprecationReport mutated input")
	}

	if got := deprecationTestEntrySummaries(report.Entries); !reflect.DeepEqual(got, []deprecationEntrySummary{
		{
			surface:  "customer.command.legacy",
			severity: diagnostics.SeverityError,
			path:     "a.lzi",
			line:     2,
			column:   0,
			deadline: "2026/12/31",
			status:   diagnostics.DeprecationDeadlineInvalid,
		},
		{
			surface:  "customer.command.reassign",
			severity: diagnostics.SeverityWarning,
			path:     "a.lzi",
			line:     3,
			column:   2,
			deadline: "2026-01-01",
			status:   diagnostics.DeprecationDeadlinePassed,
		},
		{
			surface:     "customer.command.archive",
			severity:    diagnostics.SeverityInfo,
			path:        "b.lzi",
			line:        10,
			column:      5,
			since:       "2026.04",
			replacement: "customer.command.archive_v2",
			deadline:    "2026-12-31",
			status:      diagnostics.DeprecationDeadlinePending,
		},
	}) {
		t.Fatalf("entries = %#v", got)
	}

	if report.Entries[2].Message != "customer.command.archive is deprecated since 2026.04; use customer.command.archive_v2 instead; deadline 2026-12-31." {
		t.Fatalf("archive message = %q", report.Entries[2].Message)
	}

	if keys := deprecationTestGroupKeys(report.Groups); !reflect.DeepEqual(keys, []string{"error", "warning", "info"}) {
		t.Fatalf("group keys = %v", keys)
	}
	if surfaces := deprecationTestGroupSurfaces(report.Groups[0]); !reflect.DeepEqual(surfaces, []string{"customer.command.legacy"}) {
		t.Fatalf("error group surfaces = %v", surfaces)
	}
}

func TestBuildDeprecationReportUsesOverrideMessageAndTodayIsPending(t *testing.T) {
	t.Parallel()

	now := time.Date(2026, 5, 12, 23, 59, 0, 0, time.FixedZone("test", 9*60*60))
	report := diagnostics.BuildDeprecationReport(now, []diagnostics.DeprecationReportInput{
		{
			Surface:  "customer.command.capture",
			Metadata: diagnostics.DeprecationMetadata{Sunset: "2026-05-12"},
			Severity: diagnostics.SeverityHint,
			Message:  "Use customer.command.capture_v2.",
		},
	})

	if got := len(report.Entries); got != 1 {
		t.Fatalf("entries = %d, want 1", got)
	}
	entry := report.Entries[0]
	if entry.DeadlineStatus != diagnostics.DeprecationDeadlinePending {
		t.Fatalf("deadline status = %s, want pending", entry.DeadlineStatus)
	}
	if entry.Severity != diagnostics.SeverityHint {
		t.Fatalf("severity = %s, want hint", entry.Severity)
	}
	if entry.Message != "Use customer.command.capture_v2." {
		t.Fatalf("message = %q", entry.Message)
	}
}

func TestGroupDeprecationReportEntriesBySeverityIsDeterministic(t *testing.T) {
	t.Parallel()

	entries := []diagnostics.DeprecationReportEntry{
		{Surface: "warn-b", Severity: diagnostics.SeverityWarning, Location: diagnostics.DeprecationLocation{Path: "b.lzi", Line: 1}},
		{Surface: "hint", Severity: diagnostics.SeverityHint, Location: diagnostics.DeprecationLocation{Path: "h.lzi", Line: 1}},
		{Surface: "warn-a", Severity: diagnostics.SeverityWarning, Location: diagnostics.DeprecationLocation{Path: "a.lzi", Line: 1}},
		{Surface: "unknown", Severity: diagnostics.Severity(99), Location: diagnostics.DeprecationLocation{Path: "u.lzi", Line: 1}},
		{Surface: "error", Severity: diagnostics.SeverityError, Location: diagnostics.DeprecationLocation{Path: "e.lzi", Line: 1}},
	}
	original := append([]diagnostics.DeprecationReportEntry(nil), entries...)

	groups := diagnostics.GroupDeprecationReportEntriesBySeverity(entries)
	if !reflect.DeepEqual(entries, original) {
		t.Fatal("GroupDeprecationReportEntriesBySeverity mutated input")
	}

	if keys := deprecationTestGroupKeys(groups); !reflect.DeepEqual(keys, []string{"error", "warning", "hint", "unknown"}) {
		t.Fatalf("group keys = %v", keys)
	}
	if surfaces := deprecationTestGroupSurfaces(groups[1]); !reflect.DeepEqual(surfaces, []string{"warn-a", "warn-b"}) {
		t.Fatalf("warning group surfaces = %v", surfaces)
	}
}

func TestDeprecationDeadlineStatusString(t *testing.T) {
	t.Parallel()

	cases := []struct {
		status diagnostics.DeprecationDeadlineStatus
		want   string
	}{
		{status: "", want: "none"},
		{status: diagnostics.DeprecationDeadlineNone, want: "none"},
		{status: diagnostics.DeprecationDeadlinePending, want: "pending"},
		{status: diagnostics.DeprecationDeadlinePassed, want: "passed"},
		{status: diagnostics.DeprecationDeadlineInvalid, want: "invalid"},
		{status: diagnostics.DeprecationDeadlineStatus("late"), want: "unknown"},
	}
	for _, tc := range cases {
		if got := tc.status.String(); got != tc.want {
			t.Fatalf("DeprecationDeadlineStatus(%q).String() = %q, want %q", tc.status, got, tc.want)
		}
	}
}

type deprecationEntrySummary struct {
	surface     string
	severity    diagnostics.Severity
	path        string
	line        int
	column      int
	since       string
	replacement string
	deadline    string
	status      diagnostics.DeprecationDeadlineStatus
}

func deprecationTestEntrySummaries(entries []diagnostics.DeprecationReportEntry) []deprecationEntrySummary {
	summaries := make([]deprecationEntrySummary, 0, len(entries))
	for _, entry := range entries {
		summaries = append(summaries, deprecationEntrySummary{
			surface:     entry.Surface,
			severity:    entry.Severity,
			path:        entry.Location.Path,
			line:        entry.Location.Line,
			column:      entry.Location.Column,
			since:       entry.Since,
			replacement: entry.Replacement,
			deadline:    entry.Deadline,
			status:      entry.DeadlineStatus,
		})
	}
	return summaries
}

func deprecationTestGroupKeys(groups []diagnostics.DeprecationReportGroup) []string {
	keys := make([]string, 0, len(groups))
	for _, group := range groups {
		keys = append(keys, group.Key)
	}
	return keys
}

func deprecationTestGroupSurfaces(group diagnostics.DeprecationReportGroup) []string {
	surfaces := make([]string, 0, len(group.Entries))
	for _, entry := range group.Entries {
		surfaces = append(surfaces, entry.Surface)
	}
	return surfaces
}
