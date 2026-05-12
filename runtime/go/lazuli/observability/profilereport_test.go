package observability

import (
	"reflect"
	"testing"
	"time"
)

func TestBuildProfileReportGroupsSamplesByOpLabels(t *testing.T) {
	samples := []ProfileSample{
		{
			Labels: map[string]string{
				ProfileLabelFeature:        "customer",
				ProfileLabelKind:           "command",
				ProfileLabelOp:             "create_customer",
				ProfileLabelPatternID:      "command_pgx_insert",
				ProfileLabelPatternVersion: "v1",
			},
			CPUDuration:   10 * time.Millisecond,
			AllocDuration: 4 * time.Millisecond,
			BlockDuration: time.Millisecond,
		},
		{
			Feature:       "customer",
			Kind:          "command",
			Op:            "create_customer",
			PatternID:     "command_pgx_insert",
			CPUDuration:   5 * time.Millisecond,
			AllocDuration: 2 * time.Millisecond,
			BlockDuration: 3 * time.Millisecond,
		},
		{
			Labels: map[string]string{
				ProfileLabelFeature: "invoice",
				ProfileLabelKind:    "query",
				ProfileLabelOp:      "list",
			},
			CPUDuration:   20 * time.Millisecond,
			AllocDuration: time.Millisecond,
			BlockDuration: 2 * time.Millisecond,
		},
		{
			Labels: map[string]string{
				ProfileLabelFeature: "worker",
				ProfileLabelKind:    "job",
			},
			CPUDuration:   7 * time.Millisecond,
			AllocDuration: 8 * time.Millisecond,
			BlockDuration: 9 * time.Millisecond,
		},
	}

	report := BuildProfileReport(samples, 2)

	wantTotal := ProfileTotals{
		SampleCount:   4,
		CPUDuration:   42 * time.Millisecond,
		AllocDuration: 15 * time.Millisecond,
		BlockDuration: 15 * time.Millisecond,
	}
	if report.Total != wantTotal {
		t.Fatalf("total = %+v, want %+v", report.Total, wantTotal)
	}

	wantUnattributed := ProfileTotals{
		SampleCount:   1,
		CPUDuration:   7 * time.Millisecond,
		AllocDuration: 8 * time.Millisecond,
		BlockDuration: 9 * time.Millisecond,
	}
	if report.Unattributed != wantUnattributed {
		t.Fatalf("unattributed = %+v, want %+v", report.Unattributed, wantUnattributed)
	}

	wantOps := []ProfileOpReport{
		{
			Feature:        "customer",
			Kind:           "command",
			Op:             "create_customer",
			PatternID:      "command_pgx_insert",
			PatternVersion: "v1",
			SampleCount:    2,
			CPUDuration:    15 * time.Millisecond,
			AllocDuration:  6 * time.Millisecond,
			BlockDuration:  4 * time.Millisecond,
		},
		{
			Feature:       "invoice",
			Kind:          "query",
			Op:            "list",
			SampleCount:   1,
			CPUDuration:   20 * time.Millisecond,
			AllocDuration: time.Millisecond,
			BlockDuration: 2 * time.Millisecond,
		},
	}
	profileReportAssertRows(t, report.Ops, wantOps)
}

func TestBuildProfileReportRanksTopNByEachDuration(t *testing.T) {
	report := BuildProfileReport([]ProfileSample{
		profileReportSample("billing", "command", "charge", 30, 3, 9),
		profileReportSample("customer", "command", "create", 10, 20, 4),
		profileReportSample("invoice", "query", "list", 20, 8, 30),
	}, 2)

	profileReportAssertNames(t, report.TopCPU, []string{
		"billing.command.charge",
		"invoice.query.list",
	})
	profileReportAssertNames(t, report.TopAlloc, []string{
		"customer.command.create",
		"invoice.query.list",
	})
	profileReportAssertNames(t, report.TopBlock, []string{
		"invoice.query.list",
		"billing.command.charge",
	})
}

func TestRankProfileOpsSortsTiesDeterministicallyAndCopies(t *testing.T) {
	ops := []ProfileOpReport{
		{Feature: "zeta", Kind: "query", Op: "list", CPUDuration: time.Second},
		{Feature: "alpha", Kind: "command", Op: "create", CPUDuration: time.Second},
		{Feature: "alpha", Kind: "command", Op: "archive", CPUDuration: time.Second},
	}

	ranked := RankProfileOps(ops, ProfileMetricCPU, 10)

	profileReportAssertNames(t, ranked, []string{
		"alpha.command.archive",
		"alpha.command.create",
		"zeta.query.list",
	})
	profileReportAssertNames(t, ops, []string{
		"zeta.query.list",
		"alpha.command.create",
		"alpha.command.archive",
	})
}

func TestRankProfileOpsLimitsAndRejectsInvalidMetric(t *testing.T) {
	ops := []ProfileOpReport{
		{Feature: "alpha", Kind: "command", Op: "create", CPUDuration: time.Second},
		{Feature: "beta", Kind: "query", Op: "list", CPUDuration: 2 * time.Second},
	}

	ranked := RankProfileOps(ops, ProfileMetricCPU, 1)
	profileReportAssertNames(t, ranked, []string{"beta.query.list"})

	if got := RankProfileOps(ops, ProfileMetric("unknown"), 1); got != nil {
		t.Fatalf("invalid metric rows = %+v, want nil", got)
	}
	if got := RankProfileOps(ops, ProfileMetricCPU, 0); got != nil {
		t.Fatalf("zero top N rows = %+v, want nil", got)
	}
}

func profileReportSample(feature, kind, op string, cpuMS, allocMS, blockMS int) ProfileSample {
	return ProfileSample{
		Labels: map[string]string{
			ProfileLabelFeature: feature,
			ProfileLabelKind:    kind,
			ProfileLabelOp:      op,
		},
		CPUDuration:   time.Duration(cpuMS) * time.Millisecond,
		AllocDuration: time.Duration(allocMS) * time.Millisecond,
		BlockDuration: time.Duration(blockMS) * time.Millisecond,
	}
}

func profileReportAssertRows(t *testing.T, got, want []ProfileOpReport) {
	t.Helper()

	if !reflect.DeepEqual(got, want) {
		t.Fatalf("rows = %+v, want %+v", got, want)
	}
}

func profileReportAssertNames(t *testing.T, rows []ProfileOpReport, want []string) {
	t.Helper()

	got := make([]string, 0, len(rows))
	for _, row := range rows {
		got = append(got, row.Name())
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("names = %v, want %v", got, want)
	}
}
